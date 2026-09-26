//! U8 `gpu-host-stack` — QUARANTINED adapter.
//!
//! Wires the frozen [`GpuCmd`] contract to the EXISTING real GPU stack:
//! `virtio_gpu_bridge` (typed command executor + binary wire parser),
//! `gles2wgpu` (GLES2 → WGSL translation / GL context), `webgpu_compositor`
//! (layer composition) and `webgpu_swapchain` (presentation).
//!
//! This crate does NOT reimplement translation, composition, or presentation.
//! It only *names* what the host must do, via [`HostAction`].
//!
//! # Quarantine boundary
//!
//! - [`dispatch`] is PURE: `&GpuCmd -> HostAction`. No I/O, no globals, no
//!   hidden state. It never constructs a `VirtioGpuBridge`, a `GlContext`, a
//!   `WebGpuCompositor`, a `WebGpuSwapchain`, or a `wgpu::Device`.
//! - Every side effect (bridge mutation, wgpu device use, surface present)
//!   happens OUTSIDE this crate, in the U13 browser adapter, which
//!   interprets [`HostAction`].
//! - WebGPU surface access exists only behind the U13 adapter (LLD U8/U13).
//!
//! # Intended U13 flow per frame (documented, not implemented here)
//!
//! 1. `BridgeCommand(cmd)` → `VirtioGpuBridge::execute_command(cmd)`.
//! 2. `Submit3DWire { ctx_id, commands }` → encode as
//!    `VIRTIO_GPU_CMD_SUBMIT_3D` and feed to
//!    `VirtioGpuBridge::process_binary_wire_command`; the bridge's
//!    `execute_submit_3d` decodes the mini opcode stream (CLEAR / DRAW_ARRAYS
//!    / DRAW_ELEMENTS / VIEWPORT) onto `GlContext` (`gles2wgpu`).
//! 3. After scanout setup, read pixels with
//!    `VirtioGpuBridge::get_scanout_framebuffer`, upload to a compositor
//!    layer (`WebGpuCompositor::add_or_update_layer`), compose, and present
//!    via `WebGpuSwapchain::present`.
//! 4. `SignalFence { id }` → encode a fence-completion response with
//!    `BinaryWireParser::encode_header_response(_, id, VIRTIO_GPU_FLAG_FENCE)`.
//!
//! # Honest caveats (verified against `crates/virtio_gpu_bridge/src/bridge.rs`)
//!
//! - The *typed* `execute_command` catch-all currently returns `OK_NODATA`
//!   without executing `Submit3D`; the real 3D executor is the binary wire
//!   path. Hence [`HostAction::Submit3DWire`], not a typed submit.
//! - `GpuCmd` carries no scanout id, so `Scanout` targets scanout 0
//!   (single-display guest; multi-scanout is future work).
//! - `GpuCmd` has no flush variant; scanout framebuffer refresh is driven by
//!   the adapter (bridge updates scanout fb on `ResourceFlush`).

use pathn_contracts::device::GpuCmd;
use virtio_gpu_bridge::command::GpuCommand;

/// What the host (U13 browser adapter) must do to service one [`GpuCmd`].
///
/// Describes the action only; the host performs it. Note: `PartialEq` is
/// intentionally NOT derived — `GpuCommand` (owned by `crates/*`, read-only
/// for this unit) does not implement it. Tests destructure instead of
/// comparing for equality.
#[derive(Debug, Clone)]
pub enum HostAction {
    /// Feed the typed command to `VirtioGpuBridge::execute_command`.
    ///
    /// Real backend path for the 2D commands this unit emits
    /// (`TransferToHost2D`, `SetScanout`): the bridge copies into resource
    /// backing data / registers the scanout and returns a status response.
    BridgeCommand(GpuCommand),
    /// Submit a 3D command buffer to the bridge through the binary wire path.
    ///
    /// The adapter encodes `commands` as `VIRTIO_GPU_CMD_SUBMIT_3D` and calls
    /// `VirtioGpuBridge::process_binary_wire_command`; `execute_submit_3d`
    /// runs the opcode stream on `GlContext` (`gles2wgpu`), which translates
    /// GLES2 calls toward WebGPU. This is the real path — the typed
    /// `execute_command` currently no-ops `Submit3D` (see module docs).
    Submit3DWire { ctx_id: u32, commands: Vec<u8> },
    /// Complete the virtio-gpu fence `id`.
    ///
    /// The adapter encodes a fence-completion response via
    /// `BinaryWireParser::encode_header_response` with `fence_id = id`.
    SignalFence { id: u64 },
    /// No backend path exists in the existing crates. `reason` names exactly
    /// what is missing. The adapter must surface this — never silently drop
    /// the command.
    ///
    /// Currently no `GpuCmd` variant maps here; the variant exists so future
    /// contract variants fail loudly instead of vanishing.
    Unsupported {
        cmd: &'static str,
        reason: &'static str,
    },
}

