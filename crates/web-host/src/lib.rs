//! Path N browser host — the wasm-bindgen seam between the DOM and the
//! real `pathn-sh` guest.
//!
//! Ownership split (per `docs/architecture/LLD-browser-host.md`):
//! - Rust owns the emulator: [`Orchestrator`] (with the wasm32-compatible
//!   [`WasmiExecutor`] backend), the real guest image built by `guest-image`,
//!   and the DOM-keyboard byte queue ([`KeyboardInput`]).
//! - JavaScript owns the DOM terminal, the `keydown`/`keyup` listeners, and
//!   the animation-frame stepping loop.
//!
//! The constructor boots the REAL guest and asserts the exact boot evidence
//! (`HaltReason::Wfi { addr: 0x4000_0070 }`, TX `b"pathn-sh> "`). Any boot
//! divergence fails the constructor instead of rendering a fake shell.

use guest_image::image::{build, GuestManifest};
use pathn_contracts::execution::BlockExecutor;
use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};
use u13_adapters::KeyboardInput;
use u15_exec_wasmi::WasmiExecutor;
use wasm_bindgen::prelude::*;

/// Steps executed per animation frame when the guest has pending work.
/// Small enough to keep the tab responsive, large enough that a typed
/// command resolves within a frame or two.
const STEPS_PER_FRAME: u32 = 4_096;

/// Exact boot evidence the constructor asserts — the same pins as the
/// native acceptance tests (`units/u12-orchestrator/tests/boot.rs`).
const BOOT_WFI_ADDR: u64 = 0x4000_0070;
const BOOT_PROMPT: &[u8] = b"pathn-sh> ";

/// The browser-facing shell: emulator state owned by Rust, rendering and
/// event capture owned by JavaScript.
#[wasm_bindgen]
pub struct PathnShell {
    orch: Orchestrator,
    kbd: KeyboardInput,
    /// Bytes of `console.tx_bytes` already handed to JavaScript.
    tx_drained: usize,
}

#[wasm_bindgen]
impl PathnShell {
    /// Boot the real guest. Fails closed: any divergence from the exact
    /// boot evidence (park address, prompt bytes) returns an error instead
    /// of a shell that only looks alive.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<PathnShell, JsValue> {
        console_error_panic_hook::set_once();
        let manifest = GuestManifest {
            name: "pathn-sh".to_string(),
            version: 1,
            load_addr: RAM_BASE,
        };
        let (img, _sbom) = build(&manifest);
        let executor: Box<dyn BlockExecutor> = Box::new(WasmiExecutor::new());
        let mut orch = Orchestrator::with_executor(executor);
        orch.load_image(&img)
            .map_err(|e| JsValue::from_str(&format!("load_image: {e:?}")))?;
        match orch.run_until_halt(10_000) {
            HaltReason::Wfi { addr } if addr == BOOT_WFI_ADDR => {}
            other => {
                return Err(JsValue::from_str(&format!(
                    "boot did not park at the prompt (WFI {BOOT_WFI_ADDR:#x}): {other:?}"
                )))
            }
        }
        let tx = &orch.console().tx_bytes;
        if tx != BOOT_PROMPT {
            return Err(JsValue::from_str(&format!(
                "boot prompt mismatch: {:?}",
                String::from_utf8_lossy(tx)
            )));
        }
        let tx_drained = tx.len();
        Ok(Self {
            orch,
            kbd: KeyboardInput::new(),
            tx_drained,
        })
    }

    /// Feed one DOM keyboard event into the byte queue. Called from the
    /// JS `keydown`/`keyup` listeners with the event's `code`, `key`, and
    /// pressed state. Key-up events and unmapped keys produce no bytes
    /// (pinned by the native `KeyboardInput` tests).
    pub fn push_key(&mut self, code: &str, key: &str, pressed: bool) {
        self.kbd.push_event(code, key, pressed);
    }

    /// Run the guest for up to one frame's worth of steps and return the
    /// NEW console TX bytes since the last call (UTF-8 lossy for display;
    /// input bytes are preserved end to end — the console path is UTF-8
    /// clean, only the pathn-sh builtins stay ASCII).
    ///
    /// Stops early when the guest parks at WFI (nothing more to do until
    /// new input arrives) or halts for real.
    pub fn step_frame(&mut self) -> String {
        // Host input seam: queued DOM key bytes -> console RX FIFO.
        self.orch.pump_input(&mut self.kbd);
        for _ in 0..STEPS_PER_FRAME {
            match self.orch.step_vcpu() {
                StepOutcome::Halted(_) => break,
                // Parked waiting for input: further steps just re-yield.
                StepOutcome::WfiYield { .. } => break,
                StepOutcome::Continue => {}
            }
        }
        let tx = &self.orch.console().tx_bytes;
        let from = self.tx_drained.min(tx.len());
        let new = &tx[from..];
        self.tx_drained = tx.len();
        String::from_utf8_lossy(new).into_owned()
    }

    /// Total vCPU blocks executed so far (diagnostics / acceptance).
    pub fn steps(&self) -> u32 {
        self.orch.steps().min(u32::MAX as u64) as u32
    }

    /// True while the guest is alive and parked at WFI waiting for input.
    /// WFI parking never poisons `halted` — only a real halt does.
    pub fn parked(&self) -> bool {
        self.orch.halted().is_none()
    }

    /// The exact console TX bytes observed so far (acceptance hook).
    pub fn tx_text(&self) -> String {
        String::from_utf8_lossy(&self.orch.console().tx_bytes).into_owned()
    }
}

