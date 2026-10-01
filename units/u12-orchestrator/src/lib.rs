//! U12 — the single quarantined orchestrator (LLD §12).
//!
//! This crate owns [`Orchestrator`], which threads the explicit [`MachineState`]
//! through every unit. It is the ONLY unit allowed threads and a clock — and it
//! uses neither: the clock is *injected* ([`Orchestrator::tick_clock`]), so the
//! deterministic path never reads wall time.
//!
//! ## vCPU execution model (honest)
//!
//! One step: fetch word at `pc` → U1 decode → U2 lift → U3 compile → execute
//! the WASM block through the injected [`BlockExecutor`] backend (wasmtime on
//! native, wasmi on wasm32 — see `pathn_contracts::execution`).
//!
//! U3's module shape (contract `pathn_contracts::wasm_abi`, Wave-4 U3-G1):
//! imports `env.mem_load` / `env.mem_store` / `env.wfi` plus 31 mutable `i64`
//! globals `env.r0` … `env.r30` (X0–X30), and exports `run() -> i64`. The i64
//! result is the exit address (`-1` = ExitVm sentinel). The orchestrator:
//!
//! - **Threads register state across calls.** Before each call it writes the
//!   checkpointed X0–X30 into the globals; after the call it reads them back
//!   into `machine.cpu[0].regs`. Register 31 stays XZR (const 0 / dropped);
//!   U2's scratch index 32 is U3's private local.
//! - **Serves memory and console through host functions.** `mem_load` /
//!   `mem_store` dispatch console MMIO (`CONSOLE_TX`/`CONSOLE_RX`) to the
//!   [`ConsoleState`] and everything else through the RAM bounds check
//!   ([`Orchestrator::ram_offset`]); faults become WASM traps, which halt
//!   with [`HaltReason::WasmTrap`]. Guest RAM stays host-owned —
//!   `MachineState.ram` is the single owner.
//! - **Yields on WFI.** The `wfi` import only records that the block executed
//!   WFI. After the call, with no IRQ pending, the step returns
//!   [`StepOutcome::WfiYield`] WITHOUT setting `halted` — the vCPU is parked,
//!   resumable by a later step (e.g. after input arrives). Interrupt
//!   *injection* into the guest (vector jump) still needs an exception model —
//!   future work, stated here, not faked.
//! - **Trap ops halt before compiling.** If any lifted op is `IrOp::Trap`,
//!   the orchestrator halts with [`HaltReason::Unsupported`] carrying U2's
//!   exact reason string — it never compiles or executes the trap.
//!
//! ## Console (PLATFORM.md device side)
//!
//! Pure-MMIO console per `guest-image/PLATFORM.md`: `CONSOLE_TX` (STRB → host
//! sink), `CONSOLE_RX` (LDRB → next byte or 0). Explicit-state [`ConsoleState`];
//! no virtio face — the platform contract defines MMIO only.
//!
//! NOTE (Wave-3 boundary): U2 lifts only static-address word accesses;
//! `LDRB`/`STRB` (sub-word) and register-relative addresses trap at lift, so
//! guest code cannot reach the console through the current pipeline. The
//! device model is implemented against PLATFORM.md and tested directly; it
//! becomes guest-reachable when U2's lift set grows (Wave 4).
//!
//! ## Virtio
//!
//! `queue_notify(q)` implements exactly what U6's docs assign to the
//! orchestrator: validate queue → [`u6_virtio_transport::pop_chain`] → device
//! dispatch (U7 gpu) → [`u6_virtio_transport::push_used`] → `IrqAssert` per
//! [`u6_virtio_transport::need_event_idx`]. `avail_event` is read for real
//! from the used ring when EVENT_IDX applies.
//!
//! ## Determinism
//!
//! [`Orchestrator::state_hash`] = sha256 over [`Orchestrator::snapshot_full`]:
//! U11's snapshot of the machine plus a canonical encoding of transport, gpu,
//! console, clock, steps, and halt reason. Same image + same scripted inputs
//! → identical hash, twice. No wall clock, no threads, no FP in the path.

use std::collections::VecDeque;

use pathn_contracts::adapters::{BlobStore, InputSource};
use pathn_contracts::cpu::{
    Access, BlockExit, DecodeResult, InsnKind, Instruction, IrBlock, IrOp, IrqState, MemFault,
    MmuState,
};
use pathn_contracts::device::{DevEvent, DevOut, GpuCmd, GpuDevState, TransportState};
use pathn_contracts::execution::{BlockExecutor, HostOps};
use pathn_contracts::machine::{CpuState, MachineState, SysRegs};
use sha2::{Digest, Sha256};

pub mod arm64;
use arm64::*;

#[cfg(target_arch = "wasm32")]
use u15_exec_wasmi::WasmiExecutor;
#[cfg(not(target_arch = "wasm32"))]
use u15_exec_wasmtime::WasmtimeExecutor;

// ---------------------------------------------------------------------------
// Platform constants (mirrored from guest-image/PLATFORM.md)
// ---------------------------------------------------------------------------

/// Guest RAM base (PLATFORM.md).
pub const RAM_BASE: u64 = 0x4000_0000;
/// Guest RAM size: 128 MiB (PLATFORM.md).
pub const RAM_SIZE: u64 = 0x0800_0000;
/// Console MMIO base (PLATFORM.md).
pub const CONSOLE_BASE: u64 = 0x0900_0000;
/// Console MMIO size: one page (PLATFORM.md).
pub const CONSOLE_SIZE: u64 = 0x1000;
/// STRB a byte here → host emits it (PLATFORM.md).
pub const CONSOLE_TX: u64 = 0x0900_0000;
/// LDRB here → next input byte, or 0 if none (PLATFORM.md).
pub const CONSOLE_RX: u64 = 0x0900_0008;

/// GPU command-stream MMIO base (Track A; PLATFORM.md §GPU port).
/// The guest STRBs one virtio-gpu control-stream byte per write to
/// [`GPU_DATA`], then STRBs [`GPU_SUBMIT`] to hand the accumulated buffer
/// to the host. Byte-wide accesses only, mirroring the console port model.
pub const GPU_BASE: u64 = 0x0A00_0000;
/// GPU MMIO size: one page (PLATFORM.md).
pub const GPU_SIZE: u64 = 0x1000;
/// STRB a command-stream byte here → appended to the GPU port buffer.
pub const GPU_DATA: u64 = 0x0A00_0000;
/// STRB here → the buffered stream is submitted for decode (U7) + dispatch
/// (U8). The write value is ignored; the buffer drains via
/// [`Orchestrator::drain_gpu_submit`].
pub const GPU_SUBMIT: u64 = 0x0A00_0008;

/// Platform IRQ number the orchestrator raises for virtio-gpu completion.
/// Not in the frozen contracts; a board device tree would fix this in M1+.
/// Documented here as the orchestrator's explicit choice.
pub const VIRTIO_GPU_IRQ: u32 = 48;

/// Injected timer cycles advanced per vCPU step. Fixed → deterministic.
pub const TIMER_CYCLES_PER_STEP: u64 = 1000;

/// Image magic `"PNIM"` little-endian (PLATFORM.md).
const IMAGE_MAGIC: u32 = 0x4D49_4E50;
/// Image format version (PLATFORM.md).
const IMAGE_VERSION: u32 = 1;
/// Image header length: magic u32 + version u32 + entry u64 + blob-size u64.
const IMAGE_HEADER_LEN: usize = 24;

// ---------------------------------------------------------------------------
// Errors / halt reasons — data, never panics
// ---------------------------------------------------------------------------

/// Why the vCPU stopped. Every variant is observable and hashable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HaltReason {
    IllegalInstruction {
        addr: u64,
        word: u32,
    },
    Unsupported {
        addr: u64,
        reason: &'static str,
    },
    WasmTrap {
        addr: u64,
        message: String,
    },
    FetchFault {
        addr: u64,
    },
    ExitVm,
    StepLimitExceeded,
    /// WFI executed with no IRQ pending: the vCPU is parked, NOT dead.
    /// Never stored in `Orchestrator.halted` by `step_vcpu` (it returns
    /// `StepOutcome::WfiYield` instead, keeping the machine resumable);
    /// `run_until_halt` surfaces it as its return value. Snapshot tag 7 is
    /// reserved for it (additive — tags 0–6 decode exactly as before).
    Wfi {
        addr: u64,
    },
    /// SVC executed but the exception model does not exist yet (Phase-2
    /// spike, 2026-09-30). Distinct from Unsupported so a future SVC halt is
    /// instantly recognizable in traces. Snapshot tag 8 is reserved for it
    /// (additive - tags 0-7 decode exactly as before).
    Svc { addr: u64 },
}

/// Image load failure — data, not panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    TooShort,
    BadMagic { found: u32 },
    BadVersion { found: u32 },
    BlobTooLarge { need: usize },
    Truncated { need: usize, have: usize },
}

/// Outcome of one vCPU step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Continue,
    Halted(HaltReason),
    /// The block executed WFI with no IRQ pending. The machine is NOT
    /// halted: `pc` already points past the WFI block and the next
    /// `step_vcpu` resumes normally (e.g. after the host feeds input or an
    /// IRQ pends). `run_until_halt` reports this as `HaltReason::Wfi`.
    WfiYield {
        addr: u64,
    },
}

// ---------------------------------------------------------------------------
// Console device — explicit state, PLATFORM.md §Console MMIO
// ---------------------------------------------------------------------------

/// Explicit console device state. `tx_bytes` is the host-observed sink;
/// `rx_queue` feeds guest reads. No hidden state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsoleState {
    pub tx_bytes: Vec<u8>,
    pub rx_queue: VecDeque<u8>,
}