/// Pure mapping from the frozen guest GPU command to the host action.
///
/// Total over [`GpuCmd`]: every variant maps to exactly one action; nothing
/// is silently dropped and nothing is faked (see [`HostAction`] docs for the
/// per-variant backend path).
pub fn dispatch(cmd: &GpuCmd) -> HostAction {
    match cmd {
        GpuCmd::Transfer2D {
            resource_id,
            x,
            y,
            w,
            h,
            data,
        } => HostAction::BridgeCommand(GpuCommand::TransferToHost2D {
            resource_id: *resource_id,
            x: *x,
            y: *y,
            width: *w,
            height: *h,
            offset: 0,
            data: data.clone(),
        }),
        GpuCmd::Submit3D { ctx_id, commands } => HostAction::Submit3DWire {
            ctx_id: *ctx_id,
            commands: commands.clone(),
        },
        GpuCmd::Scanout { resource_id, w, h } => {
            HostAction::BridgeCommand(GpuCommand::SetScanout {
                scanout_id: 0,
                resource_id: *resource_id,
                x: 0,
                y: 0,
                width: *w,
                height: *h,
            })
        }
        GpuCmd::Fence { id } => HostAction::SignalFence { id: *id },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_cmd() -> GpuCmd {
        GpuCmd::Transfer2D {
            resource_id: 7,
            x: 1,
            y: 2,
            w: 3,
            h: 4,
            data: vec![9, 8, 7],
        }
    }

    fn submit3d_cmd() -> GpuCmd {
        GpuCmd::Submit3D {
            ctx_id: 42,
            commands: vec![0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00],
        }
    }

    fn scanout_cmd() -> GpuCmd {
        GpuCmd::Scanout {
            resource_id: 7,
            w: 800,
            h: 600,
        }
    }

    fn fence_cmd() -> GpuCmd {
        GpuCmd::Fence { id: 1234 }
    }

    /// `GpuCommand` has no `PartialEq`; compare structurally via `Debug`.
    fn action_debug(a: &HostAction) -> String {
        format!("{a:?}")
    }

    #[test]
    fn wiring_transfer2d_maps_to_bridge_transfer_with_blit_params() {
        let before = transfer_cmd();
        match dispatch(&before) {
            HostAction::BridgeCommand(GpuCommand::TransferToHost2D {
                resource_id,
                x,
                y,
                width,
                height,
                offset,
                data,
            }) => {
                assert_eq!(resource_id, 7);
                assert_eq!((x, y, width, height), (1, 2, 3, 4));
                assert_eq!(offset, 0);
                assert_eq!(data, vec![9, 8, 7]);
            }
            other => panic!("wrong HostAction for Transfer2D: {other:?}"),
        }
        // Pure: input untouched.
        assert_eq!(before, transfer_cmd());
    }

    #[test]
    fn wiring_submit3d_maps_to_wire_submit_with_exact_bytes() {
        let before = submit3d_cmd();
        match dispatch(&before) {
            HostAction::Submit3DWire { ctx_id, commands } => {
                assert_eq!(ctx_id, 42);
                assert_eq!(
                    commands,
                    vec![0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00]
                );
            }
            other => panic!("wrong HostAction for Submit3D: {other:?}"),
        }
        assert_eq!(before, submit3d_cmd());
    }

    #[test]
    fn wiring_submit3d_empty_buffer_still_maps_without_panic() {
        let cmd = GpuCmd::Submit3D {
            ctx_id: 1,
            commands: vec![],
        };
        match dispatch(&cmd) {
            HostAction::Submit3DWire { ctx_id, commands } => {
                assert_eq!(ctx_id, 1);
                assert!(commands.is_empty());
            }
            other => panic!("wrong HostAction for empty Submit3D: {other:?}"),
        }
    }

    #[test]
    fn wiring_scanout_maps_to_set_scanout_on_primary_display() {
        let before = scanout_cmd();
        match dispatch(&before) {
            HostAction::BridgeCommand(GpuCommand::SetScanout {
                scanout_id,
                resource_id,
                x,
                y,
                width,
                height,
            }) => {
                assert_eq!(scanout_id, 0, "single-display guest targets scanout 0");
                assert_eq!(resource_id, 7);
                assert_eq!((x, y), (0, 0));
                assert_eq!((width, height), (800, 600));
            }
            other => panic!("wrong HostAction for Scanout: {other:?}"),
        }
        assert_eq!(before, scanout_cmd());
    }

    #[test]
    fn wiring_fence_maps_to_signal_fence_with_id() {
        let before = fence_cmd();
        match dispatch(&before) {
            HostAction::SignalFence { id } => assert_eq!(id, 1234),
            other => panic!("wrong HostAction for Fence: {other:?}"),
        }
        assert_eq!(before, fence_cmd());
    }

    #[test]
    fn wiring_dispatch_is_deterministic() {
        let cmds = vec![transfer_cmd(), submit3d_cmd(), scanout_cmd(), fence_cmd()];
        for cmd in &cmds {
            let a = action_debug(&dispatch(cmd));
            let b = action_debug(&dispatch(cmd));
            assert_eq!(a, b, "dispatch must be deterministic for {cmd:?}");
        }
    }

    #[test]
    fn wiring_every_variant_maps_to_a_real_action_never_unsupported() {
        // Totality: all four frozen GpuCmd variants must map to a backend
        // action. If a future variant maps to Unsupported, this test names it.
        let cmds = vec![transfer_cmd(), submit3d_cmd(), scanout_cmd(), fence_cmd()];
        for cmd in &cmds {
            if let HostAction::Unsupported { cmd: name, reason } = dispatch(cmd) {
                panic!("{cmd:?} mapped to Unsupported ({name}: {reason})");
            }
        }
    }

    #[test]
    fn wiring_unsupported_variant_is_explicit_not_silent() {
        // The honest-failure path itself is tested: it carries the command
        // name and the exact missing piece, so the adapter can surface it.
        let a = HostAction::Unsupported {
            cmd: "FutureCmd",
            reason: "no backend path in crates/* yet",
        };
        match &a {
            HostAction::Unsupported { cmd, reason } => {
                assert_eq!(*cmd, "FutureCmd");
                assert!(!reason.is_empty());
            }
            _ => panic!("expected Unsupported"),
        }
        // Debug/Clone so the adapter can log and forward it.
        let b = a.clone();
        assert_eq!(action_debug(&a), action_debug(&b));
    }
}