/// The browser-facing GPU adapter: owns the WebGPU canvas and drains the
/// shell's GPU submit queue through it. wasm32-only — the U13 canvas module
/// (real `virtio_gpu_bridge` → `gles2wgpu` → WebGPU) does not exist natively.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct GpuCanvas {
    inner: u13_adapters::webgpu_canvas::WebGpuCanvas,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl GpuCanvas {
    /// Boot the bridge and bind it to `canvas_id`. Async because WebGPU
    /// device acquisition awaits the browser. Fails loudly on a missing
    /// canvas, missing WebGPU, or an incompatible canvas format — never
    /// half-initialised.
    pub async fn create(canvas_id: &str) -> Result<GpuCanvas, JsValue> {
        let inner = u13_adapters::webgpu_canvas::WebGpuCanvas::new(canvas_id)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Self { inner })
    }

    /// Drain the shell's pending GPU submit (if any) and execute every
    /// decoded command through U8 dispatch on the canvas bridge, WITHOUT
    /// presenting. Returns the number of commands executed. Used to isolate
    /// render vs present failures.
    pub fn execute_pending(&mut self, shell: &mut PathnShell) -> Result<u32, JsValue> {
        let sub = match shell.orch.drain_gpu_submit() {
            Some(s) => s,
            None => return Ok(0),
        };
        if !sub.decode_errors.is_empty() {
            return Err(JsValue::from_str(&format!(
                "GPU submit had U7 decode errors at offsets {:?}; \
                 refusing to render a partial stream",
                sub.decode_errors
            )));
        }
        let n = sub.commands.len() as u32;
        for cmd in &sub.commands {
            self.inner
                .execute_cmd(cmd)
                .map_err(|e| JsValue::from_str(&e))?;
        }
        Ok(n)
    }

    /// Present the bridge's render target to the canvas surface.
    ///
    /// NOTE: headless Chromium's Dawn loses the device on
    /// `get_current_texture()`; headless flows must use `execute_pending` +
    /// `readback` instead and never call this.
    pub fn present_canvas(&mut self) -> Result<(), JsValue> {
        self.inner.present().map_err(|e| JsValue::from_str(&e))
    }

    /// Drain the shell's pending GPU submit (if any), execute every decoded
    /// command through U8 dispatch on the canvas bridge, then present.
    /// Returns the number of commands executed.
    ///
    /// U7 decode errors fail the call — they are surfaced, never silently
    /// dropped (U12's `GpuSubmit` contract).
    pub fn pump(&mut self, shell: &mut PathnShell) -> Result<u32, JsValue> {
        let n = self.execute_pending(shell)?;
        if n > 0 {
            self.present_canvas()?;
        }
        Ok(n)
    }

    /// Read back the bridge's 3D render target as 640×480 RGBA8 bytes —
    /// the ground-truth pixel assertion path (headless screenshots do not
    /// composite WebGPU canvases).
    pub async fn readback(&self) -> Result<js_sys::Uint8Array, JsValue> {
        self.inner
            .readback()
            .await
            .map_err(|e| JsValue::from_str(&e))
    }
}