impl ConsoleState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Guest STRB to CONSOLE_TX.
    pub fn write_tx(&mut self, byte: u8) {
        self.tx_bytes.push(byte);
    }

    /// Guest LDRB from CONSOLE_RX: next byte, or 0 when empty (PLATFORM.md).
    pub fn read_rx(&mut self) -> u8 {
        self.rx_queue.pop_front().unwrap_or(0)
    }

    /// Host feeds input bytes (test drives this; prod wires an InputSource).
    pub fn feed_rx(&mut self, bytes: &[u8]) {
        self.rx_queue.extend(bytes.iter().copied());
    }

    /// Drain polled input events into the RX queue (one byte per Key event
    /// with code < 256; other events are not text and are skipped — the
    /// byte-console only carries text).
    pub fn poll_input(&mut self, src: &mut dyn InputSource) {
        use pathn_contracts::adapters::NormalizedInput;
        for ev in src.poll() {
            if let NormalizedInput::Key {
                code,
                pressed: true,
            } = ev
            {
                if code.0 < 256 {
                    self.rx_queue.push_back(code.0 as u8);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GPU command-stream port (Track A)
// ---------------------------------------------------------------------------

/// Host-side receive buffer for the guest's virtio-gpu control stream.
///
/// The guest speaks bytes: one STRB per stream byte to [`GPU_DATA`], one
/// STRB to [`GPU_SUBMIT`] when the stream is complete. This struct is the
/// byte pipe — decoding (U7) and dispatch (U8) happen in
/// [`Orchestrator::drain_gpu_submit`], so the port itself stays as dumb as
/// the console TX byte queue. Never silently drops: a submit with an empty
/// buffer still sets the pending flag, and [`GpuPort::take_submit`] always
/// clears both buffer and flag.
#[derive(Debug, Default)]
pub struct GpuPort {
    stream: Vec<u8>,
    submit_pending: bool,
}

impl GpuPort {
    /// Empty port, no submit pending.
    pub fn new() -> Self {
        Self {
            stream: Vec::new(),
            submit_pending: false,
        }
    }

    /// Append one stream byte (guest STRB to [`GPU_DATA`]).
    pub fn push_byte(&mut self, b: u8) {
        self.stream.push(b);
    }

    /// Mark the buffered stream submitted (guest STRB to [`GPU_SUBMIT`]).
    pub fn submit(&mut self) {
        self.submit_pending = true;
    }

    /// True when the guest signalled submit and the stream was not drained.
    pub fn is_pending(&self) -> bool {
        self.submit_pending
    }
    /// Bytes currently buffered (not yet submitted).
    pub fn buffered_len(&self) -> usize {
        self.stream.len()
    }

    /// Take the pending stream for decode. Returns `None` when no submit
    /// is pending; always clears the pending flag and the buffer.
    pub fn take_submit(&mut self) -> Option<Vec<u8>> {
        if !self.submit_pending {
            return None;
        }
        self.submit_pending = false;
        Some(std::mem::take(&mut self.stream))
    }
}

/// A guest-submitted virtio-gpu control stream, decoded into typed commands
/// (U7) and ready for dispatch (U8). Returned by
/// [`Orchestrator::drain_gpu_submit`].
#[derive(Debug)]
pub struct GpuSubmit {
    /// Decoded [`GpuCmd`]s in stream order.
    pub commands: Vec<GpuCmd>,
    /// U7 decode error signals (config-change values) — never silently
    /// dropped; the host adapter must surface them.
    pub decode_errors: Vec<u64>,
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// The single quarantined coordinator. Owns the machine state and threads it
/// through every unit. `engine` and `block_cache` are runtime machinery —
/// explicitly NOT part of the hashed state.
pub struct Orchestrator {
    pub machine: MachineState,
    pub transport: TransportState,
    pub gpu: GpuDevState,
    pub console: ConsoleState,
    /// Guest-to-host virtio-gpu command-stream port (Track A).
    pub gpu_port: GpuPort,
    /// Injected cycle counter — the only "clock". Never wall time.
    pub clock_cycles: u64,
    pub steps: u64,
    pub halted: Option<HaltReason>,
    /// Per-queue last-notified used index for EVENT_IDX decisions.
    last_notified: Vec<u16>,
    /// The WASM execution backend (wasmtime on native, wasmi on wasm32).
    /// Injected — see [`Orchestrator::with_executor`].
    executor: Box<dyn BlockExecutor>,
}

/// Default backend for this target: wasmtime where a JIT is available,
/// wasmi (interpreter) on wasm32 where wasmtime cannot compile.
fn default_executor() -> Box<dyn BlockExecutor> {
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(WasmiExecutor::new())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(WasmtimeExecutor::new())
    }
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            machine: MachineState {
                cpu: vec![CpuState {
                    regs: [0; 31],
                    sp: 0x4800_0000,
                    pc: 0x4000_0000,
                    pstate: 0,
                    sysregs: SysRegs::default(),
                }],
                mmu: MmuState {
                    ttbr0: 0,
                    ttbr1: 0,
                    tcr: 0,
                    sctlr: 0,
                },
                irq: IrqState {
                    enabled: 0,
                    pending: 0,
                    timer_count: 0,
                    timer_compare: 0,
                },
                ram: vec![0; RAM_SIZE as usize],
                devices: vec![],
            },
            transport: TransportState {
                queue_count: 0,
                features: 0,
                status: 0,
                queues: Vec::new(),
            },
            gpu: GpuDevState {
                next_resource_id: 0,
                next_fence_id: 0,
            },
            console: ConsoleState::new(),
            gpu_port: GpuPort::new(),
            clock_cycles: 0,
            steps: 0,
            halted: None,
            last_notified: Vec::new(),
            executor: default_executor(),
        }
    }

    /// Build with an explicit execution backend — the injection point for the
    /// browser host and for backend-parity tests.
    pub fn with_executor(executor: Box<dyn BlockExecutor>) -> Self {
        let mut o = Self::new();
        o.executor = executor;
        o
    }

    /// Was the machine halted?
    pub fn halted(&self) -> Option<&HaltReason> {
        self.halted.as_ref()
    }

    /// Number of vCPU blocks executed.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Number of distinct blocks currently in the JIT cache.
    pub fn block_cache_len(&self) -> usize {
        self.executor.cache_len()
    }

    /// Read-only view of the machine state (U11 snapshot input).
    pub fn machine(&self) -> &MachineState {
        &self.machine
    }

    /// Mutable view for test/bring-up scaffolding (loading test programs).
    pub fn machine_mut(&mut self) -> &mut MachineState {
        &mut self.machine
    }

    /// The MMIO console model.
    pub fn console(&self) -> &ConsoleState {
        &self.console
    }

    /// Mutable console model (feeding scripted input).
    pub fn console_mut(&mut self) -> &mut ConsoleState {
        &mut self.console
    }

    /// Drain one poll of an [`InputSource`](pathn_contracts::adapters::InputSource)
    /// into the console RX FIFO. This is the host input seam: the production
    /// host calls it on its input event loop (browser key events arrive as
    /// `NormalizedInput::Key` and become RX bytes); the guest consumes them
    /// through real `LDRB CONSOLE_RX` reads in `read_char`. Deterministic
    /// under scripted sources (one `poll()` per call).
    pub fn pump_input(&mut self, src: &mut dyn pathn_contracts::adapters::InputSource) {
        self.console.poll_input(src);
    }

    /// The virtio transport state (U6).
    pub fn transport(&self) -> &TransportState {
        &self.transport
    }

    /// Mutable transport state (bring-up wiring).
    pub fn transport_mut(&mut self) -> &mut TransportState {
        &mut self.transport
    }

    /// The GPU device state (U7).
    pub fn gpu(&self) -> &GpuDevState {
        &self.gpu
    }

    /// The guest-to-host GPU command-stream port (Track A).
    pub fn gpu_port(&self) -> &GpuPort {
        &self.gpu_port
    }

    /// Drain a submitted GPU stream: take the pending bytes from the port,
    /// run U7 `step` over them (queue 0 notify), and return the decoded
    /// commands. Returns `None` when no submit is pending. The port buffer
    /// and pending flag are always cleared, even on decode error.
    pub fn drain_gpu_submit(&mut self) -> Option<GpuSubmit> {
        let bytes = self.gpu_port.take_submit()?;
        let (next, outs) =
            u7_gpu_device::step(&self.gpu, DevEvent::QueueNotify { queue_idx: 0 }, &bytes);
        self.gpu = next;
        let mut commands = Vec::new();
        let mut decode_errors = Vec::new();
        for out in outs {
            match out {
                DevOut::GpuCommands(cmds) => commands.extend(cmds),
                DevOut::ConfigValue(v) => decode_errors.push(v),
                // IRQ/ring/packet signals are not part of the byte-port
                // submit path; the guest did not use the transport.
                DevOut::UsedRingUpdate { .. } | DevOut::IrqAssert { .. } | DevOut::NetPacket(_) => {
                }
            }
        }
        Some(GpuSubmit {
            commands,
            decode_errors,
        })
    }

    /// Load a PNIM guest image (PLATFORM.md §Image format) into RAM and set
    /// the entry contract: pc = entry, sp = RAM top, regs/pstate = 0.
    pub fn load_image(&mut self, image: &[u8]) -> Result<(), ImageError> {
        if image.len() < IMAGE_HEADER_LEN {
            return Err(ImageError::TooShort);
        }
        let magic = u32::from_le_bytes(image[0..4].try_into().unwrap());
        if magic != IMAGE_MAGIC {
            return Err(ImageError::BadMagic { found: magic });
        }
        let version = u32::from_le_bytes(image[4..8].try_into().unwrap());
        if version != IMAGE_VERSION {
            return Err(ImageError::BadVersion { found: version });
        }
        let entry = u64::from_le_bytes(image[8..16].try_into().unwrap());
        let blob_size = u64::from_le_bytes(image[16..24].try_into().unwrap()) as usize;
        if blob_size > self.machine.ram.len() {
            return Err(ImageError::BlobTooLarge { need: blob_size });
        }
        if image.len() < IMAGE_HEADER_LEN + blob_size {
            return Err(ImageError::Truncated {
                need: IMAGE_HEADER_LEN + blob_size,
                have: image.len(),
            });
        }
        self.machine.ram[..blob_size]
            .copy_from_slice(&image[IMAGE_HEADER_LEN..IMAGE_HEADER_LEN + blob_size]);
        // Entry contract (PLATFORM.md): pc = entry, sp = RAM top, regs = 0.
        let cpu = &mut self.machine.cpu[0];
        cpu.pc = entry;
        cpu.sp = RAM_BASE + RAM_SIZE;
        cpu.regs = [0; 31];
        cpu.pstate = 0;
        self.invalidate_code_cache();
        Ok(())
    }

    pub fn invalidate_code_cache(&mut self) {
        self.executor.invalidate();
    }

    // ---- memory path (M0: MMU disabled, physical addresses = guest addresses)
    // ----

    /// Translate a guest physical address to a RAM offset. M0 runs with the
    /// MMU disabled, so this is identity-with-bounds-check, documented here.
    /// (When the MMU is enabled in M1+, this routes through U4 `translate`.)
    fn ram_offset(&self, pa: u64, len: u64) -> Result<usize, MemFault> {
        ram_offset_in(&self.machine.ram, pa, len)
    }

    fn fetch_word(&self, pc: u64) -> Result<u32, HaltReason> {
        // GB-22: instruction fetch goes through stage-1 translation, same
        // as GB-20's data accesses. MMU off: identity. MMU on: VA -> PA
        // with Access::Execute (XN/PXN enforced); a translation fault is
        // an honest FetchFault at the faulting VA.
        let pa = self.translate_fetch(pc)?;
        match self.ram_offset(pa, 4) {
            Ok(off) => Ok(u32::from_le_bytes(
                self.machine.ram[off..off + 4].try_into().unwrap(),
            )),
            Err(_) => Err(HaltReason::FetchFault { addr: pc }),
        }
    }

    /// Translate a fetch VA to a guest-physical address. Mirrors
    /// WasmHost::translate_data but reports FetchFault (the fetch path's
    /// own halt reason) instead of a trap string.
    fn translate_fetch(&self, va: u64) -> Result<u64, HaltReason> {
        self.translate_va(va, Access::Execute)
            .map_err(|_| HaltReason::FetchFault { addr: va })
    }

    /// Translate a data-access VA to a guest-physical address for the
    /// fast-path interpreter. MMU off: identity. Faults become WasmTrap
    /// (same surfacing as WasmHost data accesses).
    fn translate_data_orch(&self, va: u64, access: Access) -> Result<u64, HaltReason> {
        self.translate_va(va, access)
            .map_err(|f| HaltReason::WasmTrap {
                addr: va,
                message: format!("mmu_fault: {f:?}"),
            })
    }

    /// Shared VA->PA translation. MMU off (SCTLR_EL1.M == 0): identity.
    fn translate_va(&self, va: u64, access: Access) -> Result<u64, MemFault> {
        let sysregs = &self.machine.cpu[0].sysregs;
        if sysregs.sctlr_el1 & 1 == 0 {
            return Ok(va);
        }
        let st = MmuState {
            sctlr: sysregs.sctlr_el1,
            tcr: sysregs.tcr_el1,
            ttbr0: sysregs.ttbr0_el1,
            ttbr1: sysregs.ttbr1_el1,
        };
        u4_mmu::translate_with_base(&st, &self.machine.ram, RAM_BASE, va, access)
    }

    /// MMIO dispatch per PLATFORM.md. Returns Some(byte) for console reads,
    /// None when the address is not MMIO (caller then uses RAM). Public:
    /// this is the vCPU's device interface for trapped MMIO accesses.
    pub fn mmio_read(&mut self, addr: u64) -> Option<u8> {
        if (CONSOLE_BASE..CONSOLE_BASE + CONSOLE_SIZE).contains(&addr) {
            if addr == CONSOLE_RX {
                Some(self.console.read_rx())
            } else {
                // Defined MMIO region, non-RX offset: reads return 0.
                Some(0)
            }
        } else {
            None
        }
    }

    /// MMIO write per PLATFORM.md. Returns true if the address was MMIO.
    /// Public: this is the vCPU's device interface for trapped MMIO accesses.
    pub fn mmio_write(&mut self, addr: u64, byte: u8) -> bool {
        if (CONSOLE_BASE..CONSOLE_BASE + CONSOLE_SIZE).contains(&addr) {
            if addr == CONSOLE_TX {
                self.console.write_tx(byte);
            }
            // Writes to other console offsets are acknowledged, no effect.
            true
        } else {
            false
        }
    }

    fn read_ram_u64(&self, pa: u64) -> Result<u64, HaltReason> {
        let off = self.ram_offset(pa, 8).map_err(|_| HaltReason::FetchFault { addr: pa })?;
        Ok(u64::from_le_bytes(self.machine.ram[off..off + 8].try_into().unwrap()))
    }

    fn read_ram_u32(&self, pa: u64) -> Result<u32, HaltReason> {
        let off = self.ram_offset(pa, 4).map_err(|_| HaltReason::FetchFault { addr: pa })?;
        Ok(u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap()))
    }

    fn write_ram_u64(&mut self, pa: u64, val: u64) -> Result<(), HaltReason> {
        let off = self.ram_offset(pa, 8).map_err(|_| HaltReason::FetchFault { addr: pa })?;
        self.machine.ram[off..off + 8].copy_from_slice(&val.to_le_bytes());
        Ok(())
    }

    fn write_ram_u32(&mut self, pa: u64, val: u32) -> Result<(), HaltReason> {
        let off = self.ram_offset(pa, 4).map_err(|_| HaltReason::FetchFault { addr: pa })?;
        self.machine.ram[off..off + 4].copy_from_slice(&val.to_le_bytes());
        Ok(())
    }

    /// Direct vCPU execution for branches + flags (Track GB-2).
    ///
    /// Pure state transition on machine.cpu[0] and machine.ram.
    /// Returns `Some(Ok(()))` when the instruction was handled,
    /// `Some(Err(reason))` when execution produced a fault,
    /// or `None` if the instruction should fall through to the WASM lifter.
    pub fn execute_arm64(&mut self, insn: &Instruction) -> Option<Result<(), HaltReason>> {
        let word = insn.word;
        let pc = insn.addr;

        // 1. B.cond: 0101010 0 imm19 0 cond
        if (word >> 24) == 0x54 && (word & 0x10) == 0 {
            let cond = (word & 0xF) as u8;
            if cond < 15 {
                let imm19 = ((word >> 5) & 0x7FFFF) as i32;
                let offset = (((imm19 << 13) >> 13) as i64) * 4;
                let target = (pc as i64).wrapping_add(offset) as u64;
                let taken = condition_holds(cond, self.machine.cpu[0].pstate);
                self.machine.cpu[0].pc = if taken { target } else { pc.wrapping_add(4) };
                return Some(Ok(()));
            }
        }

        // 2. TBZ / TBNZ: b5 011011 op b40 imm14 Rt
        if (word >> 25) & 0x3F == 0b011011 {
            let b5 = (word >> 31) & 1;
            let op = (word >> 24) & 1; // 0 = TBZ, 1 = TBNZ
            let b40 = (word >> 19) & 0x1F;
            let bit_pos = (b5 << 5) | b40;
            let imm14 = ((word >> 5) & 0x3FFF) as i32;
            let offset = (((imm14 << 18) >> 18) as i64) * 4;
            let target = (pc as i64).wrapping_add(offset) as u64;
            let rt = (word & 0x1F) as usize;
            let val = if rt == 31 { 0 } else { self.machine.cpu[0].regs[rt] };
            let bit = (val >> bit_pos) & 1;
            let taken = if op == 0 { bit == 0 } else { bit != 0 };
            self.machine.cpu[0].pc = if taken { target } else { pc.wrapping_add(4) };
            return Some(Ok(()));
        }

        // 3. Add/subtract (immediate) with S=1: ADDS / SUBS / CMP / CMN
        // sf op S 10001 sh imm12 Rn Rd
        // GB-24: Also handle S=0 (plain ADD/SUB). For S=0, Rd=31 means SP
        // (not XZR) — this is how the kernel sets up its stack.
        if (word >> 24) & 0x1F == 0x11 {
            let s = (word >> 29) & 1;
            let sf = (word >> 31) & 1;
            let op = (word >> 30) & 1; // 0 = ADD, 1 = SUB
            let sh = (word >> 22) & 1;
            let imm12 = (word >> 10) & 0xFFF;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;
            let imm = (imm12 as u64) << (if sh == 1 { 12 } else { 0 });
            // Rn=31 means SP for ADD/SUB (both S=0 and S=1).
            let rn_val = if rn == 31 { self.machine.cpu[0].sp } else { self.machine.cpu[0].regs[rn] };
            if s == 1 {
                if sf == 1 {
                    let nzcv = if op == 0 {
                        let res = rn_val.wrapping_add(imm);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                        nzcv_add64(rn_val, imm)
                    } else {
                        let res = rn_val.wrapping_sub(imm);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                        nzcv_sub64(rn_val, imm)
                    };
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let a32 = rn_val as u32;
                    let b32 = imm as u32;
                    let nzcv = if op == 0 {
                        let res = a32.wrapping_add(b32);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                        nzcv_add32(a32, b32)
                    } else {
                        let res = a32.wrapping_sub(b32);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                        nzcv_sub32(a32, b32)
                    };
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            } else {
                // S=0: plain ADD/SUB. Rd=31 means SP (not XZR).
                // This handles stack setup like SUB SP, SP, #imm.
                if sf == 1 {
                    let res = if op == 0 {
                        rn_val.wrapping_add(imm)
                    } else {
                        rn_val.wrapping_sub(imm)
                    };
                    if rd == 31 {
                        self.machine.cpu[0].sp = res;
                    } else {
                        self.machine.cpu[0].regs[rd] = res;
                    }
                } else {
                    let a32 = rn_val as u32;
                    let b32 = imm as u32;
                    let res = if op == 0 {
                        a32.wrapping_add(b32)
                    } else {
                        a32.wrapping_sub(b32)
                    };
                    if rd == 31 {
                        // 32-bit ADD/SUB to SP: zero-extend to 64-bit.
                        self.machine.cpu[0].sp = res as u64;
                    } else {
                        self.machine.cpu[0].regs[rd] = res as u64;
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 4. Add/subtract (shifted register) with S=1: ADDS / SUBS / CMP / CMN
        // sf op S 01011 shift 0 Rm imm6 Rn Rd
        if (word >> 24) & 0x1F == 0x0B {
            let s = (word >> 29) & 1;
            let is_shifted = ((word >> 21) & 1) == 0;
            let shift = ((word >> 22) & 0x3) as u8;
            if s == 1 && is_shifted && shift < 3 {
                let sf = (word >> 31) & 1;
                let op = (word >> 30) & 1; // 0 = ADD, 1 = SUB
                let rm = ((word >> 16) & 0x1F) as usize;
                let imm6 = ((word >> 10) & 0x3F) as u8;
                let rn = ((word >> 5) & 0x1F) as usize;
                let rd = (word & 0x1F) as usize;
                let rn_val = if rn == 31 { self.machine.cpu[0].sp } else { self.machine.cpu[0].regs[rn] };
                let rm_val = if rm == 31 { 0 } else { self.machine.cpu[0].regs[rm] };
                if sf == 1 {
                    let operand2 = eval_shift64(rm_val, shift, imm6);
                    let nzcv = if op == 0 {
                        let res = rn_val.wrapping_add(operand2);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                        nzcv_add64(rn_val, operand2)
                    } else {
                        let res = rn_val.wrapping_sub(operand2);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                        nzcv_sub64(rn_val, operand2)
                    };
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let a32 = rn_val as u32;
                    let b32 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    let nzcv = if op == 0 {
                        let res = a32.wrapping_add(b32);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                        nzcv_add32(a32, b32)
                    } else {
                        let res = a32.wrapping_sub(b32);
                        if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                        nzcv_sub32(a32, b32)
                    };
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 5. Logical (shifted register): ANDS / TST (opc=11) and BIC (opc=00, N=1)
        // sf opc 01010 shift N Rm imm6 Rn Rd
        if (word >> 24) & 0x1F == 0x0A {
            let opc = (word >> 29) & 0x3;
            let shift = ((word >> 22) & 0x3) as u8;
            let n = (word >> 21) & 1;
            let sf = (word >> 31) & 1;
            let rm = ((word >> 16) & 0x1F) as usize;
            let imm6 = ((word >> 10) & 0x3F) as u8;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;

            if opc == 0b11 && shift < 3 {
                // ANDS (TST when rd=31) or BICS
                let rn_val = if rn == 31 { 0 } else { self.machine.cpu[0].regs[rn] };
                let rm_val = if rm == 31 { 0 } else { self.machine.cpu[0].regs[rm] };
                if sf == 1 {
                    let mut op2 = eval_shift64(rm_val, shift, imm6);
                    if n == 1 { op2 = !op2; }
                    let res = rn_val & op2;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                    let nzcv = nzcv_and64(res);
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let mut op2 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    if n == 1 { op2 = !op2; }
                    let res = (rn_val as u32) & op2;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                    let nzcv = nzcv_and32(res);
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
            if opc == 0b00 && n == 1 && shift < 3 {
                // BIC: Rd = Rn & ~shifted(Rm)
                let rn_val = if rn == 31 { 0 } else { self.machine.cpu[0].regs[rn] };
                let rm_val = if rm == 31 { 0 } else { self.machine.cpu[0].regs[rm] };
                if sf == 1 {
                    let op2 = !eval_shift64(rm_val, shift, imm6);
                    let res = rn_val & op2;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                } else {
                    let op2 = !eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    let res = (rn_val as u32) & op2;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
            // GB-25: ORR (opc=01). Rd = Rn | shift(Rm).
            // 32-bit form (sf=0) zeroes upper 32 bits of Rd.
            // Note: for logical ops, Rn/Rm=31 means XZR (not SP).
            if opc == 0b01 && shift < 3 {
                let rn_val = if rn == 31 { 0 } else { self.machine.cpu[0].regs[rn] };
                let rm_val = if rm == 31 { 0 } else { self.machine.cpu[0].regs[rm] };
                if sf == 1 {
                    let op2 = eval_shift64(rm_val, shift, imm6);
                    let res = rn_val | op2;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                } else {
                    let op2 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    let res = (rn_val as u32) | op2;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 6. Logical (immediate): ANDS / TST
        // sf 11 100100 N immr imms Rn Rd
        if ((word >> 23) & 0x3F == 0b100100) && ((word >> 29) & 0x3 == 0b11) {
            let sf = ((word >> 31) & 1) as u8;
            let n = ((word >> 22) & 1) as u8;
            let immr = ((word >> 16) & 0x3F) as u8;
            let imms = ((word >> 10) & 0x3F) as u8;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;
            if let Some(mask) = decode_logical_immediate(sf, n, immr, imms) {
                let rn_val = if rn == 31 { 0 } else { self.machine.cpu[0].regs[rn] };
                if sf == 1 {
                    let res = rn_val & mask;
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res; }
                    let nzcv = nzcv_and64(res);
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let res = (rn_val as u32) & (mask as u32);
                    if rd != 31 { self.machine.cpu[0].regs[rd] = res as u64; }
                    let nzcv = nzcv_and32(res);
                    self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 7. Conditional select: CSEL / CSINC / CSINV / CSNEG (GB-15).
        // sf 00 11010100 Rm cond op(2) Rn Rd -- bits[30:21] == 0xD4.
        // Executed directly like B.cond: the condition reads the live
        // NZCV flags from pstate, which the WASM path cannot see, so
        // no U2/U3 lifting is involved (condition_holds is the shared
        // GB-2 cond-eval helper).
        if (word >> 21) & 0x3FF == 0xD4 {
            let sf = (word >> 31) & 1;
            if sf == 0 {
                return Some(Err(HaltReason::Unsupported {
                    addr: pc,
                    reason: "CSEL: 32-bit form not implemented",
                }));
            }
            let cond = ((word >> 12) & 0xF) as u8;
            if cond == 0xF {
                return Some(Err(HaltReason::Unsupported {
                    addr: pc,
                    reason: "CSEL: cond 0b1111 is unallocated",
                }));
            }
            // op: 00 = CSEL, 01 = CSINC, 10 = CSINV, 11 = CSNEG.
            let op = (word >> 10) & 0x3;
            let rm = ((word >> 16) & 0x1F) as usize;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;
            let cpu = &mut self.machine.cpu[0];
            let m_val = if rm == 31 { 0 } else { cpu.regs[rm] };
            let n_val = if rn == 31 { 0 } else { cpu.regs[rn] };
            let else_val = match op {
                0b00 => m_val,
                0b01 => m_val.wrapping_add(1),
                0b10 => !m_val,
                _ => m_val.wrapping_neg(),
            };
            let val = if condition_holds(cond, cpu.pstate) {
                n_val
            } else {
                else_val
            };
            if rd != 31 {
                cpu.regs[rd] = val;
            }
            cpu.pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 7. SP-relative load/store (unsigned immediate and pair).
        // GB-23: the Wave-4 IR has no SP, so U2 traps these. The fast path
        // handles the common forms directly: Rn=31 reads SP.
        // STR/LDR (unsigned imm): xx111000 Vx imm12 Rn Rt
        //   xx=10 (32-bit) / 11 (64-bit); V=0 (STR) / 1 (LDR)
        // STP/LDP (signed off): 10101001 1x imm7 Rt2 Rn Rt1
        //   x=1 (STP) / 0 (LDP), 64-bit only here
        let op10 = (word >> 22) & 0x3FF;
        let rn = ((word >> 5) & 0x1F) as usize;
        if rn == 31 {
            // STR/LDR unsigned immediate.
            if (op10 & 0x3FC) == 0x3E0 || (op10 & 0x3FC) == 0x2E0 {
                // Size is word bit 30 -> op10 bit 8 (0x100). Bit 31 is 1
                // for both 32-bit and 64-bit unsigned-immediate forms.
                let is64 = (op10 & 0x100) != 0;
                let is_load = (op10 & 0x1) != 0;
                let imm12 = ((word >> 10) & 0xFFF) as u64;
                let rt = (word & 0x1F) as usize;
                let size: u64 = if is64 { 8 } else { 4 };
                let va = self.machine.cpu[0].sp.wrapping_add(imm12 * size);
                let access = if is_load { Access::Read } else { Access::Write };
                let pa = match self.translate_data_orch(va, access) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, size) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "sp-relative access outside RAM".to_string(),
                        }))
                    }
                };
                if is_load {
                    let val = if is64 {
                        u64::from_le_bytes(self.machine.ram[off..off + 8].try_into().unwrap())
                    } else {
                        u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap()) as u64
                    };
                    if rt != 31 {
                        self.machine.cpu[0].regs[rt] = val;
                    }
                } else {
                    let val = if rt == 31 { 0 } else { self.machine.cpu[0].regs[rt] };
                    if is64 {
                        self.machine.ram[off..off + 8].copy_from_slice(&val.to_le_bytes());
                    } else {
                        self.machine.ram[off..off + 4]
                            .copy_from_slice(&(val as u32).to_le_bytes());
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
            // STP/LDP (pair, signed offset). 64-bit: 10101001 1x;
            // 32-bit: 00101001 1x.
            if op10 == 0x2A6 || op10 == 0x2A5 || op10 == 0x0A6 || op10 == 0x0A5 {
                let is_store = (op10 & 0x1) == 0;
                let is64 = (op10 & 0x200) != 0;
                let scale: i64 = if is64 { 8 } else { 4 };
                let imm7 = ((word >> 15) & 0x7F) as i64;
                let imm7 = ((imm7 << 57) >> 57) * scale; // sign-extend, scale
                let rt2 = ((word >> 10) & 0x1F) as usize;
                let rt1 = (word & 0x1F) as usize;
                let va = (self.machine.cpu[0].sp as i64).wrapping_add(imm7) as u64;
                let access = if is_store { Access::Write } else { Access::Read };
                let pa = match self.translate_data_orch(va, access) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                // Pair accesses 2*scale bytes; translate the second half too.
                let va2 = va.wrapping_add(scale as u64);
                let pa2 = match self.translate_data_orch(va2, access) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, scale as u64) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "sp-relative pair access outside RAM".to_string(),
                        }))
                    }
                };
                let off2 = match self.ram_offset(pa2, scale as u64) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va2,
                            message: "sp-relative pair access outside RAM".to_string(),
                        }))
                    }
                };
                if is_store {
                    let v1 = if rt1 == 31 { 0 } else { self.machine.cpu[0].regs[rt1] };
                    let v2 = if rt2 == 31 { 0 } else { self.machine.cpu[0].regs[rt2] };
                    if is64 {
                        self.machine.ram[off..off + 8].copy_from_slice(&v1.to_le_bytes());
                        self.machine.ram[off2..off2 + 8].copy_from_slice(&v2.to_le_bytes());
                    } else {
                        self.machine.ram[off..off + 4]
                            .copy_from_slice(&(v1 as u32).to_le_bytes());
                        self.machine.ram[off2..off2 + 4]
                            .copy_from_slice(&(v2 as u32).to_le_bytes());
                    }
                } else if is64 {
                    let v1 = u64::from_le_bytes(self.machine.ram[off..off + 8].try_into().unwrap());
                    let v2 =
                        u64::from_le_bytes(self.machine.ram[off2..off2 + 8].try_into().unwrap());
                    if rt1 != 31 {
                        self.machine.cpu[0].regs[rt1] = v1;
                    }
                    if rt2 != 31 {
                        self.machine.cpu[0].regs[rt2] = v2;
                    }
                } else {
                    let v1 =
                        u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())
                            as u64;
                    let v2 = u32::from_le_bytes(
                        self.machine.ram[off2..off2 + 4].try_into().unwrap(),
                    ) as u64;
                    if rt1 != 31 {
                        self.machine.cpu[0].regs[rt1] = v1;
                    }
                    if rt2 != 31 {
                        self.machine.cpu[0].regs[rt2] = v2;
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        None
    }

    // ---- vCPU ----

    /// One vCPU step: fetch → U1 decode → U2 lift → U3 compile → wasmtime.
    /// Never panics: every failure becomes a typed [`HaltReason`].
    pub fn step_vcpu(&mut self) -> StepOutcome {
        if let Some(reason) = self.halted.clone() {
            return StepOutcome::Halted(reason);
        }
        let pc = self.machine.cpu[0].pc;

        // Fetch.
        let word = match self.fetch_word(pc) {
            Ok(w) => w,
            Err(reason) => {
                self.halted = Some(reason.clone());
                return StepOutcome::Halted(reason);
            }
        };

        // Fast path first (GB-23): handles SP-relative and other forms that
        // U1/U2 don't lift yet. Tried before decode so unrecognized words
        // get a chance here. The kind is unused by execute_arm64 (it decodes
        // from the word directly).
        {
            let insn = Instruction {
                addr: pc,
                word,
                kind: InsnKind::LoadStore,
            };
            if let Some(res) = self.execute_arm64(&insn) {
                match res {
                    Ok(()) => {
                        self.steps += 1;
                        self.tick_clock(TIMER_CYCLES_PER_STEP);
                        return StepOutcome::Continue;
                    }
                    Err(reason) => {
                        self.halted = Some(reason.clone());
                        return StepOutcome::Halted(reason);
                    }
                }
            }
        }

        // Decode (U1).
        let kind = match u1_decode::decode(word) {
            DecodeResult::Illegal { word } => {
                let reason = HaltReason::IllegalInstruction { addr: pc, word };
                self.halted = Some(reason.clone());
                return StepOutcome::Halted(reason);
            }
            DecodeResult::Ok(insn) => insn.kind,
        };
        let insn = Instruction {
            addr: pc,
            word,
            kind,
        };

        // Lift (U2).
        let ops = u2_ir_lift::lift(&insn);

        // Trap check BEFORE compiling: U2's trap strings are the exact,
        // honest reason. Compiling a trap would only produce `unreachable`.
        if let Some(reason) = first_trap_reason(&ops) {
            // Phase-2 spike: SVC gets its own halt variant so a future SVC
            // halt is instantly recognizable in traces vs generic Unsupported.
            let halt = if reason == u2_ir_lift::R_SVC_UNIMPL {
                HaltReason::Svc { addr: pc }
            } else {
                HaltReason::Unsupported { addr: pc, reason }
            };
            self.halted = Some(halt.clone());
            return StepOutcome::Halted(halt);
        }

        // Block exits: a conditional branch declares both arms (U3 lowers
        // the if/else from them); an explicit Branch op wins; else fall through.
        let exits = block_exits(pc, &ops);
        let block = IrBlock {
            entry_addr: pc,
            ops,
            exits,
        };

        // Compile (U3) and execute through the injected backend. Disjoint
        // field borrows: the backend borrows `executor`, the host borrows
        // regs / RAM / console / GPU port.
        let wasm = u3_wasm_jit::compile(&block);
        let cpu: &mut CpuState = &mut self.machine.cpu[0];
        let regs: &mut [u64; 31] = &mut cpu.regs;
        let ram: &mut Vec<u8> = &mut self.machine.ram;
        let console: &mut ConsoleState = &mut self.console;
        let gpu_port: &mut GpuPort = &mut self.gpu_port;
        let sysregs: &mut SysRegs = &mut cpu.sysregs;
        let mut host = WasmHost {
            ram,
            console,
            gpu_port,
            sysregs,
        };
        let (exit_addr, wfi_seen) = match self.executor.run_block(&wasm, regs, &mut host) {
            Ok(pair) => pair,
            Err(message) => {
                let halt = HaltReason::WasmTrap { addr: pc, message };
                self.halted = Some(halt.clone());
                return StepOutcome::Halted(halt);
            }
        };
        // The ExitVm sentinel is exactly -1 (0xFFFF_FFFF_FFFF_FFFF). A `< 0`
        // test is wrong: kernel VAs live in the high half (0xFFFF_...), so
        // every legitimate kernel-VA indirect-branch target is negative as
        // i64. Only the exact sentinel means "exit the VM"; anything else —
        // even 0xFFFF_FFFF_FFFF_FFFE — is a real address that fetch will
        // translate or fault honestly. (GB-21: BLR X8 to 0xffffff80095c0280
        // falsely halted here.)
        if exit_addr == -1 {
            let halt = HaltReason::ExitVm;
            self.halted = Some(halt.clone());
            return StepOutcome::Halted(halt);
        }
        self.machine.cpu[0].pc = exit_addr as u64;

        self.steps += 1;
        // Injected clock: the timer advances only here, never wall time.
        self.tick_clock(TIMER_CYCLES_PER_STEP);
        // WFI with no IRQ pending parks the vCPU: resumable, not halted.
        // (A pending IRQ is a spurious wake — the guest spins, honestly.)
        if wfi_seen && self.machine.irq.pending == 0 {
            return StepOutcome::WfiYield { addr: pc };
        }
        StepOutcome::Continue
    }

    /// Run until halt or `max_steps`. Returns the halt reason (or
    /// `StepLimitExceeded` when the budget runs out first). A WFI yield
    /// surfaces as `HaltReason::Wfi` WITHOUT poisoning `halted` — the
    /// machine stays resumable.
    pub fn run_until_halt(&mut self, max_steps: u64) -> HaltReason {
        for _ in 0..max_steps {
            match self.step_vcpu() {
                StepOutcome::Continue => {}
                StepOutcome::WfiYield { addr } => return HaltReason::Wfi { addr },
                StepOutcome::Halted(reason) => return reason,
            }
        }
        let halt = HaltReason::StepLimitExceeded;
        self.halted = Some(halt.clone());
        halt
    }

    // ---- clock / timer (U5) ----

    /// Advance the injected clock and tick the U5 timer. The ONLY clock.
    /// U5 sets the pending bit on the rising edge (INTID 27 per the LLD
    /// convention); the orchestrator surfaces it via `pending_irqs`.
    /// vCPU interrupt *injection* (vector jump) needs an exception model —
    /// Wave-4 work, stated here, not faked.
    pub fn tick_clock(&mut self, cycles: u64) {
        self.clock_cycles = self.clock_cycles.wrapping_add(cycles);
        let (next_irq, _irqs) = u5_gic_timer::tick(&self.machine.irq, cycles);
        self.machine.irq = next_irq;
    }

    /// Currently asserted interrupt lines (INTIDs with pending bits set).
    pub fn pending_irqs(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut bits = self.machine.irq.pending;
        let mut n = 0u32;
        while bits != 0 {
            if bits & 1 != 0 {
                out.push(n);
            }
            bits >>= 1;
            n += 1;
        }
        out
    }

    // ---- virtio (U6 transport + U7 gpu device) ----

    /// Drive one virtio queue: QueueNotify → U6 pop_chain → U7 device step →
    /// U6 push_used → IrqAssert per need_event_idx. Returns the device outputs
    /// on success; `Ok(vec![])` when there is no work or the queue is not
    /// live; `Err` on a malformed chain (U6's typed error, surfaced — never
    /// swallowed). Pure scheduling around unit calls; no device logic here.
    pub fn queue_notify(
        &mut self,
        queue_idx: u16,
    ) -> Result<Vec<DevOut>, u6_virtio_transport::ChainError> {
        let mut outs = Vec::new();
        let tq = match self.transport.queues.get(queue_idx as usize) {
            Some(q) => q.clone(),
            None => return Ok(outs),
        };
        // The queue must be live: configured, ready, driver OK.
        const DRIVER_OK: u8 = 4;
        if !tq.ready || self.transport.status & DRIVER_OK == 0 {
            return Ok(outs);
        }
        // U6 works in ram-slice coordinates: every address it takes (ring
        // bases, descriptor buffer addresses) is an offset into the `ram`
        // slice, validated internally by U6. The VirtQueue therefore stores
        // offsets (guest-PA − RAM_BASE); the guest-physical → offset
        // translation happens at the virtio-MMIO programming boundary when
        // the driver writes QUEUE_DESC/QUEUE_AVAIL/QUEUE_USED (Wave-4 driver
        // work — no MMIO virtio programming path exists yet).
        let pop = match u6_virtio_transport::pop_chain(
            &self.machine.ram,
            tq.desc_addr,
            tq.avail_addr,
            tq.size,
            tq.last_avail_idx,
        ) {
            Ok(Some(pop)) => pop,
            Ok(None) => return Ok(outs), // nothing available — not an error
            Err(e) => return Err(e),
        };
        // Gather the readable bytes for the device model. U6 already
        // range-checked these as slice offsets; re-check defensively.
        let mut cmd_bytes = Vec::new();
        for seg in &pop.chain.readable {
            let off = seg.addr as usize;
            let len = seg.len as usize;
            let end = off
                .checked_add(len)
                .filter(|&e| e <= self.machine.ram.len())
                .ok_or(u6_virtio_transport::ChainError::DmaOutOfRange)?;
            cmd_bytes.extend_from_slice(&self.machine.ram[off..end]);
        }
        // Device dispatch (U7 gpu). U7 decodes the command stream from the
        // provided buffer — the orchestrator supplies the chain's bytes.
        let (next_gpu, mut dev_outs) =
            u7_gpu_device::step(&self.gpu, DevEvent::QueueNotify { queue_idx }, &cmd_bytes);
        self.gpu = next_gpu;
        // Route GPU commands through the host stack (U8). Submit3D flows to
        // the U13 surface in Wave 4; here dispatch records the HostAction.
        for out in &dev_outs {
            if let DevOut::GpuCommands(cmds) = out {
                for cmd in cmds {
                    let _action = u8_gpu_host::dispatch(cmd);
                }
            }
        }
        outs.append(&mut dev_outs);
        // Commit to the used ring (U6 owns the ring format).
        let new_used = u6_virtio_transport::push_used(
            &mut self.machine.ram,
            tq.used_addr,
            tq.size,
            tq.last_used_idx,
            pop.chain.head,
            0, // gpu commands write no bytes back in this device model
        )?;
        // Advance the queue cursors (explicit state threading).
        if let Some(q) = self.transport.queues.get_mut(queue_idx as usize) {
            q.last_avail_idx = pop.next_avail;
            q.last_used_idx = new_used;
        }
        // IRQ decision per need_event_idx. avail_event is read for real from
        // the used ring when the transport negotiated EVENT_IDX.
        let event =
            read_avail_event(&self.machine.ram, tq.used_addr, tq.size).unwrap_or(pop.next_avail);
        while self.last_notified.len() <= queue_idx as usize {
            self.last_notified.push(0);
        }
        let old_notified = self.last_notified[queue_idx as usize];
        if u6_virtio_transport::need_event_idx(new_used, event, old_notified) {
            self.last_notified[queue_idx as usize] = new_used;
            outs.push(DevOut::IrqAssert {
                num: VIRTIO_GPU_IRQ,
            });
            self.machine.irq.pending |= 1u64 << VIRTIO_GPU_IRQ;
        }
        Ok(outs)
    }

    // ---- snapshot / persist (U11 + U13 BlobStore) ----

    /// Full snapshot: U11's machine snapshot plus a canonical encoding of the
    /// orchestrator-owned device states (transport, gpu, console), the injected
    /// clock, step count, and halt reason. Layout: `u64` U11-blob length,
    /// U11 blob, `b"PN12"`, then the canonical section.
    pub fn snapshot_full(&self) -> Vec<u8> {
        let snap = u11_snapshot::snapshot(&self.machine);
        let mut out = Vec::new();
        out.extend_from_slice(&(snap.0.len() as u64).to_le_bytes());
        out.extend_from_slice(&snap.0);
        out.extend_from_slice(b"PN12");
        let mut w = CanonWriter::new();
        // TransportState.
        w.u16(self.transport.queue_count);
        w.u64(self.transport.features);
        w.u8(self.transport.status);
        w.u32(self.transport.queues.len() as u32);
        for q in &self.transport.queues {
            w.u64(q.desc_addr);
            w.u64(q.avail_addr);
            w.u64(q.used_addr);
            w.u16(q.size);
            w.u8(q.ready as u8);
            w.u16(q.last_avail_idx);
            w.u16(q.last_used_idx);
        }
        // GpuDevState.
        w.u32(self.gpu.next_resource_id);
        w.u64(self.gpu.next_fence_id);
        // ConsoleState.
        w.bytes(&self.console.tx_bytes);
        w.u32(self.console.rx_queue.len() as u32);
        for b in &self.console.rx_queue {
            w.u8(*b);
        }
        // Clock, steps, halt.
        w.u64(self.clock_cycles);
        w.u64(self.steps);
        match &self.halted {
            None => w.u8(0),
            Some(HaltReason::IllegalInstruction { addr, word }) => {
                w.u8(1);
                w.u64(*addr);
                w.u32(*word);
            }
            Some(HaltReason::Unsupported { addr, reason }) => {
                w.u8(2);
                w.u64(*addr);
                w.bytes(reason.as_bytes());
            }
            Some(HaltReason::WasmTrap { addr, message }) => {
                w.u8(3);
                w.u64(*addr);
                w.bytes(message.as_bytes());
            }
            Some(HaltReason::FetchFault { addr }) => {
                w.u8(4);
                w.u64(*addr);
            }
            Some(HaltReason::ExitVm) => w.u8(5),
            Some(HaltReason::StepLimitExceeded) => w.u8(6),
            // Tag 7 is additive: blobs written before Wave 4 never carry it,
            // and tags 0-6 decode exactly as before.
            Some(HaltReason::Wfi { addr }) => {
                w.u8(7);
                w.u64(*addr);
            }
            // Tag 8 is additive: blobs written before the Phase-2 spike
            // never carry it, and tags 0-7 decode exactly as before.
            Some(HaltReason::Svc { addr }) => {
                w.u8(8);
                w.u64(*addr);
            }
        }
        out.extend_from_slice(&w.buf);
        out
    }

    /// Restore a [`Orchestrator::snapshot_full`] blob. Data in, state out —
    /// corrupt input is a typed error, never a panic.
    pub fn restore_full(bytes: &[u8]) -> Result<Self, RestoreError> {
        let mut r = CanonReader::new(bytes);
        let u11_len = r.u64().ok_or(RestoreError::BadFormat)? as usize;
        let u11_blob = r.take(u11_len).ok_or(RestoreError::BadFormat)?;
        let machine = u11_snapshot::restore(u11_blob).map_err(|_| RestoreError::BadFormat)?;
        let marker = r.take(4).ok_or(RestoreError::BadFormat)?;
        if marker != b"PN12" {
            return Err(RestoreError::BadFormat);
        }
        let mut o = Orchestrator::new();
        o.machine = machine;
        o.transport.queue_count = r.u16().ok_or(RestoreError::BadFormat)?;
        o.transport.features = r.u64().ok_or(RestoreError::BadFormat)?;
        o.transport.status = r.u8().ok_or(RestoreError::BadFormat)?;
        let qlen = r.u32().ok_or(RestoreError::BadFormat)? as usize;
        if qlen > 1024 {
            return Err(RestoreError::BadFormat);
        }
        for _ in 0..qlen {
            o.transport.queues.push(pathn_contracts::device::VirtQueue {
                desc_addr: r.u64().ok_or(RestoreError::BadFormat)?,
                avail_addr: r.u64().ok_or(RestoreError::BadFormat)?,
                used_addr: r.u64().ok_or(RestoreError::BadFormat)?,
                size: r.u16().ok_or(RestoreError::BadFormat)?,
                ready: r.u8().ok_or(RestoreError::BadFormat)? != 0,
                last_avail_idx: r.u16().ok_or(RestoreError::BadFormat)?,
                last_used_idx: r.u16().ok_or(RestoreError::BadFormat)?,
            });
        }
        o.gpu.next_resource_id = r.u32().ok_or(RestoreError::BadFormat)?;
        o.gpu.next_fence_id = r.u64().ok_or(RestoreError::BadFormat)?;
        let tx = r.bytes().ok_or(RestoreError::BadFormat)?;
        o.console.tx_bytes = tx;
        let rxlen = r.u32().ok_or(RestoreError::BadFormat)? as usize;
        if rxlen > 1 << 20 {
            return Err(RestoreError::BadFormat);
        }
        for _ in 0..rxlen {
            o.console
                .rx_queue
                .push_back(r.u8().ok_or(RestoreError::BadFormat)?);
        }
        o.clock_cycles = r.u64().ok_or(RestoreError::BadFormat)?;
        o.steps = r.u64().ok_or(RestoreError::BadFormat)?;
        let tag = r.u8().ok_or(RestoreError::BadFormat)?;
        o.halted = match tag {
            0 => None,
            1 => Some(HaltReason::IllegalInstruction {
                addr: r.u64().ok_or(RestoreError::BadFormat)?,
                word: r.u32().ok_or(RestoreError::BadFormat)?,
            }),
            2 => {
                let addr = r.u64().ok_or(RestoreError::BadFormat)?;
                let rb = r.bytes().ok_or(RestoreError::BadFormat)?;
                // Reason strings are U2's static strings; map back by value.
                let reason = match_static_reason(&rb).ok_or(RestoreError::BadFormat)?;
                Some(HaltReason::Unsupported { addr, reason })
            }
            3 => Some(HaltReason::WasmTrap {
                addr: r.u64().ok_or(RestoreError::BadFormat)?,
                message: String::from_utf8(r.bytes().ok_or(RestoreError::BadFormat)?)
                    .map_err(|_| RestoreError::BadFormat)?,
            }),
            4 => Some(HaltReason::FetchFault {
                addr: r.u64().ok_or(RestoreError::BadFormat)?,
            }),
            5 => Some(HaltReason::ExitVm),
            6 => Some(HaltReason::StepLimitExceeded),
            7 => Some(HaltReason::Wfi {
                addr: r.u64().ok_or(RestoreError::BadFormat)?,
            }),
            8 => Some(HaltReason::Svc {
                addr: r.u64().ok_or(RestoreError::BadFormat)?,
            }),
            _ => return Err(RestoreError::BadFormat),
        };
        o.last_notified = vec![0; o.transport.queues.len()];
        Ok(o)
    }

    /// Persist the full snapshot through a U13 BlobStore (5.1 acceptance).
    pub fn persist(&self, store: &mut dyn BlobStore, key: &str) {
        store.save(key, &self.snapshot_full());
    }

    /// Deterministic state hash: sha256 over the full snapshot. Same image +
    /// same scripted inputs → identical hash. The engine and block cache are
    /// excluded (runtime machinery, not state).
    pub fn state_hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.snapshot_full());
        h.finalize().into()
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot restore failure — data, not panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    BadFormat,
}

// ---------------------------------------------------------------------------
// Small pure helpers (scheduling logic, no emulation semantics)
// ---------------------------------------------------------------------------

/// First `IrOp::Trap` reason in a lifted op sequence, if any.
fn first_trap_reason(ops: &[IrOp]) -> Option<&'static str> {
    for op in ops {
        if let IrOp::Trap { reason } = op {
            return Some(reason);
        }
    }
    None
}

/// Static branch target if the sequence ends in an explicit branch.
fn branch_target(ops: &[IrOp]) -> Option<u64> {
    for op in ops {
        if let IrOp::Branch { target } = op {
            return Some(*target);
        }
    }
    None
}

/// Block exits for a lifted op sequence. A conditional branch declares both
/// arms (U3 lowers the if/else from `Branch` + `FallThrough`); an indirect
/// branch (Wave 5) declares `Dynamic` — no static target exists; an explicit
/// `Branch` op wins; otherwise the block falls through. U3's `CondBranch`
/// traps loudly when no `FallThrough` is present, so the orchestrator always
/// supplies one here.
fn block_exits(pc: u64, ops: &[IrOp]) -> Vec<BlockExit> {
    for op in ops {
        if let IrOp::CondBranch { target, .. } = op {
            return vec![
                BlockExit::Branch(*target),
                BlockExit::FallThrough(pc.wrapping_add(4)),
            ];
        }
    }
    for op in ops {
        if let IrOp::BranchDyn { .. } = op {
            return vec![BlockExit::Dynamic];
        }
    }
    match branch_target(ops) {
        Some(t) => vec![BlockExit::Branch(t)],
        None => vec![BlockExit::FallThrough(pc.wrapping_add(4))],
    }
}

// ---------------------------------------------------------------------------
// WASM host integration (Wave 4: U3-G1 module shape)
// ---------------------------------------------------------------------------

/// The host state a U3 module sees. Borrowed mutably from the orchestrator
/// for exactly one block execution; dropped before the next step.
/// Implements [`HostOps`] — the execution backend forwards the U3 imports
/// here. (The `wfi_seen` flag moved into the backends' tracking wrapper.)
struct WasmHost<'a> {
    ram: &'a mut Vec<u8>,
    console: &'a mut ConsoleState,
    gpu_port: &'a mut GpuPort,
    sysregs: &'a mut SysRegs,
}

impl WasmHost<'_> {
    /// Live stage-1 translation state, rebuilt from the guest's current
    /// system registers on every data access. MSR writes update
    /// `sysregs` directly, so this is always fresh; the persisted
    /// `MachineState.mmu` snapshot is NOT used here (it is stale).
    fn mmu_state(&self) -> MmuState {
        MmuState {
            sctlr: self.sysregs.sctlr_el1,
            tcr: self.sysregs.tcr_el1,
            ttbr0: self.sysregs.ttbr0_el1,
            ttbr1: self.sysregs.ttbr1_el1,
        }
    }

    /// Translate a data-access VA to a guest-physical address. MMU off
    /// (SCTLR_EL1.M == 0): identity. MMU on: stage-1 walk against the
    /// live registers, rebasing descriptor PAs by RAM_BASE. Faults
    /// surface as `mmu_fault: <MemFault>` strings, which the executor
    /// turns into `HaltReason::WasmTrap`.
    fn translate_data(&self, va: u64, access: Access) -> Result<u64, String> {
        let st = self.mmu_state();
        if st.sctlr & 1 == 0 {
            return Ok(va);
        }
        u4_mmu::translate_with_base(&st, self.ram, RAM_BASE, va, access)
            .map_err(|f| format!("mmu_fault: {f:?}"))
    }
}

/// Translate a guest physical address to a RAM offset (M0: MMU disabled,
/// identity-with-bounds-check). Free function so the WASM host closures can
/// use it without borrowing the whole orchestrator.
fn ram_offset_in(ram: &[u8], pa: u64, len: u64) -> Result<usize, MemFault> {
    let off = pa
        .checked_sub(RAM_BASE)
        .ok_or(MemFault::TranslationFault { va: pa })?;
    let end = off
        .checked_add(len)
        .ok_or(MemFault::TranslationFault { va: pa })?;
    if end > ram.len() as u64 {
        return Err(MemFault::TranslationFault { va: pa });
    }
    Ok(off as usize)
}

/// `env.mem_load(addr, size) -> value`. Console and GPU MMIO are dispatched
/// to their device models; everything else goes through the RAM bounds
/// check. Faults become WASM traps (surfaced as `HaltReason::WasmTrap`).
impl HostOps for WasmHost<'_> {
    fn mem_load(&mut self, addr: i64, size: i64) -> Result<i64, String> {
        // Data VA -> PA first (identity when the MMU is off); the
        // translated PA then dispatches to MMIO or RAM as before.
        let pa = self.translate_data(addr as u64, Access::Read)?;
        if (CONSOLE_BASE..CONSOLE_BASE + CONSOLE_SIZE).contains(&pa) {
            if size != 1 {
                return Err("mem_load: console MMIO is byte-only".to_string());
            }
            if pa == CONSOLE_RX {
                return Ok(self.console.read_rx() as i64);
            }
            // Defined MMIO region, non-RX offset: reads return 0 (PLATFORM.md).
            return Ok(0);
        }
        if (GPU_BASE..GPU_BASE + GPU_SIZE).contains(&pa) {
            if size != 1 {
                return Err("mem_load: GPU MMIO is byte-only".to_string());
            }
            // Defined MMIO region, no readable registers: reads return 0.
            return Ok(0);
        }
        if !matches!(size, 1 | 2 | 4 | 8) {
            return Err("mem_load: size must be 1, 2, 4 or 8".to_string());
        }
        let off = ram_offset_in(self.ram, pa, size as u64)
            .map_err(|f| format!("mem_load fault: {f:?}"))?;
        let mut v = 0u64;
        for i in 0..size as usize {
            v |= (self.ram[off + i] as u64) << (8 * i);
        }
        Ok(v as i64)
    }

    /// `env.mem_store(addr, size, value)`. Console and GPU MMIO are
    /// dispatched to their device models; everything else goes through the
    /// RAM bounds check.
    fn mem_store(&mut self, addr: i64, size: i64, val: i64) -> Result<(), String> {
        // Data VA -> PA first (identity when the MMU is off); the
        // translated PA then dispatches to MMIO or RAM as before.
        let pa = self.translate_data(addr as u64, Access::Write)?;
        if (CONSOLE_BASE..CONSOLE_BASE + CONSOLE_SIZE).contains(&pa) {
            if size != 1 {
                return Err("mem_store: console MMIO is byte-only".to_string());
            }
            if pa == CONSOLE_TX {
                self.console.write_tx(val as u8);
            }
            // Writes to other console offsets are acknowledged, no effect.
            return Ok(());
        }
        if (GPU_BASE..GPU_BASE + GPU_SIZE).contains(&pa) {
            if size != 1 {
                return Err("mem_store: GPU MMIO is byte-only".to_string());
            }
            if pa == GPU_DATA {
                self.gpu_port.push_byte(val as u8);
            } else if pa == GPU_SUBMIT {
                // Write value ignored; the signal is the submit itself.
                self.gpu_port.submit();
            }
            // Writes to other GPU offsets are acknowledged, no effect.
            return Ok(());
        }
        if !matches!(size, 1 | 2 | 4 | 8) {
            return Err("mem_store: size must be 1, 2, 4 or 8".to_string());
        }
        let off = ram_offset_in(self.ram, pa, size as u64)
            .map_err(|f| format!("mem_store fault: {f:?}"))?;
        for i in 0..size as usize {
            self.ram[off + i] = (val as u64 >> (8 * i)) as u8;
        }
        Ok(())
    }

    fn wfi(&mut self) {
        // Notification only; the backend tracks invocation for `wfi_seen`.
    }

    fn sysreg_load(&mut self, reg: u8) -> Result<i64, String> {
        let sel = SysRegs::from_index(reg)
            .ok_or_else(|| format!("sysreg_load: bad index"))?;
        Ok(self.sysregs.load(sel) as i64)
    }

    fn sysreg_store(&mut self, reg: u8, val: i64) -> Result<(), String> {
        let sel = SysRegs::from_index(reg)
            .ok_or_else(|| format!("sysreg_store: bad index"))?;
        self.sysregs.store(sel, val as u64);
        Ok(())
    }
}

/// Read `avail_event` from a used ring (EVENT_IDX layout): flags u16 @0,
/// idx u16 @2, ring @4 (8 bytes * size), avail_event u16 after.
/// `used_off` is an offset into `ram` (already PA-translated).
fn read_avail_event(ram: &[u8], used_off: u64, size: u16) -> Option<u16> {
    let off = used_off.checked_add(4 + 8 * size as u64)? as usize;
    if off + 2 > ram.len() {
        return None;
    }
    Some(u16::from_le_bytes(ram[off..off + 2].try_into().ok()?))
}

/// Map a persisted trap-reason string back to U2's static string.
/// Only reasons the orchestrator can actually produce are listed; anything
/// else is a corrupt snapshot, not a guess.
fn match_static_reason(bytes: &[u8]) -> Option<&'static str> {
    // U2's trap strings are `&'static str` constants; equality is by value.
    // We enumerate the ones reachable through the current lift set.
    const KNOWN: &[&str] = &[
        "DataProc: unsupported encoding",
        "DataProc: MOVZ hw field > 1 with sf = 0 is unallocated",
        "DataProc: 32-bit ADD immediate width is not expressible in IrOp::Add",
        "DataProc: 32-bit ADD register width is not expressible in IrOp::Add",
        "DataProc: shifted or extended ADD register operand is not expressible in IrOp",
        "LoadStore: unsupported encoding",
        "LoadStore: sub-word access width is not expressible in IrOp",
        "LoadStore: register-relative address is dynamic; IrOp::Load/Store carry static addresses only",
        "LoadStore: register-relative access via SP is not expressible in IrOp",
        "Branch: only unconditional immediate B is lifted",
        // Wave 5 (BL/RET scope) renamed the reason above; the old string is
        // kept so pre-Wave-5 snapshots still decode. New halts carry this:
        "Branch: only B/BL/RET/CBZ/CBNZ are lifted",
        "Branch: unsupported encoding",
        "Branch: 32-bit CBZ/CBNZ width is not expressible in IrOp::CondBranch",
        "Branch: 32-bit ORR width is not expressible in IrOp::OrrShift",
        "System: system and privileged semantics are not lifted",
        "Unknown: illegal or unrecognized instruction word",
    ];
    let s = std::str::from_utf8(bytes).ok()?;
    KNOWN.iter().find(|k| **k == s).copied()
}

// ---------------------------------------------------------------------------
// Canonical byte writer/reader for the orchestrator-owned snapshot section
// ---------------------------------------------------------------------------

struct CanonWriter {
    buf: Vec<u8>,
}

impl CanonWriter {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.u64(b.len() as u64);
        self.buf.extend_from_slice(b);
    }
}

struct CanonReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> CanonReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.pos + n > self.buf.len() {
            return None;
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2)
            .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u64()? as usize;
        if n > 1 << 30 {
            return None;
        }
        self.take(n).map(|s| s.to_vec())
    }
}

// ---------------------------------------------------------------------------
// Unit tests — device models, timer, snapshot (the `boot_` integration tests
// live in tests/boot.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use pathn_contracts::adapters::NormalizedInput;
    use u13_adapters::{MemStore, ScriptedInput};

    // ---- console ----

    #[test]
    fn console_tx_appends_and_rx_reads_fifo() {
        let mut c = ConsoleState::new();
        c.write_tx(b'A');
        c.write_tx(b'B');
        assert_eq!(c.tx_bytes, vec![b'A', b'B']);
        c.feed_rx(b"hi");
        assert_eq!(c.read_rx(), b'h');
        assert_eq!(c.read_rx(), b'i');
        // PLATFORM.md: empty RX reads as 0.
        assert_eq!(c.read_rx(), 0);
    }

    #[test]
    fn console_poll_input_takes_key_bytes_only() {
        use pathn_contracts::adapters::KeyCode;
        let mut c = ConsoleState::new();
        let mut src = ScriptedInput::new(vec![vec![
            NormalizedInput::Key {
                code: KeyCode(65),
                pressed: true,
            },
            NormalizedInput::Key {
                code: KeyCode(66),
                pressed: false, // release: not text
            },
            NormalizedInput::PointerMove { x: 1, y: 2 }, // not text
            NormalizedInput::Key {
                code: KeyCode(999), // >= 256: not a byte
                pressed: true,
            },
        ]]);
        c.poll_input(&mut src);
        assert_eq!(c.read_rx(), 65);
        assert_eq!(c.read_rx(), 0);
    }

    #[test]
    fn mmio_dispatch_routes_console_range() {
        let mut o = Orchestrator::new();
        assert!(o.mmio_write(CONSOLE_TX, b'Z'));
        assert_eq!(o.console.tx_bytes, vec![b'Z']);
        // Writes to other console offsets are acknowledged, no effect.
        assert!(o.mmio_write(CONSOLE_BASE + 0x100, 0xFF));
        assert_eq!(o.console.tx_bytes, vec![b'Z']);
        // Non-MMIO address is not claimed.
        assert!(!o.mmio_write(RAM_BASE, 0xFF));

        o.console.feed_rx(b"Q");
        assert_eq!(o.mmio_read(CONSOLE_RX), Some(b'Q'));
        assert_eq!(o.mmio_read(CONSOLE_RX), Some(0));
        assert_eq!(o.mmio_read(CONSOLE_BASE + 0x200), Some(0));
        assert_eq!(o.mmio_read(RAM_BASE), None);
    }

    // ---- timer (U5, injected clock) ----

    #[test]
    fn timer_fires_intid_27_on_injected_ticks_only() {
        let mut o = Orchestrator::new();
        // Enable the timer (bit 0) and arm the compare.
        o.machine.irq.enabled = 1;
        o.machine.irq.timer_compare = 5000;
        o.tick_clock(4000);
        assert!(o.pending_irqs().is_empty());
        assert_eq!(o.machine.irq.timer_count, 4000);
        o.tick_clock(1000);
        // Rising edge: INTID 27 asserted exactly once.
        assert_eq!(o.pending_irqs(), vec![27]);
        let count_before = o.machine.irq.timer_count;
        o.tick_clock(1000);
        // Still pending, no second edge.
        assert_eq!(o.pending_irqs(), vec![27]);
        assert_eq!(o.machine.irq.timer_count, count_before + 1000);
    }

    #[test]
    fn timer_disabled_never_fires() {
        let mut o = Orchestrator::new();
        o.machine.irq.timer_compare = 1;
        o.tick_clock(1 << 20);
        assert!(o.pending_irqs().is_empty());
    }

    // ---- image loading ----

    fn minimal_image(entry: u64, words: &[u32]) -> Vec<u8> {
        let mut img = Vec::new();
        img.extend_from_slice(&IMAGE_MAGIC.to_le_bytes());
        img.extend_from_slice(&IMAGE_VERSION.to_le_bytes());
        img.extend_from_slice(&entry.to_le_bytes());
        img.extend_from_slice(&(words.len() as u64 * 4).to_le_bytes());
        for w in words {
            img.extend_from_slice(&w.to_le_bytes());
        }
        img
    }

    #[test]
    fn load_image_sets_entry_contract() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &[0xD28000A1]))
            .unwrap();
        let cpu = &o.machine.cpu[0];
        assert_eq!(cpu.pc, 0x4000_0000);
        assert_eq!(cpu.sp, 0x4800_0000);
        assert_eq!(cpu.regs, [0; 31]);
        assert_eq!(cpu.pstate, 0);
        assert_eq!(o.machine.ram[0..4], [0xA1, 0x00, 0x80, 0xD2]);
    }

    #[test]
    fn load_image_rejects_bad_magic_and_truncation() {
        let mut o = Orchestrator::new();
        assert_eq!(o.load_image(&[0u8; 8]), Err(ImageError::TooShort));
        let mut bad = minimal_image(0x4000_0000, &[1]);
        bad[0] = 0xFF;
        assert!(matches!(
            o.load_image(&bad),
            Err(ImageError::BadMagic { .. })
        ));
        let mut short = minimal_image(0x4000_0000, &[1, 2]);
        short.truncate(short.len() - 1);
        assert!(matches!(
            o.load_image(&short),
            Err(ImageError::Truncated { .. })
        ));
    }

    // ---- snapshot / persist ----

    #[test]
    fn snapshot_full_roundtrips_all_state() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &[0xD28000A1]))
            .unwrap();
        o.console.write_tx(b'X');
        o.console.feed_rx(b"yz");
        o.transport.queue_count = 1;
        o.transport.status = 4;
        o.transport.queues.push(pathn_contracts::device::VirtQueue {
            desc_addr: 0x1000,
            avail_addr: 0x2000,
            used_addr: 0x3000,
            size: 16,
            ready: true,
            last_avail_idx: 3,
            last_used_idx: 2,
        });
        o.gpu.next_resource_id = 42;
        o.tick_clock(1234);
        o.halted = Some(HaltReason::Unsupported {
            addr: 0x4000_0000,
            reason: "DataProc: unsupported encoding",
        });
        let blob = o.snapshot_full();
        let r = Orchestrator::restore_full(&blob).unwrap();
        assert_eq!(r.machine, o.machine);
        assert_eq!(r.transport, o.transport);
        assert_eq!(r.gpu, o.gpu);
        assert_eq!(r.console, o.console);
        assert_eq!(r.clock_cycles, o.clock_cycles);
        assert_eq!(r.steps, o.steps);
        assert_eq!(r.halted, o.halted);
        // State hash is stable across the roundtrip.
        assert_eq!(r.state_hash(), o.state_hash());
    }

    #[test]
    fn restore_full_rejects_corrupt_input() {
        assert!(matches!(
            Orchestrator::restore_full(&[0u8; 10]),
            Err(RestoreError::BadFormat)
        ));
        assert!(matches!(
            Orchestrator::restore_full(b"definitely not a snapshot"),
            Err(RestoreError::BadFormat)
        ));
        // Truncated tail.
        let o = Orchestrator::new();
        let mut blob = o.snapshot_full();
        blob.truncate(blob.len() - 5);
        assert!(matches!(
            Orchestrator::restore_full(&blob),
            Err(RestoreError::BadFormat)
        ));
    }

    #[test]
    fn persist_uses_blobstore_contract() {
        let o = Orchestrator::new();
        let mut store = MemStore::new();
        o.persist(&mut store, "snap/boot");
        let loaded = store.load("snap/boot").unwrap();
        let r = Orchestrator::restore_full(&loaded).unwrap();
        assert_eq!(r.state_hash(), o.state_hash());
        assert_eq!(store.load("missing"), None);
    }

    #[test]
    fn state_hash_changes_with_state() {
        let mut a = Orchestrator::new();
        let mut b = Orchestrator::new();
        assert_eq!(a.state_hash(), b.state_hash());
        b.console.feed_rx(b"x");
        assert_ne!(a.state_hash(), b.state_hash());
        a.console.feed_rx(b"x");
        assert_eq!(a.state_hash(), b.state_hash());
    }

    // ---- virtio plumbing ----

    #[test]
    fn queue_notify_no_queues_is_noop() {
        let mut o = Orchestrator::new();
        let outs = o.queue_notify(0).unwrap();
        assert!(outs.is_empty());
    }

    #[test]
    fn queue_notify_not_ready_is_noop() {
        let mut o = Orchestrator::new();
        o.transport.queue_count = 1;
        o.transport.status = 4; // DRIVER_OK, but queue not ready
        o.transport.queues.push(pathn_contracts::device::VirtQueue {
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            size: 16,
            ready: false,
            last_avail_idx: 0,
            last_used_idx: 0,
        });
        let outs = o.queue_notify(0).unwrap();
        assert!(outs.is_empty());
    }

    // ---- input source wiring ----

    #[test]
    fn scripted_input_feeds_console() {
        let mut o = Orchestrator::new();
        let mut src = ScriptedInput::new(vec![]);
        // Empty script: nothing fed, no panic.
        o.console.poll_input(&mut src);
        assert_eq!(o.console.read_rx(), 0);
    }

    // ---- Wave 4: host integration (U3-G1) ----

    /// The real 4.1 guest's entry word. Wave 3 halted here with
    /// `IllegalInstruction`; Wave 4 must execute past it.
    const ADRP_X10: u32 = 0xB000_000A; // ADRP X10, #0x1000
    const WFI: u32 = 0xD503_207F;

    #[test]
    fn wave4_adrp_executes_past_wave3_halt() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &[ADRP_X10, WFI]))
            .unwrap();
        let reason = o.run_until_halt(10);
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_0004 });
        // ADRP X10, #0x1000 @0x40000000 -> X10 = 0x40001000 (page math).
        assert_eq!(o.machine.cpu[0].regs[10], 0x4000_1000);
        assert_eq!(o.steps(), 2);
    }

    #[test]
    fn wave4_registers_persist_across_blocks() {
        let mut o = Orchestrator::new();
        // MOVZ X1, #5; ADD X2, X1, #3; WFI.
        o.load_image(&minimal_image(0x4000_0000, &[0xD28000A1, 0x91000C22, WFI]))
            .unwrap();
        let reason = o.run_until_halt(10);
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_0008 });
        assert_eq!(o.machine.cpu[0].regs[1], 5);
        assert_eq!(o.machine.cpu[0].regs[2], 8);
        // Untouched registers stay zero — no cross-block leakage.
        assert_eq!(o.machine.cpu[0].regs[3], 0);
    }

    /// LDRB W2, [X10]; CBZ X2, #8; STRB W2, [X11]; WFI — with X10 = RX,
    /// X11 = TX. Empty RX reads 0, so CBZ is taken and the STRB is skipped.
    #[test]
    fn wave4_cbz_taken_skips_strb() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(
            0x4000_0000,
            &[0x39400142, 0xB4000042, 0x39000162, WFI],
        ))
        .unwrap();
        o.machine.cpu[0].regs[10] = CONSOLE_RX;
        o.machine.cpu[0].regs[11] = CONSOLE_TX;
        let reason = o.run_until_halt(10);
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_000C });
        assert_eq!(o.machine.cpu[0].regs[2], 0);
        assert!(o.console.tx_bytes.is_empty());
        assert_eq!(o.steps(), 3);
    }

    /// Same program, but RX holds `Z`: CBZ not taken, STRB echoes the byte.
    #[test]
    fn wave4_cbnz_path_runs_strb() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(
            0x4000_0000,
            &[0x39400142, 0xB4000042, 0x39000162, WFI],
        ))
        .unwrap();
        o.machine.cpu[0].regs[10] = CONSOLE_RX;
        o.machine.cpu[0].regs[11] = CONSOLE_TX;
        o.console.feed_rx(b"Z");
        let reason = o.run_until_halt(10);
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_000C });
        assert_eq!(o.machine.cpu[0].regs[2], b'Z' as u64);
        assert_eq!(o.console.tx_bytes, vec![b'Z']);
        assert_eq!(o.steps(), 4);
    }

    #[test]
    fn wave4_wfi_yield_is_resumable() {
        let mut o = Orchestrator::new();
        // WFI; MOVZ X1, #7.
        o.load_image(&minimal_image(0x4000_0000, &[WFI, 0xD28000E1]))
            .unwrap();
        let reason = o.run_until_halt(10);
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_0000 });
        // The machine is parked, NOT halted: halted() stays None ...
        assert_eq!(o.halted(), None);
        // ... and the next step resumes past the WFI.
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine.cpu[0].regs[1], 7);
        assert_eq!(o.machine.cpu[0].pc, 0x4000_0008);
    }

    #[test]
    fn wave4_wfi_does_not_busy_spin() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &[WFI])).unwrap();
        // One WFI step yields immediately — no spinning to the step budget.
        let reason = o.run_until_halt(1_000_000);
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_0000 });
        assert_eq!(o.steps(), 1);
    }

    #[test]
    fn wave4_snapshot_wfi_tag_roundtrips() {
        let mut o = Orchestrator::new();
        o.halted = Some(HaltReason::Wfi { addr: 0x4000_0000 });
        let blob = o.snapshot_full();
        let r = Orchestrator::restore_full(&blob).unwrap();
        assert_eq!(r.halted, o.halted);
        assert_eq!(r.state_hash(), o.state_hash());
    }

    #[test]
    fn wave4_match_static_reason_covers_new_traps() {
        // New Wave-4 trap strings must survive a snapshot round-trip too.
        for s in [
            "LoadStore: register-relative access via SP is not expressible in IrOp",
            "Branch: 32-bit CBZ/CBNZ width is not expressible in IrOp::CondBranch",
            "Branch: 32-bit ORR width is not expressible in IrOp::OrrShift",
        ] {
            assert!(match_static_reason(s.as_bytes()).is_some());
        }
        // Old strings still map (snapshot compatibility).
        assert_eq!(
            match_static_reason(b"Branch: only unconditional immediate B is lifted"),
            Some("Branch: only unconditional immediate B is lifted")
        );
        assert_eq!(match_static_reason(b"nope"), None);
    }

    #[test]
    fn wave4_mem_fault_becomes_wasm_trap() {
        let mut o = Orchestrator::new();
        // LDRB W2, [X10] with X10 far outside RAM -> mem_load faults.
        o.load_image(&minimal_image(0x4000_0000, &[0x39400142]))
            .unwrap();
        o.machine.cpu[0].regs[10] = 0xFFFF_FFFF_0000_0000;
        let reason = o.run_until_halt(10);
        assert!(matches!(reason, HaltReason::WasmTrap { .. }));
        assert_eq!(o.halted(), Some(&reason));
    }

    #[test]
    fn wave4_wfi_with_pending_irq_continues() {
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &[WFI])).unwrap();
        // Spurious wake: an already-pending IRQ means WFI returns at once.
        // (Bit 5: a non-timer source — U5 owns and clears the timer bit 27
        // on ticks where the timer is not asserting.)
        o.machine.irq.pending = 1 << 5;
        let outcome = o.step_vcpu();
        assert_eq!(outcome, StepOutcome::Continue);
        assert_eq!(o.machine.cpu[0].pc, 0x4000_0004);
    }

    #[test]
    fn wave5_bl_ret_link_register_roundtrip() {
        // Hand-assembled call/return through the real pipeline:
        //   0x4000_0000: BL +2          ; X30 = 0x4000_0004, pc -> 0x4000_0008
        //   0x4000_0004: B +3           ; landing pad -> 0x4000_0010
        //   0x4000_0008: MOVZ X7, #0x2A ; subroutine body
        //   0x4000_000C: RET X30        ; indirect branch back to 0x4000_0004
        //   0x4000_0010: WFI           ; park
        let words = [
            0x9400_0002, // BL +2
            0x1400_0003, // B +3
            0xD280_0547, // MOVZ X7, #0x2A
            0xD65F_03C0, // RET X30
            0xD503_207F, // WFI
        ];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0010 });
        assert_eq!(o.steps(), 5);
        // Link register holds the return address; the body ran; control
        // returned through the register-held target, not a static exit.
        assert_eq!(o.machine.cpu[0].regs[30], 0x4000_0004);
        assert_eq!(o.machine.cpu[0].regs[7], 0x2A);
        assert_eq!(o.machine.cpu[0].pc, 0x4000_0014);
    }

    #[test]
    fn wave5_real_guest_reaches_shell_prompt() {
        // The real 4.1 `pathn-sh` guest. Wave 4 halted at the first BL
        // (0x4000_004C); with BL/RET lifted the guest now runs THROUGH the
        // call into print_cstr, prints the prompt, returns, runs the read
        // loop, and parks at WFI waiting for input — the shell is live.
        let (image, _sbom) =
            guest_image::image::build(&guest_image::image::GuestManifest::pathn_sh());
        let mut o = Orchestrator::new();
        o.load_image(&image).unwrap();
        let reason = o.run_until_halt(10_000);
        let cpu = &o.machine.cpu[0];
        // Measured acceptance values (also printed for the Wave-5 report).
        println!("halt: {reason:?}");
        println!("pc: {:#x}", cpu.pc);
        println!("steps: {}", o.steps());
        println!("x10: {:#x}", cpu.regs[10]);
        println!("x0: {:#x}", cpu.regs[0]);
        println!("x30: {:#x}", cpu.regs[30]);
        println!("console: {:?}", o.console.tx_bytes);
        // 74 instructions executed: entry -> prompt -> BL print_cstr ->
        // 10-char print loop -> RET -> read_loop -> BL read_char -> RET
        // (x0 = 0, no input) -> CBNZ falls through -> WFI parks.
        assert_eq!(o.steps(), 74);
        // Parked at the read_loop WFI (0x4000_0070), resumable — not halted.
        assert_eq!(reason, HaltReason::Wfi { addr: 0x4000_0070 });
        assert_eq!(cpu.pc, 0x4000_0074);
        // x10 (data base) survived the calls; x0 = 0 (read_char: no byte).
        assert_eq!(cpu.regs[10], 0x4000_1000);
        assert_eq!(cpu.regs[0], 0x0);
        // The prompt was actually printed through the console MMIO.
        assert_eq!(o.console.tx_bytes, b"pathn-sh> ");
    }

    // ---- GB-2: branches + flags vCPU golden-word tests ----

    #[test]
    fn gb2_cmp_and_b_cond_eq_ne() {
        // Test CMP (immediate) setting Z flag and B.EQ / B.NE branch execution.
        // Sequence:
        // 0x4000_0000: CMP X0, #5   (0xF100141F)
        // 0x4000_0004: B.EQ +8      (0x54000040) -> branches to 0x4000_000C
        // 0x4000_0008: MOVZ X1, #1  (0xD2800021) -> skipped
        // 0x4000_000C: MOVZ X2, #2  (0xD2800042)
        // 0x4000_0010: WFI          (0xD503207F)
        let words = [
            0xF100_141F, // CMP X0, #5
            0x5400_0040, // B.EQ +8
            0xD280_0021, // MOVZ X1, #1
            0xD280_0042, // MOVZ X2, #2
            0xD503_207F, // WFI
        ];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[0] = 5; // X0 = 5 -> 5 - 5 == 0 -> Z=1

        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0010 });
        assert_eq!(o.machine.cpu[0].regs[1], 0); // skipped!
        assert_eq!(o.machine.cpu[0].regs[2], 2); // executed!
        assert_eq!(o.machine.cpu[0].pstate & FLAG_Z, FLAG_Z);
    }

    #[test]
    fn gb2_cmp_and_b_cond_not_taken() {
        // Same sequence, but X0 = 6 -> CMP X0, #5 sets Z=0, so B.EQ is not taken.
        let words = [
            0xF100_141F, // CMP X0, #5
            0x5400_0040, // B.EQ +8
            0xD280_0021, // MOVZ X1, #1
            0xD280_0042, // MOVZ X2, #2
            0xD503_207F, // WFI
        ];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[0] = 6; // X0 = 6 -> Z=0

        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0010 });
        assert_eq!(o.machine.cpu[0].regs[1], 1); // executed!
        assert_eq!(o.machine.cpu[0].regs[2], 2); // executed!
        assert_eq!(o.machine.cpu[0].pstate & FLAG_Z, 0);
    }

    #[test]
    fn gb2_tbz_and_tbnz_execution() {
        // Test TBZ (test bit zero) and TBNZ (test bit nonzero)
        // 0x4000_0000: TBZ X0, #2, +8  (0x36100040) -> bit 2 of 1 is 0 -> TAKEN to 0x4000_0008
        // 0x4000_0004: MOVZ X1, #99    (0xD2800C61) -> skipped
        // 0x4000_0008: TBNZ X0, #0, +8 (0x37000040) -> bit 0 of 1 is 1 -> TAKEN to 0x4000_0010
        // 0x4000_000C: MOVZ X2, #99    (0xD2800C62) -> skipped
        // 0x4000_0010: WFI             (0xD503207F)
        let words = [
            0x3610_0040, // TBZ W0, #2, +8
            0xD280_0C61, // MOVZ X1, #99
            0x3700_0040, // TBNZ W0, #0, +8
            0xD280_0C62, // MOVZ X2, #99
            0xD503_207F, // WFI
        ];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[0] = 1; // bit 0 = 1, bit 2 = 0

        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0010 });
        assert_eq!(o.machine.cpu[0].regs[1], 0); // skipped
        assert_eq!(o.machine.cpu[0].regs[2], 0); // skipped
    }

    #[test]
    fn gb2_cmn_and_tst_execution() {
        // CMN X0, #5: ADDS XZR, X0, #5
        // TST X0, X1: ANDS XZR, X0, X1
        let words = [
            0xB100_141F, // CMN W0, #5 (ADDS WZR, W0, #5)
            0x5400_0040, // B.EQ +8
            0xD280_0021, // MOVZ X1, #1 (skipped if Z=1)
            0xEA02_001F, // TST X0, X2 (ANDS XZR, X0, X2)
            0x5400_0041, // B.NE +8
            0xD280_0063, // MOVZ X3, #3 (skipped if Z=0)
            0xD503_207F, // WFI
        ];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[0] = (-5i64) as u64; // (-5) + 5 == 0 -> Z=1
        o.machine_mut().cpu[0].regs[2] = 0xFF;           // (-5) & 0xFF != 0 -> Z=0

        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0018 });
        assert_eq!(o.machine.cpu[0].regs[1], 0); // skipped by B.EQ
        assert_eq!(o.machine.cpu[0].regs[3], 0); // skipped by B.NE
    }

    // ---- GB-15: conditional select (CSEL / CSINC / CSINV / CSNEG) ----

    #[test]
    fn gb15_csel_hi_false_selects_rm() {
        // The real guest sequence (x5 = min_unsigned(x5, 5)):
        // 0x4000_0000: CMP X5, X6       (0xEB0600BF) -> 0 - 5 borrows -> C=0
        // 0x4000_0004: CSEL X5,X6,X5,HI (0x9A8580C5) -> HI false -> X5 = X5
        // 0x4000_0008: WFI
        let words = [0xEB06_00BF, 0x9A85_80C5, 0xD503_207F];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[5] = 0;
        o.machine_mut().cpu[0].regs[6] = 5;
        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0008 });
        assert_eq!(o.machine.cpu[0].regs[5], 0); // HI false -> Rm kept
    }

    #[test]
    fn gb15_csel_hi_true_selects_rn() {
        // CMP X6, X5 (0xEB0500DF): 5 - 0, no borrow -> C=1, Z=0 -> HI true.
        // CSEL X5, X6, X5, HI -> X5 = X6 = 5.
        let words = [0xEB05_00DF, 0x9A85_80C5, 0xD503_207F];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[5] = 0;
        o.machine_mut().cpu[0].regs[6] = 5;
        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0008 });
        assert_eq!(o.machine.cpu[0].regs[5], 5); // HI true -> Rn taken
    }

    #[test]
    fn gb15_csinc_csinv_csneg_else_transforms() {
        // X5 = 0x10. CMP X0, X0 -> Z=1 -> EQ true: CSINC takes Rn (XZR).
        // CMP X0, #1 -> Z=0 -> EQ false: else-operand transforms apply.
        // 0x4000_0000: CMP X0, X0          (0xF100001F)
        // 0x4000_0004: CSINC X7,XZR,X5,EQ (0x9A8507E7) -> X7 = 0
        // 0x4000_0008: CMP X0, #1          (0xF100041F)
        // 0x4000_000C: CSINC X8,XZR,X5,EQ (0x9A8507E8) -> X8 = 0x11
        // 0x4000_0010: CSINV X9,XZR,X5,EQ (0x9A850BE9) -> X9 = !0x10
        // 0x4000_0014: CSNEG X10,XZR,X5,EQ(0x9A850FEA) -> X10 = -0x10
        // 0x4000_0018: WFI
        let words = [
            0xF100_001F,
            0x9A85_07E7,
            0xF100_041F,
            0x9A85_07E8,
            0x9A85_0BE9,
            0x9A85_0FEA,
            0xD503_207F,
        ];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        o.machine_mut().cpu[0].regs[5] = 0x10;
        let halt = o.run_until_halt(100);
        assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0018 });
        assert_eq!(o.machine.cpu[0].regs[7], 0); // EQ true -> Rn (XZR)
        assert_eq!(o.machine.cpu[0].regs[8], 0x11); // CSINC: m + 1
        assert_eq!(o.machine.cpu[0].regs[9], !0x10u64); // CSINV: !m
        assert_eq!(o.machine.cpu[0].regs[10], 0xFFFF_FFFF_FFFF_FFF0); // CSNEG: -m
    }

    #[test]
    fn gb15_csel_32bit_traps_unsupported() {
        // CSEL W5, W6, W5, HI (sf = 0): recognized, explicitly trapped.
        let words = [0x1A85_80C5];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        let halt = o.run_until_halt(100);
        assert_eq!(
            halt,
            HaltReason::Unsupported {
                addr: 0x4000_0000,
                reason: "CSEL: 32-bit form not implemented",
            }
        );
    }

    #[test]
    fn gb15_csel_cond_nv_traps_unsupported() {
        // CSEL X5, X6, X5, cond=0b1111: unallocated, explicitly trapped.
        let words = [0x9A85_F0C5];
        let mut o = Orchestrator::new();
        o.load_image(&minimal_image(0x4000_0000, &words)).unwrap();
        let halt = o.run_until_halt(100);
        assert_eq!(
            halt,
            HaltReason::Unsupported {
                addr: 0x4000_0000,
                reason: "CSEL: cond 0b1111 is unallocated",
            }
        );
    }

    // ---- GB-20: MMU data-access translation in WasmHost ----

    /// Real AOSP kernel V01 translation vector (GB-19 halt, step 7533).
    /// VA 0xFFFF_FF80_096A_B158 -> PA 0x416A_B158 via TTBR1, 2 MiB block.
    fn v01_sysregs() -> SysRegs {
        SysRegs {
            sctlr_el1: 0x34f5_d91d, // M=1
            tcr_el1: 0x0040_0030_b559_3519,
            ttbr0_el1: 0x4166_5000,
            ttbr1_el1: 0x4166_a000,
            ..Default::default()
        }
    }

    /// RAM slice starting at RAM_BASE carrying the V01 page tables:
    /// L1[0]@0x4166a000 = 0x4166b003, L2[75]@0x4166b258 = 0x41600711.
    fn v01_ram() -> Vec<u8> {
        let mut ram = vec![0u8; 0x170_0000];
        let w = |ram: &mut Vec<u8>, pa: u64, v: u64| {
            let s = (pa - RAM_BASE) as usize;
            ram[s..s + 8].copy_from_slice(&v.to_le_bytes());
        };
        w(&mut ram, 0x4166_a000, 0x4166_b003);
        w(&mut ram, 0x4166_b258, 0x4160_0711);
        ram
    }

    struct HostParts {
        ram: Vec<u8>,
        console: ConsoleState,
        gpu_port: GpuPort,
        sysregs: SysRegs,
    }

    fn host_parts(sysregs: SysRegs) -> HostParts {
        HostParts {
            ram: v01_ram(),
            console: ConsoleState::new(),
            gpu_port: GpuPort::new(),
            sysregs,
        }
    }

    /// Split-borrow helper: build a WasmHost over the parts.
    fn with_host<T>(p: &mut HostParts, f: impl FnOnce(&mut WasmHost) -> T) -> T {
        let mut host = WasmHost {
            ram: &mut p.ram,
            console: &mut p.console,
            gpu_port: &mut p.gpu_port,
            sysregs: &mut p.sysregs,
        };
        f(&mut host)
    }

    #[test]
    fn gb20_host_translated_read() {
        let mut p = host_parts(v01_sysregs());
        // Known pattern at physical 0x416AB158.
        let off = (0x416A_B158 - RAM_BASE) as usize;
        p.ram[off..off + 8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        let got = with_host(&mut p, |h| {
            h.mem_load(0xFFFF_FF80_096A_B158u64 as i64, 8)
        });
        assert_eq!(got, Ok(0x1122_3344_5566_7788u64 as i64));
    }

    #[test]
    fn gb20_host_translated_store() {
        let mut p = host_parts(v01_sysregs());
        let r = with_host(&mut p, |h| {
            h.mem_store(
                0xFFFF_FF80_096A_B158u64 as i64,
                8,
                0xAABB_CCDD_EEFF_0011u64 as i64,
            )
        });
        assert_eq!(r, Ok(()));
        let off = (0x416A_B158 - RAM_BASE) as usize;
        let mut b = [0u8; 8];
        b.copy_from_slice(&p.ram[off..off + 8]);
        assert_eq!(u64::from_le_bytes(b), 0xAABB_CCDD_EEFF_0011);
    }

    #[test]
    fn gb20_host_mmu_off_is_identity() {
        let mut sysregs = v01_sysregs();
        sysregs.sctlr_el1 = 0; // MMU off: no walk, identity map.
        let mut p = host_parts(sysregs);
        // Physical read at RAM_BASE works with no page tables consulted.
        let got = with_host(&mut p, |h| h.mem_load(RAM_BASE as i64, 1));
        assert_eq!(got, Ok(0));
    }

    #[test]
    fn gb20_host_translation_fault_surfaces_as_err() {
        // MMU on but TTBR0 points below RAM_BASE: the walk faults.
        // WasmHost reports `mmu_fault: ...`; step_vcpu turns any host
        // Err into HaltReason::WasmTrap (existing path).
        let mut sysregs = SysRegs::default();
        sysregs.sctlr_el1 = 1;
        sysregs.tcr_el1 = 16; // 4K granule, 48-bit, low region
        sysregs.ttbr0_el1 = 0x1000; // below RAM_BASE -> rebase underflow
        let mut p = host_parts(sysregs);
        let err = with_host(&mut p, |h| h.mem_load(0x401234, 8)).unwrap_err();
        assert!(
            err.starts_with("mmu_fault:"),
            "expected mmu_fault prefix, got: {err}"
        );
    }

    #[test]
    fn gb21_blr_to_kernel_va_does_not_exit_vm() {
        // Regression: the ExitVm sentinel is exactly -1. A dynamic-branch
        // target in the kernel high half (0xFFFF_...) is negative as i64
        // but is NOT the sentinel — the old `exit_addr < 0` check falsely
        // halted the real kernel's BLR X8 to 0xffffff80095c0280 as ExitVm.
        let mut o = Orchestrator::new();
        {
            let m = o.machine_mut();
            // BLR X8 at the reset vector (MMU off: identity fetch).
            m.ram[0..4].copy_from_slice(&0xD63F_0100u32.to_le_bytes());
            m.cpu[0].regs[8] = 0xffff_ff80_095c_0280;
        }
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].pc, 0xffff_ff80_095c_0280);
        assert_eq!(o.machine().cpu[0].regs[30], 0x4000_0004); // link written
        assert_eq!(o.halted(), None);
    }

    #[test]
    fn gb21_exact_sentinel_still_exits_vm() {
        // The exact -1 sentinel still means ExitVm: a dynamic branch to
        // 0xFFFF_FFFF_FFFF_FFFF halts (it is not a translatable address).
        let mut o = Orchestrator::new();
        {
            let m = o.machine_mut();
            m.ram[0..4].copy_from_slice(&0xD63F_0100u32.to_le_bytes()); // BLR X8
            m.cpu[0].regs[8] = 0xffff_ffff_ffff_ffff;
        }
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Halted(HaltReason::ExitVm)),
            "expected ExitVm halt, got: {outcome:?}"
        );
    }

    #[test]
    fn gb22_fetch_translates_kernel_va() {
        // GB-22: fetch_word goes through stage-1 translation. With the V01
        // page tables (VA 0xffffff80096abxxx -> PA 0x416abxxx, verified in
        // GB-20), a NOP at PA 0x416ab000 must be fetched when pc is the
        // kernel VA 0xffffff80096ab000.
        let mut o = Orchestrator::new();
        {
            let m = o.machine_mut();
            // V01 page tables: L1[0]@0x4166a000, L2[75]@0x4166b258.
            let w = |ram: &mut Vec<u8>, pa: u64, v: u64| {
                let s = (pa - RAM_BASE) as usize;
                ram[s..s + 8].copy_from_slice(&v.to_le_bytes());
            };
            w(&mut m.ram, 0x4166_a000, 0x4166_b003);
            w(&mut m.ram, 0x4166_b258, 0x4160_0711);
            // NOP (0xD503201F) at PA 0x416ab000.
            let s = (0x416a_b000 - RAM_BASE) as usize;
            m.ram[s..s + 4].copy_from_slice(&0xD503_201Fu32.to_le_bytes());
            // MMU on, V01 registers.
            m.cpu[0].sysregs.sctlr_el1 = 0x34f5_d91d; // M=1
            m.cpu[0].sysregs.tcr_el1 = 0x0040_0030_b559_3519;
            m.cpu[0].sysregs.ttbr0_el1 = 0x4166_5000;
            m.cpu[0].sysregs.ttbr1_el1 = 0x4166_a000;
            m.cpu[0].pc = 0xffff_ff80_096a_b000;
        }
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].pc, 0xffff_ff80_096a_b004);
        assert_eq!(o.halted(), None);
    }

    #[test]
    fn gb22_fetch_unmapped_va_is_fetch_fault() {
        // GB-22: a fetch VA with no mapping faults honestly as FetchFault
        // at the VA (not a silent wrong-physical read).
        let mut o = Orchestrator::new();
        {
            let m = o.machine_mut();
            m.cpu[0].sysregs.sctlr_el1 = 0x34f5_d91d; // M=1, no tables
            m.cpu[0].sysregs.tcr_el1 = 0x0040_0030_b559_3519;
            m.cpu[0].sysregs.ttbr0_el1 = 0x4166_5000;
            m.cpu[0].sysregs.ttbr1_el1 = 0x4166_a000;
            m.cpu[0].pc = 0xffff_ff80_096a_b000;
        }
        let outcome = o.step_vcpu();
        assert!(
            matches!(
                outcome,
                StepOutcome::Halted(HaltReason::FetchFault { addr: 0xffff_ff80_096a_b000 })
            ),
            "expected FetchFault, got: {outcome:?}"
        );
    }

    /// Helper: fresh Orchestrator with MMU off (identity), PC at `pc`,
    /// SP at `sp`, and `word` written at PC.
    fn sp_test_orchestrator(pc: u64, sp: u64, word: u32) -> Orchestrator {
        let mut o = Orchestrator::new();
        {
            let m = o.machine_mut();
            m.cpu[0].sysregs.sctlr_el1 = 0; // MMU off: identity map.
            let s = (pc - RAM_BASE) as usize;
            m.ram[s..s + 4].copy_from_slice(&word.to_le_bytes());
            m.cpu[0].pc = pc;
            m.cpu[0].sp = sp;
        }
        o
    }

    #[test]
    fn gb23_sp_str_imm_64() {
        // GB-23: STR X1, [SP, #16] stores X1 at SP+16.
        // 1111100000 imm12=2 11111 00001 = 0xF8000BE1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF800_0BE1);
        o.machine_mut().cpu[0].regs[1] = 0xDEAD_BEEF_CAFE_1234;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp + 16 - RAM_BASE) as usize;
        let mut b = [0u8; 8];
        b.copy_from_slice(&o.machine().ram[s..s + 8]);
        assert_eq!(u64::from_le_bytes(b), 0xDEAD_BEEF_CAFE_1234);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb23_sp_ldr_imm_64() {
        // GB-23: LDR X2, [SP, #16] loads SP+16 into X2.
        // 1111100001 imm12=2 11111 00010 = 0xF8400BE2.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF840_0BE2);
        let s = (sp + 16 - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[2], 0x1122_3344_5566_7788);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb23_sp_str_imm_32() {
        // GB-23: STR W3, [SP, #8] stores low 32 bits of X3 at SP+8.
        // 1011100000 imm12=2 11111 00011 = 0xB8000BE3.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xB800_0BE3);
        o.machine_mut().cpu[0].regs[3] = 0xFFFF_FFFF_ABCD_1234;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp + 8 - RAM_BASE) as usize;
        let mut b = [0u8; 4];
        b.copy_from_slice(&o.machine().ram[s..s + 4]);
        assert_eq!(u32::from_le_bytes(b), 0xABCD_1234);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb23_sp_stp_pair() {
        // GB-23: STP X1, X2, [SP, #-16] stores the pair at SP-16, SP-8.
        // 1010100110 imm7=-2(0x7E) 00010 11111 00001 = 0xA9BF0BE1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xA9BF_0BE1);
        o.machine_mut().cpu[0].regs[1] = 0x1111_1111_1111_1111;
        o.machine_mut().cpu[0].regs[2] = 0x2222_2222_2222_2222;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp - 16 - RAM_BASE) as usize;
        let mut b = [0u8; 8];
        b.copy_from_slice(&o.machine().ram[s..s + 8]);
        assert_eq!(u64::from_le_bytes(b), 0x1111_1111_1111_1111);
        b.copy_from_slice(&o.machine().ram[s + 8..s + 16]);
        assert_eq!(u64::from_le_bytes(b), 0x2222_2222_2222_2222);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb23_sp_ldp_pair() {
        // GB-23: LDP X1, X2, [SP, #-16] loads the pair from SP-16, SP-8.
        // 1010100101 imm7=-2(0x7E) 00010 11111 00001 = 0xA97F0BE1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xA97F_0BE1);
        let s = (sp - 16 - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 8].copy_from_slice(&0xAAAA_AAAA_AAAA_AAAAu64.to_le_bytes());
        o.machine_mut().ram[s + 8..s + 16]
            .copy_from_slice(&0xBBBB_BBBB_BBBB_BBBBu64.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[1], 0xAAAA_AAAA_AAAA_AAAA);
        assert_eq!(o.machine().cpu[0].regs[2], 0xBBBB_BBBB_BBBB_BBBB);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb23_sp_store_does_not_clobber_xzr() {
        // GB-23: STR XZR, [SP, #0] stores 0 (XZR read), not SP.
        // 1111100000 imm12=0 11111 11111 = 0xF80003FF.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF800_03FF);
        let s = (sp - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 8].copy_from_slice(&0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let mut b = [0u8; 8];
        b.copy_from_slice(&o.machine().ram[s..s + 8]);
        assert_eq!(u64::from_le_bytes(b), 0);
    }

    #[test]
    fn gb24_add_sp_imm_updates_sp() {
        // GB-24: ADD SP, SP, #16 updates SP (Rd=31 means SP for S=0).
        // 1001000100 imm12=16 11111 11111 = 0x910040FF.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x9100_43FF);
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].sp, sp + 16);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb24_sub_sp_imm_updates_sp() {
        // GB-24: SUB SP, SP, #32 updates SP.
        // 1101000100 imm12=32 11111 11111 = 0xD10080FF.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xD100_83FF);
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].sp, sp - 32);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb24_add_x0_sp_imm_reads_sp() {
        // GB-24: ADD X0, SP, #8 reads SP (Rn=31) into X0.
        // 1001000100 imm12=8 11111 00000 = 0x910020E0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x9100_23E0);
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[0], sp + 8);
        // SP unchanged.
        assert_eq!(o.machine().cpu[0].sp, sp);
    }

    #[test]
    fn gb25_orr_32() {
        // GB-25: ORR W0, W1, W2 (32-bit). Rd = Rn | Rm, upper 32 zeroed.
        // 00101010 00 0 00010 000000 00001 00000 = 0x2A020020.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x2A02_0020);
        o.machine_mut().cpu[0].regs[1] = 0xFFFF_FFFF_0000_00F0;
        o.machine_mut().cpu[0].regs[2] = 0x0000_0000_0000_0F0F;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        // 32-bit: (0x000000F0 | 0x00000F0F) = 0x00000FFF, upper zeroed.
        assert_eq!(o.machine().cpu[0].regs[0], 0x0000_0FFF);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb25_orr_64() {
        // GB-25: ORR X0, X1, X2 (64-bit).
        // 10101010 00 0 00010 000000 00001 00000 = 0xAA020020.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xAA02_0020);
        o.machine_mut().cpu[0].regs[1] = 0xF0F0_0000_0000_00F0;
        o.machine_mut().cpu[0].regs[2] = 0x0F0F_FFFF_FFFF_0F0F;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[0], 0xFFFF_FFFF_FFFF_0FFF);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb25_orr_32_shifted() {
        // GB-25: ORR W0, W1, W2, LSL #4 (32-bit with shift).
        // 00101010 00 0 00010 000100 00001 00000 = 0x2A021020.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x2A02_1020);
        o.machine_mut().cpu[0].regs[1] = 0x0000_00F0;
        o.machine_mut().cpu[0].regs[2] = 0x0000_000F;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        // W2 << 4 = 0xF0, 0xF0 | 0xF0 = 0xF0.
        assert_eq!(o.machine().cpu[0].regs[0], 0x0000_00F0);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }
}
