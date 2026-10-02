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
/// Guest RAM size: 1 GiB, matches QEMU -m 1024 (required for DTB at 0x48000000).
pub const RAM_SIZE: u64 = 0x4000_0000;
/// Console MMIO base (PLATFORM.md).
pub const CONSOLE_BASE: u64 = 0x0900_0000;
/// Console MMIO size: one page (PLATFORM.md).
pub const CONSOLE_SIZE: u64 = 0x1000;
/// STRB a byte here → host emits it (PLATFORM.md).
pub const CONSOLE_TX: u64 = 0x0900_0000;
/// LDRB here → next input byte, or 0 if none (PLATFORM.md).
pub const CONSOLE_RX: u64 = 0x0900_0008;
/// PL011 Flag Register offset within the console window.
///
/// A real PL011 driver (Linux `amba-pl011`) polls `FR.TXFF` before every
/// `DR` write and uses 32-bit `readl`/`writel`. The orchestrator answers
/// 32-bit reads here with `TXFE|RXFE` (transmit never full, receive always
/// empty) so such a driver can make progress; all other PL011 registers
/// read as 0. Added 2026-10-02 (fam/uart-sandbox): without this, a 32-bit
/// `LDR` from `FR` was rejected as "byte-only" and would halt the guest.
pub const CONSOLE_PL011_FR: u64 = 0x0900_0018;
/// PL011 Flag Register value: TXFE (bit 7) | RXFE (bit 4). TXFF (bit 5)
/// clear → a polling driver never spins; BUSY never asserted.
pub const CONSOLE_PL011_FR_VALUE: u32 = 0x90;

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
    Svc {
        addr: u64,
    },
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

/// Recorded unimplemented or illegal instruction in survey mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurveyMiss {
    pub step: u64,
    pub pc: u64,
    pub word: u32,
    pub kind: InsnKind,
    pub mnemonic: &'static str,
}

/// Best-effort classification of an instruction word for survey mode logging.
/// Pure function: word -> (InsnKind, &'static str).
pub fn best_effort_classify(word: u32) -> (InsnKind, &'static str) {
    // 1. Branches & Exceptions & System
    if word & 0xFC00_0000 == 0x1400_0000 {
        return (InsnKind::Branch, "B");
    }
    if word & 0xFC00_0000 == 0x9400_0000 {
        return (InsnKind::Branch, "BL");
    }
    if word & 0xFF00_0000 == 0x5400_0000 {
        return (InsnKind::Branch, "B.cond");
    }
    if word & 0x7F00_0000 == 0x3400_0000 {
        return (InsnKind::Branch, "CBZ");
    }
    if word & 0x7F00_0000 == 0x3500_0000 {
        return (InsnKind::Branch, "CBNZ");
    }
    if word & 0x7F00_0000 == 0x3600_0000 {
        return (InsnKind::Branch, "TBZ");
    }
    if word & 0x7F00_0000 == 0x3700_0000 {
        return (InsnKind::Branch, "TBNZ");
    }
    if word == 0xD69F_03E0 {
        return (InsnKind::Branch, "ERET");
    }
    if word & 0xFE00_0000 == 0xD600_0000 {
        let op = (word >> 21) & 0xF;
        return match op {
            0b0001 => (InsnKind::Branch, "BR"),
            0b0010 => (InsnKind::Branch, "BLR"),
            0b0100 => (InsnKind::Branch, "RET"),
            _ => (InsnKind::Branch, "Branch (reg)"),
        };
    }
    if word & 0xFF80_0000 == 0xD400_0000 {
        // Exception generation: opc = bits[23:21], LL = bits[1:0].
        // SVC/HVC/SMC share opc=000 and differ by LL; BRK is opc=001, HLT opc=010.
        let opc = (word >> 21) & 0x7;
        let ll = word & 0x3;
        return match (opc, ll) {
            (0b000, 0b01) => (InsnKind::Svc, "SVC"),
            (0b000, 0b10) => (InsnKind::System, "HVC"),
            (0b000, 0b11) => (InsnKind::System, "SMC"),
            (0b001, _) => (InsnKind::System, "BRK"),
            (0b010, _) => (InsnKind::System, "HLT"),
            _ => (InsnKind::System, "Exception"),
        };
    }
    if word & 0xFFC0_0000 == 0xD500_0000 {
        if word & 0xFFFF_F000 == 0xD503_2000 || word & 0xFFFF_F000 == 0xD503_3000 {
            return (InsnKind::System, "HINT / NOP / Barrier");
        }
        let op0 = (word >> 19) & 0x7;
        return match op0 {
            0 => (InsnKind::System, "MSR (imm)"),
            1 => (InsnKind::System, "SYS"),
            3 => (InsnKind::System, "MSR (reg)"),
            7 => (InsnKind::System, "MRS"),
            _ => (InsnKind::System, "System"),
        };
    }

    // 2. PC-relative addressing
    if word & 0x1F00_0000 == 0x1000_0000 {
        let op = (word >> 31) & 1;
        return if op == 1 {
            (InsnKind::PcRel, "ADRP")
        } else {
            (InsnKind::PcRel, "ADR")
        };
    }

    // 3. Data Processing - Immediate
    if word & 0x1F00_0000 == 0x1100_0000 {
        return (InsnKind::DataProc, "ADD/SUB (imm)");
    }
    if word & 0x1F80_0000 == 0x1200_0000 {
        return (InsnKind::DataProc, "Logical (imm)");
    }
    if word & 0x1F80_0000 == 0x1280_0000 {
        return (InsnKind::DataProc, "MOV wide (imm)");
    }
    if word & 0x1F00_0000 == 0x1300_0000 {
        return (InsnKind::DataProc, "Bitfield");
    }

    // 4. Data Processing - Register
    if word & 0x1F00_0000 == 0x0B00_0000 {
        return (InsnKind::DataProc, "ADD/SUB (shifted reg)");
    }
    if word & 0x1F00_0000 == 0x0A00_0000 {
        return (InsnKind::DataProc, "Logical (shifted reg)");
    }
    if word & 0x1F00_0000 == 0x1B00_0000 {
        return (InsnKind::DataProc, "MADD / MSUB");
    }
    if word & 0x7FE0_0000 == 0x1AC0_0000 {
        return (InsnKind::DataProc, "DP (2-source)");
    }
    if word & 0x7FE0_0000 == 0x1A80_0000
        || word & 0x7FE0_0000 == 0x3A40_0000
        || word & 0x7FE0_0000 == 0x7A40_0000
    {
        return (InsnKind::DataProc, "Conditional Select / Compare");
    }
    if word & 0x7FE0_0000 == 0x5AC0_0000 {
        return (InsnKind::DataProc, "DP (1-source: CLZ/REV)");
    }

    // 5. Loads and Stores
    let top4 = (word >> 25) & 0xF;
    if top4 == 0b0100 || top4 == 0b1100 {
        if (word >> 24) & 0x3F == 0b001000 {
            if (word >> 23) & 1 == 1 && (word >> 21) & 1 == 1 {
                return (InsnKind::LoadStore, "CAS");
            } else if (word >> 23) & 1 == 1 && (word >> 21) & 1 == 0 {
                return (InsnKind::LoadStore, "LDAR / STLR");
            } else if (word >> 23) & 1 == 0 {
                return (InsnKind::LoadStore, "LDXR / STXR");
            }
        } else if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 {
            if (word >> 15) & 1 == 1 && (word >> 10) & 0x1F == 0 {
                return (InsnKind::LoadStore, "SWP");
            } else if (word >> 16) & 0x1F == 0b11111 && (word >> 10) & 0x3F == 0b110000 {
                return (InsnKind::LoadStore, "LDAPR");
            } else if (word >> 10) & 0x3 == 0 {
                return (InsnKind::LoadStore, "LSE Atomic");
            }
        }
        return (InsnKind::LoadStore, "LDR / STR");
    }

    // 6. FP / SIMD
    if top4 == 0b0111 || top4 == 0b1111 || top4 == 0b0110 || top4 == 0b1110 {
        return (InsnKind::Unknown, "FP / SIMD");
    }

    (InsnKind::Unknown, "Unknown")
}

/// True for guest exception-generation traps (BRK/HLT) that must NOT be
/// counted as unimplemented-instruction misses in survey mode.
/// BRK is opc=001 (0xD420_0000), HLT is opc=010 (0xD440_0000); the mask keeps
/// bits[31:21] so imm16/op2/LL variants are all recognized as traps.
pub fn is_guest_trap_word(word: u32) -> bool {
    matches!(word & 0xFFE0_0000, 0xD420_0000 | 0xD440_0000)
}

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
    /// Optional trace writer for per-step tracing.
    #[cfg(not(target_arch = "wasm32"))]
    trace_writer: Option<std::io::BufWriter<std::fs::File>>,
    /// Survey mode: skips unimplemented/illegal instructions for opcode discovery.
    /// Hard rule: discovery only — never progress.
    pub survey_mode: bool,
    /// First unimplemented/illegal miss per InsnKind seen during survey mode.
    pub survey_first_miss: Vec<SurveyMiss>,
    /// Guest traps (BRK/HLT) seen during survey mode, first per mnemonic.
    /// Bucketed separately: a guest trap is guest behavior, NOT an
    /// unimplemented instruction, and must never inflate the miss list.
    pub survey_traps: Vec<SurveyMiss>,
    /// Per-queue last-notified used index for EVENT_IDX decisions.
    last_notified: Vec<u16>,
    /// Local exclusive monitor for LDXR/STXR (GB-26). `Some((addr, size))`
    /// after a load-exclusive; cleared by store-exclusive or CLREX.
    /// Single-vCPU: no other agent can clear it, so STXR succeeds iff the
    /// monitor still holds the accessed address.
    exclusive: Option<(u64, u8)>,
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
        #[allow(unused_mut)]
        let mut orch = Self {
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
            #[cfg(not(target_arch = "wasm32"))]
            trace_writer: None,
            survey_mode: false,
            survey_first_miss: Vec::new(),
            survey_traps: Vec::new(),
            last_notified: Vec::new(),
            exclusive: None,
            executor: default_executor(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(trace_path) = std::env::var("PATHN_TRACE") {
                if !trace_path.is_empty() {
                    let _ = orch.enable_trace_file(&trace_path);
                }
            }
        }
        orch
    }

    /// Enable instruction tracing to the specified file path.
    /// Format per instruction: `step pc word x0..x30 sp nzcv`
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_trace_file(&mut self, path: &str) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        self.trace_writer = Some(std::io::BufWriter::new(file));
        Ok(())
    }

    /// Flush any pending trace output.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn flush_trace(&mut self) {
        if let Some(ref mut w) = self.trace_writer {
            let _ = std::io::Write::flush(w);
        }
    }

    /// Configure survey mode.
    ///
    /// HARD RULE: Survey mode is discovery only — it must NEVER be used to
    /// claim boot progress or move any baseline.
    pub fn set_survey_mode(&mut self, enabled: bool) {
        self.survey_mode = enabled;
    }

    /// Check if survey mode is active.
    pub fn survey_mode(&self) -> bool {
        self.survey_mode
    }

    /// Read the first-miss-per-class summary recorded in survey mode.
    pub fn survey_summary(&self) -> &[SurveyMiss] {
        &self.survey_first_miss
    }

    /// Read the guest-trap summary recorded in survey mode (BRK/HLT).
    /// Traps are guest behavior, never unimplemented-instruction misses.
    pub fn survey_trap_summary(&self) -> &[SurveyMiss] {
        &self.survey_traps
    }

    /// Print the first-miss-per-class summary.
    pub fn print_survey_summary(&self) {
        println!("=== Survey Mode: First-Miss-Per-Class Summary ===");
        if self.survey_first_miss.is_empty() {
            println!("No unimplemented/illegal instructions encountered.");
        } else {
            for miss in &self.survey_first_miss {
                println!(
                    "{:<12}: first miss at step {:>7}, pc={:#018x}, word={:#010x} ({})",
                    format!("{:?}", miss.kind),
                    miss.step,
                    miss.pc,
                    miss.word,
                    miss.mnemonic
                );
            }
        }
        println!("=== Survey Mode: Guest Traps (BRK/HLT, not unimplemented) ===");
        if self.survey_traps.is_empty() {
            println!("No guest traps encountered.");
            return;
        }
        for trap in &self.survey_traps {
            println!(
                "{:<12}: first trap at step {:>7}, pc={:#018x}, word={:#010x} ({})",
                format!("{:?}", trap.kind),
                trap.step,
                trap.pc,
                trap.word,
                trap.mnemonic
            );
        }
    }

    /// Record a survey miss if the InsnKind has not been seen yet.
    pub fn record_survey_miss(&mut self, miss: SurveyMiss) {
        if !self.survey_first_miss.iter().any(|m| m.kind == miss.kind) {
            self.survey_first_miss.push(miss);
        }
    }

    /// Skip an unimplemented/illegal instruction in survey mode, logging and continuing.
    /// Guest traps (BRK/HLT) are skipped too but bucketed separately — they are
    /// guest behavior, never unimplemented-instruction misses.
    fn survey_skip_instruction(
        &mut self,
        pc: u64,
        word: u32,
        known_kind: Option<InsnKind>,
    ) -> StepOutcome {
        let (best_kind, mnemonic) = best_effort_classify(word);
        let kind = known_kind.unwrap_or(best_kind);
        if is_guest_trap_word(word) {
            println!(
                "SURVEY [step {}]: pc={:#018x} word={:#010x} kind={:?} ({}) - TRAP (guest, not unimplemented) - SKIPPED",
                self.steps, pc, word, kind, mnemonic
            );
            let trap = SurveyMiss {
                step: self.steps,
                pc,
                word,
                kind,
                mnemonic,
            };
            if !self
                .survey_traps
                .iter()
                .any(|m| m.mnemonic == trap.mnemonic)
            {
                self.survey_traps.push(trap);
            }
        } else {
            println!(
                "SURVEY [step {}]: pc={:#018x} word={:#010x} kind={:?} ({}) - SKIPPED",
                self.steps, pc, word, kind, mnemonic
            );
            self.record_survey_miss(SurveyMiss {
                step: self.steps,
                pc,
                word,
                kind,
                mnemonic,
            });
        }
        self.machine.cpu[0].pc = pc.wrapping_add(4);
        self.steps += 1;
        self.tick_clock(TIMER_CYCLES_PER_STEP);
        StepOutcome::Continue
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

    /// Debug: fetch instruction words around a PC for bringup diagnostics.
    pub fn debug_fetch_around(&self, pc: u64, count: usize) -> Vec<(u64, Result<u32, String>)> {
        let mut out = Vec::new();
        for i in 0..count {
            let addr = pc.wrapping_add((i as u64) * 4);
            let word = self.fetch_word(addr).map_err(|e| format!("{:?}", e));
            out.push((addr, word));
        }
        out
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
        let off = self
            .ram_offset(pa, 8)
            .map_err(|_| HaltReason::FetchFault { addr: pa })?;
        Ok(u64::from_le_bytes(
            self.machine.ram[off..off + 8].try_into().unwrap(),
        ))
    }

    fn read_ram_u32(&self, pa: u64) -> Result<u32, HaltReason> {
        let off = self
            .ram_offset(pa, 4)
            .map_err(|_| HaltReason::FetchFault { addr: pa })?;
        Ok(u32::from_le_bytes(
            self.machine.ram[off..off + 4].try_into().unwrap(),
        ))
    }

    fn write_ram_u64(&mut self, pa: u64, val: u64) -> Result<(), HaltReason> {
        let off = self
            .ram_offset(pa, 8)
            .map_err(|_| HaltReason::FetchFault { addr: pa })?;
        self.machine.ram[off..off + 8].copy_from_slice(&val.to_le_bytes());
        Ok(())
    }

    fn write_ram_u32(&mut self, pa: u64, val: u32) -> Result<(), HaltReason> {
        let off = self
            .ram_offset(pa, 4)
            .map_err(|_| HaltReason::FetchFault { addr: pa })?;
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

        // 1. B.cond: 0101010 0 imm19 0 cond (cond 0..15, AL/NV legal and always true)
        if (word >> 24) == 0x54 && (word & 0x10) == 0 {
            let cond = (word & 0xF) as u8;
            let imm19 = ((word >> 5) & 0x7FFFF) as i32;
            let offset = (((imm19 << 13) >> 13) as i64) * 4;
            let target = (pc as i64).wrapping_add(offset) as u64;
            let taken = condition_holds(cond, self.machine.cpu[0].pstate);
            self.machine.cpu[0].pc = if taken { target } else { pc.wrapping_add(4) };
            return Some(Ok(()));
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
            let val = if rt == 31 {
                0
            } else {
                self.machine.cpu[0].regs[rt]
            };
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
            // Rn=31 means SP for ADD/SUB (S=0), XZR for ADDS/SUBS/CMP/CMN (S=1).
            let rn_val = if rn == 31 {
                if s == 1 {
                    0
                } else {
                    self.machine.cpu[0].sp
                }
            } else {
                self.machine.cpu[0].regs[rn]
            };
            if s == 1 {
                if sf == 1 {
                    let nzcv = if op == 0 {
                        let res = rn_val.wrapping_add(imm);
                        if rd != 31 {
                            self.machine.cpu[0].regs[rd] = res;
                        }
                        nzcv_add64(rn_val, imm)
                    } else {
                        let res = rn_val.wrapping_sub(imm);
                        if rd != 31 {
                            self.machine.cpu[0].regs[rd] = res;
                        }
                        nzcv_sub64(rn_val, imm)
                    };
                    self.machine.cpu[0].pstate =
                        (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let a32 = rn_val as u32;
                    let b32 = imm as u32;
                    let nzcv = if op == 0 {
                        let res = a32.wrapping_add(b32);
                        if rd != 31 {
                            self.machine.cpu[0].regs[rd] = res as u64;
                        }
                        nzcv_add32(a32, b32)
                    } else {
                        let res = a32.wrapping_sub(b32);
                        if rd != 31 {
                            self.machine.cpu[0].regs[rd] = res as u64;
                        }
                        nzcv_sub32(a32, b32)
                    };
                    self.machine.cpu[0].pstate =
                        (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
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

        // 4. Add/subtract (shifted register): ADD/SUB (S=0) and
        // ADDS/SUBS/CMP/CMN (S=1).
        // sf op S 01011 shift 0 Rm imm6 Rn Rd
        // GB-26: S=0 was missing -- the kernel hits 32-bit `sub w0, w8, w19`
        // (0x4B130100) at step 1249446, which U2 traps ("32-bit SUB
        // register width is not expressible in IrOp::Sub"). For S=0,
        // Rd=31 means SP (not XZR) and no flags are updated.
        if (word >> 24) & 0x1F == 0x0B {
            let s = (word >> 29) & 1;
            let is_shifted = ((word >> 21) & 1) == 0;
            let shift = ((word >> 22) & 0x3) as u8;
            if is_shifted && shift < 3 {
                let sf = (word >> 31) & 1;
                let op = (word >> 30) & 1; // 0 = ADD, 1 = SUB
                let rm = ((word >> 16) & 0x1F) as usize;
                let imm6 = ((word >> 10) & 0x3F) as u8;
                let rn = ((word >> 5) & 0x1F) as usize;
                let rd = (word & 0x1F) as usize;
                // Rn=31: SP for S=0 (ADD/SUB), XZR for S=1 (ADDS/SUBS/CMP/CMN).
                // Rm=31 is always XZR (zero).
                let rm_val = if rm == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rm]
                };
                if s == 1 {
                    let rn_val = if rn == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rn]
                    };
                    if sf == 1 {
                        let operand2 = eval_shift64(rm_val, shift, imm6);
                        let nzcv = if op == 0 {
                            let res = rn_val.wrapping_add(operand2);
                            if rd != 31 {
                                self.machine.cpu[0].regs[rd] = res;
                            }
                            nzcv_add64(rn_val, operand2)
                        } else {
                            let res = rn_val.wrapping_sub(operand2);
                            if rd != 31 {
                                self.machine.cpu[0].regs[rd] = res;
                            }
                            nzcv_sub64(rn_val, operand2)
                        };
                        self.machine.cpu[0].pstate =
                            (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                    } else {
                        let a32 = rn_val as u32;
                        let b32 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                        let nzcv = if op == 0 {
                            let res = a32.wrapping_add(b32);
                            if rd != 31 {
                                self.machine.cpu[0].regs[rd] = res as u64;
                            }
                            nzcv_add32(a32, b32)
                        } else {
                            let res = a32.wrapping_sub(b32);
                            if rd != 31 {
                                self.machine.cpu[0].regs[rd] = res as u64;
                            }
                            nzcv_sub32(a32, b32)
                        };
                        self.machine.cpu[0].pstate =
                            (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                    }
                } else {
                    // S=0: plain ADD/SUB. Rn=31 means SP, Rd=31 means SP
                    // (not XZR); no flags are updated.
                    let rn_val = if rn == 31 {
                        self.machine.cpu[0].sp
                    } else {
                        self.machine.cpu[0].regs[rn]
                    };
                    if sf == 1 {
                        let operand2 = eval_shift64(rm_val, shift, imm6);
                        let res = if op == 0 {
                            rn_val.wrapping_add(operand2)
                        } else {
                            rn_val.wrapping_sub(operand2)
                        };
                        if rd == 31 {
                            self.machine.cpu[0].sp = res;
                        } else {
                            self.machine.cpu[0].regs[rd] = res;
                        }
                    } else {
                        let a32 = rn_val as u32;
                        let b32 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                        let res = if op == 0 {
                            a32.wrapping_add(b32)
                        } else {
                            a32.wrapping_sub(b32)
                        };
                        // 32-bit form zero-extends (also into SP).
                        if rd == 31 {
                            self.machine.cpu[0].sp = res as u64;
                        } else {
                            self.machine.cpu[0].regs[rd] = res as u64;
                        }
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 4b. Add/subtract (extended register): ADD/SUB (S=0) and
        // ADDS/SUBS/CMP/CMN (S=1), 64-bit.
        // GB-26: the kernel hits `add x2, x22, w23, sxtw` (0x8B37C2C2)
        // at step 1250432, then `cmp x22, w0, sxtw` (0xEB20C2DF) at step
        // 1250446. U1 marks bit21=1 (extend form) out of scope, so they
        // fall through to IllegalInstruction.
        // Encoding: sf op S 01011 opt 1 Rm option imm3 Rn Rd --
        // bits[28:24] == 0b01011 (same as shifted), bit21 == 1 selects
        // the extend form. option = bits[15:13] (extend type), imm3 =
        // bits[12:10] (LSL amount, 0-4). Rm is always 32-bit (Wm);
        // Rn=31 names SP, Rm=31 names WZR. For S=1, Rd=31 discards
        // (CMP/CMN) and NZCV is updated.
        if (word >> 24) & 0x1F == 0x0B && (word >> 21) & 1 == 1 {
            let sf = (word >> 31) & 1;
            let op = (word >> 30) & 1; // 0 = ADD, 1 = SUB
            let s = (word >> 29) & 1;
            // 64-bit only; 32-bit falls through to U2.
            if sf == 1 {
                let rm = ((word >> 16) & 0x1F) as usize;
                let option = (word >> 13) & 0x7;
                let imm3 = (word >> 10) & 0x7;
                let rn = ((word >> 5) & 0x1F) as usize;
                let rd = (word & 0x1F) as usize;
                // Only UXTW/UXTX/SXTW/SXTX are valid for 64-bit ADD/SUB
                // (extend). Byte/halfword options (UXTB/UXTH/SXTB/SXTH)
                // are UNDEFINED — do not claim, fall through.
                let valid_option = matches!(option, 0b010 | 0b011 | 0b110 | 0b111);
                if imm3 <= 4 && valid_option {
                    let w = if rm == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rm] as u32
                    };
                    let extended: u64 = match option {
                        0b000 => (w as u8) as u64,  // UXTB
                        0b001 => (w as u16) as u64, // UXTH
                        0b010 => w as u64,          // UXTW
                        0b011 => {
                            // UXTX
                            if rm == 31 {
                                0
                            } else {
                                self.machine.cpu[0].regs[rm]
                            }
                        }
                        0b100 => (w as i8) as i64 as u64,  // SXTB
                        0b101 => (w as i16) as i64 as u64, // SXTH
                        0b110 => (w as i32) as i64 as u64, // SXTW
                        _ => {
                            // SXTX
                            if rm == 31 {
                                0
                            } else {
                                self.machine.cpu[0].regs[rm]
                            }
                        }
                    };
                    let op2 = extended << imm3;
                    // Rn=31: SP for S=0 (ADD/SUB), XZR for S=1 (ADDS/SUBS/CMP/CMN).
                    let rn_val = if rn == 31 {
                        if s == 1 {
                            0
                        } else {
                            self.machine.cpu[0].sp
                        }
                    } else {
                        self.machine.cpu[0].regs[rn]
                    };
                    if s == 1 {
                        // ADDS/SUBS/CMP/CMN: Rd=31 discards, NZCV updates.
                        let nzcv = if op == 0 {
                            let res = rn_val.wrapping_add(op2);
                            if rd != 31 {
                                self.machine.cpu[0].regs[rd] = res;
                            }
                            nzcv_add64(rn_val, op2)
                        } else {
                            let res = rn_val.wrapping_sub(op2);
                            if rd != 31 {
                                self.machine.cpu[0].regs[rd] = res;
                            }
                            nzcv_sub64(rn_val, op2)
                        };
                        self.machine.cpu[0].pstate =
                            (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                    } else {
                        let res = if op == 0 {
                            rn_val.wrapping_add(op2)
                        } else {
                            rn_val.wrapping_sub(op2)
                        };
                        if rd == 31 {
                            self.machine.cpu[0].sp = res;
                        } else {
                            self.machine.cpu[0].regs[rd] = res;
                        }
                    }
                    self.machine.cpu[0].pc = pc.wrapping_add(4);
                    return Some(Ok(()));
                }
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
                let rn_val = if rn == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                let rm_val = if rm == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rm]
                };
                if sf == 1 {
                    let mut op2 = eval_shift64(rm_val, shift, imm6);
                    if n == 1 {
                        op2 = !op2;
                    }
                    let res = rn_val & op2;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res;
                    }
                    let nzcv = nzcv_and64(res);
                    self.machine.cpu[0].pstate =
                        (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let mut op2 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    if n == 1 {
                        op2 = !op2;
                    }
                    let res = (rn_val as u32) & op2;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res as u64;
                    }
                    let nzcv = nzcv_and32(res);
                    self.machine.cpu[0].pstate =
                        (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
            if opc == 0b00 && n == 1 && shift < 3 {
                // BIC: Rd = Rn & ~shifted(Rm)
                let rn_val = if rn == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                let rm_val = if rm == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rm]
                };
                if sf == 1 {
                    let op2 = !eval_shift64(rm_val, shift, imm6);
                    let res = rn_val & op2;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res;
                    }
                } else {
                    let op2 = !eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    let res = (rn_val as u32) & op2;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res as u64;
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
            // GB-25: ORR (opc=01). Rd = Rn | shift(Rm).
            // 32-bit form (sf=0) zeroes upper 32 bits of Rd.
            // Note: for logical ops, Rn/Rm=31 means XZR (not SP).
            if opc == 0b01 && shift < 3 {
                let rn_val = if rn == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                let rm_val = if rm == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rm]
                };
                if sf == 1 {
                    let op2 = eval_shift64(rm_val, shift, imm6);
                    let res = rn_val | op2;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res;
                    }
                } else {
                    let op2 = eval_shift32(rm_val as u32, shift, imm6 & 0x1F);
                    let res = (rn_val as u32) | op2;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res as u64;
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 5b. Multiply-add/subtract, 32-bit: MADD/MSUB (Wd = Wa +/- Wn*Wm).
        // GB-26: the kernel hits `madd w8, w8, w10, w11` (0x1B0A2D08) at
        // step 1249581, which U2 traps ("DataProc: 32-bit MADD width is
        // not expressible in IrOp::Madd"); 64-bit MADD goes through WASM
        // fine, so only the 32-bit form is claimed here.
        // Encoding: sf=0 00 11011 000 Rm o0 Ra Rn Rd --
        // bits[31:21] == 0b00011011000; bit15: 0 = MADD, 1 = MSUB.
        // 32-bit arithmetic: low 32 bits of product, wrapping add/sub,
        // zero-extended into Rd. Rn/Rm/Ra/Rd=31 name WZR.
        if (word >> 21) & 0x7FF == 0b00011011000 {
            let rm = ((word >> 16) & 0x1F) as usize;
            let is_sub = (word >> 15) & 1 == 1;
            let ra = ((word >> 10) & 0x1F) as usize;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;
            let n = if rn == 31 {
                0
            } else {
                self.machine.cpu[0].regs[rn] as u32
            };
            let m = if rm == 31 {
                0
            } else {
                self.machine.cpu[0].regs[rm] as u32
            };
            let a = if ra == 31 {
                0
            } else {
                self.machine.cpu[0].regs[ra] as u32
            };
            let prod = n.wrapping_mul(m);
            let res = if is_sub {
                a.wrapping_sub(prod)
            } else {
                a.wrapping_add(prod)
            };
            if rd != 31 {
                self.machine.cpu[0].regs[rd] = res as u64;
            }
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
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
                let rn_val = if rn == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                if sf == 1 {
                    let res = rn_val & mask;
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res;
                    }
                    let nzcv = nzcv_and64(res);
                    self.machine.cpu[0].pstate =
                        (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                } else {
                    let res = (rn_val as u32) & (mask as u32);
                    if rd != 31 {
                        self.machine.cpu[0].regs[rd] = res as u64;
                    }
                    let nzcv = nzcv_and32(res);
                    self.machine.cpu[0].pstate =
                        (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | nzcv;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 7. Conditional select: CSEL / CSINC / CSINV / CSNEG (GB-15,
        // encoding corrected GB-26). sf op S 11010100 Rm cond op2 Rn Rd.
        // op=bit30 selects the pair (0: CSEL/CSINC, 1: CSINV/CSNEG),
        // op2=bits[11:10] selects within it (00: select/invert, 01:
        // increment/negate); bits[30:21] == 0xD4 (op=0) or 0x2D4 (op=1),
        // S (bit29) is 0. GB-15 matched only op=0 and read the pair
        // select from op2, so real CSINV/CSNEG words (op=1, e.g. the
        // kernel halt word 0xDA80202A = csinv x10, x1, x0, hs at step
        // 1249127) were never claimed. Executed directly like B.cond:
        // the condition reads the live NZCV flags from pstate, which the
        // WASM path cannot see, so no U2/U3 lifting is involved
        // (condition_holds is the shared GB-2 cond-eval helper).
        // op2 == 0b10/0b11 is unallocated (cond=AL/NV is legal and
        // always-true): fall through to the U2 DataProc trap rather
        // than executing made-up semantics.
        {
            let b30_21 = (word >> 21) & 0x3FF;
            if b30_21 == 0xD4 || b30_21 == 0x2D4 {
                let sf = (word >> 31) & 1;
                let op = (word >> 30) & 1;
                let op2 = (word >> 10) & 0x3;
                let cond = ((word >> 12) & 0xF) as u8;
                if op2 <= 0x1 {
                    let rm = ((word >> 16) & 0x1F) as usize;
                    let rn = ((word >> 5) & 0x1F) as usize;
                    let rd = (word & 0x1F) as usize;
                    let cpu = &mut self.machine.cpu[0];
                    let m_val = if rm == 31 { 0 } else { cpu.regs[rm] };
                    let n_val = if rn == 31 { 0 } else { cpu.regs[rn] };
                    let taken = condition_holds(cond, cpu.pstate);
                    let val = if sf == 1 {
                        if taken {
                            n_val
                        } else {
                            match (op, op2) {
                                (0, 0) => m_val,
                                (0, _) => m_val.wrapping_add(1),
                                (_, 0) => !m_val,
                                _ => m_val.wrapping_neg(),
                            }
                        }
                    } else {
                        // 32-bit form: operate on the low 32 bits, then
                        // zero-extend into the 64-bit slot.
                        let m32 = m_val as u32;
                        let n32 = n_val as u32;
                        let w = if taken {
                            n32
                        } else {
                            match (op, op2) {
                                (0, 0) => m32,
                                (0, _) => m32.wrapping_add(1),
                                (_, 0) => !m32,
                                _ => m32.wrapping_neg(),
                            }
                        };
                        w as u64
                    };
                    if rd != 31 {
                        cpu.regs[rd] = val;
                    }
                    cpu.pc = pc.wrapping_add(4);
                    return Some(Ok(()));
                }
            }
        }

        // 6b. PRFM (immediate): prefetch-memory hint, architecturally NOP.        // GB-26: the kernel hits `prfm pstl1strm, [x0]` (0xF9800011) at
        // step 1250435. U1 has no PRFM kind, so it falls through to
        // IllegalInstruction. Prefetch is a pure hint with no
        // architectural state change on single-vCPU: retire as NOP.
        // Encoding: 11 111001 10 imm12 Rn Rt -- bits[31:22] ==
        // 0b1111100110 (Rt names the prefetch op, not a register).
        if (word >> 22) & 0x3FF == 0b1111100110 {
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 6c. Load/store exclusive: LDXR/LDAXR and STXR/STLXR.
        // GB-26: the kernel hits `ldxr w16, [x0]` (0x885F7C10) at step
        // 1250436 and `stlxr w17, w2, [x0]` (0x8811FC02) at step 1250439
        // -- the first atomics. U1 has no exclusive kind, so they fall
        // through to IllegalInstruction.
        // Encoding: size 001000 o2 L o1 Rs o0 11111 Rn Rt --
        // bits[29:24] == 0b001000 pins the class; o2=bit23=0,
        // bits[14:10] == 0b11111 select the non-pair form. L=bit22:
        // 1=load, 0=store. o0=bit15 is acquire/release (LDAXR/STLXR):
        // a NOP for single-vCPU ordering. size: 00=B, 01=H, 10=W, 11=X.
        // LDXR: Rt = zero-extended [Rn]; monitor = (addr, size).
        // STXR: Rs=Ws status; if the monitor still holds (addr, size)
        // the store commits and Ws=0, else Ws=1 with no store; the
        // monitor is cleared either way.
        // Known simplification: a plain store to the monitored address
        // between LDXR and STXR does not clear the monitor here (ARM
        // requires it). Single-vCPU boot atomics never do this; the
        // monitor lives in the orchestrator, not the contract state.
        if (word >> 24) & 0x3F == 0b001000
            && (word >> 23) & 1 == 0
            && (word >> 10) & 0x1F == 0b11111
        {
            let size = (word >> 30) & 0x3;
            let is_load = (word >> 22) & 1 == 1;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rt = (word & 0x1F) as usize;
            let nbytes: u64 = 1 << size;
            let va = if rn == 31 {
                self.machine.cpu[0].sp
            } else {
                self.machine.cpu[0].regs[rn]
            };
            if is_load {
                // Rs must be 11111 for loads; otherwise fall through.
                if (word >> 16) & 0x1F != 0b11111 {
                    return None;
                }
                let pa = match self.translate_data_orch(va, Access::Read) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, nbytes) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "load-exclusive outside RAM".to_string(),
                        }))
                    }
                };
                let val = match size {
                    0 => self.machine.ram[off] as u64,
                    1 => u16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                        as u64,
                    2 => u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())
                        as u64,
                    _ => u64::from_le_bytes(self.machine.ram[off..off + 8].try_into().unwrap()),
                };
                if rt != 31 {
                    self.machine.cpu[0].regs[rt] = val;
                }
                self.exclusive = Some((va, nbytes as u8));
            } else {
                let rs = ((word >> 16) & 0x1F) as usize; // Ws status
                let pa = match self.translate_data_orch(va, Access::Write) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, nbytes) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "store-exclusive outside RAM".to_string(),
                        }))
                    }
                };
                let ok = self.exclusive == Some((va, nbytes as u8));
                if ok {
                    let data = if rt == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rt]
                    };
                    match size {
                        0 => self.machine.ram[off] = data as u8,
                        1 => self.machine.ram[off..off + 2]
                            .copy_from_slice(&(data as u16).to_le_bytes()),
                        2 => self.machine.ram[off..off + 4]
                            .copy_from_slice(&(data as u32).to_le_bytes()),
                        _ => self.machine.ram[off..off + 8].copy_from_slice(&data.to_le_bytes()),
                    }
                }
                self.exclusive = None;
                if rs != 31 {
                    self.machine.cpu[0].regs[rs] = if ok { 0 } else { 1 };
                }
            }
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 6c2. Store-release / load-acquire: STLR/LDLR.
        // GB-26: the kernel hits `stlr x19, [x8]` (0xC89FFD13) at step
        // 1250920. These are single-copy atomic, NOT exclusives: no
        // monitor interaction. Release/acquire ordering is a NOP for
        // single-vCPU.
        // Encoding: size 001000 o2=1 L o1=0 Rs=11111 o0=1 11111 Rn Rt --
        // bits[29:24] == 0b001000 (same class as exclusives), o2=bit23=1
        // selects STLR/LDLR over STXR/LDXR. L=bit22: 1=LDLR, 0=STLR.
        // size: 00=B, 01=H, 10=W, 11=X. Rn=31 names SP.
        if (word >> 24) & 0x3F == 0b001000
            && (word >> 23) & 1 == 1
            && (word >> 21) & 1 == 0
            && (word >> 16) & 0x1F == 0b11111
            && (word >> 15) & 1 == 1
            && (word >> 10) & 0x1F == 0b11111
        {
            let size = (word >> 30) & 0x3;
            let is_load = (word >> 22) & 1 == 1;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rt = (word & 0x1F) as usize;
            let nbytes: u64 = 1 << size;
            let va = if rn == 31 {
                self.machine.cpu[0].sp
            } else {
                self.machine.cpu[0].regs[rn]
            };
            let access = if is_load { Access::Read } else { Access::Write };
            let pa = match self.translate_data_orch(va, access) {
                Ok(pa) => pa,
                Err(reason) => return Some(Err(reason)),
            };
            let off = match self.ram_offset(pa, nbytes) {
                Ok(off) => off,
                Err(_) => {
                    return Some(Err(HaltReason::WasmTrap {
                        addr: va,
                        message: "stlr/ldlr outside RAM".to_string(),
                    }))
                }
            };
            if is_load {
                let val = match size {
                    0 => self.machine.ram[off] as u64,
                    1 => u16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                        as u64,
                    2 => u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())
                        as u64,
                    _ => u64::from_le_bytes(self.machine.ram[off..off + 8].try_into().unwrap()),
                };
                if rt != 31 {
                    self.machine.cpu[0].regs[rt] = val;
                }
            } else {
                let val = if rt == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rt]
                };
                match size {
                    0 => self.machine.ram[off] = val as u8,
                    1 => {
                        self.machine.ram[off..off + 2].copy_from_slice(&(val as u16).to_le_bytes())
                    }
                    2 => {
                        self.machine.ram[off..off + 4].copy_from_slice(&(val as u32).to_le_bytes())
                    }
                    _ => self.machine.ram[off..off + 8].copy_from_slice(&val.to_le_bytes()),
                }
            }
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 6d. Conditional compare (immediate): CCMP/CCMN.
        // GB-26: the kernel hits `ccmp x4, #0, #4, ne` (0xFA401884) at
        // step 1250639. U1 has no conditional-compare kind.
        // Encoding (ARM DDI 0487): sf op 1 11010 0 1 0 imm5 cond 1 0
        // Rn 0 nzcv -- bits[29:24] == 0b111010, op=bit30 (0=CCMN/ADD,
        // 1=CCMP/SUB), bits[23:21] == 0b010, bit11=1, bit10=0, bit4=0.
        // imm5 = bits[20:16], cond = bits[15:12], Rn = bits[9:5],
        // nzcv = bits[3:0]. If cond holds, NZCV = Rn +/- imm5 (discarded);
        // else NZCV = nzcv. Rn=31 names XZR (not SP).
        // (Fixed: old mask `(word>>24)&0x7F==0x7A` forced op=1, so CCMN
        // was dead code despite the comment claiming both.)
        if (word >> 24) & 0x3F == 0x3A
            && (word >> 21) & 0x7 == 0b010
            && (word >> 10) & 0x3 == 0b10
            && (word >> 4) & 1 == 0
        {
            let sf = (word >> 31) & 1;
            let op = (word >> 30) & 1; // 0 = CCMN (ADD), 1 = CCMP (SUB)
            let imm5 = (word >> 16) & 0x1F;
            let cond = ((word >> 12) & 0xF) as u8;
            let rn = ((word >> 5) & 0x1F) as usize;
            let nzcv_imm = (word & 0xF) as u64;
            let new_nzcv = if condition_holds(cond, self.machine.cpu[0].pstate) {
                // Rn=31 names XZR (not SP) for conditional compare.
                let rn_val = if rn == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                if sf == 1 {
                    if op == 1 {
                        nzcv_sub64(rn_val, imm5 as u64)
                    } else {
                        nzcv_add64(rn_val, imm5 as u64)
                    }
                } else {
                    let a32 = rn_val as u32;
                    let b32 = imm5 as u32;
                    if op == 1 {
                        nzcv_sub32(a32, b32)
                    } else {
                        nzcv_add32(a32, b32)
                    }
                }
            } else {
                // nzcv immediate: bit3=N, bit2=Z, bit1=C, bit0=V.
                (nzcv_imm & 0xF) << 28
            };
            self.machine.cpu[0].pstate = (self.machine.cpu[0].pstate & !FLAGS_NZCV_MASK) | new_nzcv;
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 6e. Byte-reverse: REV/REV32/REV16 (data-processing, 1 source).
        // GB-26: the kernel hits `rev x6, x6` (0xDAC00CC6) at step 1250673.
        // U1 only recognizes CLZ in this class; REV* stay Illegal there.
        // Encoding: sf 1 0 11010 110 00000 opcode Rn Rd (S=bit29=0).
        // bits[30:24] == 0b1011010. opcode: 000001=REV16,
        // 000010=REV(32-bit)/REV32(64-bit), 000011=REV(64-bit).
        // Rd=31 discards (data-processing Rd is XZR, not SP).
        if (word >> 24) & 0x7F == 0x5A && (word >> 21) & 0x7 == 0b110 && (word >> 16) & 0x1F == 0 {
            let sf = (word >> 31) & 1;
            let opcode = (word >> 10) & 0x3F;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;
            let rn_val = if rn == 31 {
                0
            } else {
                self.machine.cpu[0].regs[rn]
            };
            let result = match (sf, opcode) {
                // REV 64-bit: reverse all 8 bytes.
                (1, 0b000011) => rn_val.swap_bytes(),
                // REV32 64-bit: reverse bytes in each 32-bit word.
                (1, 0b000010) => {
                    let lo = (rn_val as u32).swap_bytes() as u64;
                    let hi = ((rn_val >> 32) as u32).swap_bytes() as u64;
                    lo | (hi << 32)
                }
                // REV 32-bit: reverse 4 bytes, zero-extend.
                (0, 0b000010) => (rn_val as u32).swap_bytes() as u64,
                // REV16 64-bit: reverse bytes in each 16-bit halfword.
                (1, 0b000001) => {
                    let mut r = 0u64;
                    for i in 0..4 {
                        let h = ((rn_val >> (i * 16)) & 0xFFFF) as u64;
                        r |= h.swap_bytes() << (i * 16);
                    }
                    r
                }
                // REV16 32-bit: reverse bytes in each 16-bit halfword.
                (0, 0b000001) => {
                    let v = rn_val as u32;
                    let lo = (v & 0xFFFF).swap_bytes() as u64;
                    let hi = ((v >> 16) & 0xFFFF).swap_bytes() as u64;
                    lo | (hi << 16)
                }
                _ => return None, // RBIT/CLZ/CLS or S=1: not handled here.
            };
            if rd != 31 {
                self.machine.cpu[0].regs[rd] = result;
            }
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 6f. EXTR (extract), including the ROR (immediate) alias.
        // GB-26: the kernel hits `ror x8, x8, #2` (0x93C80908) at step
        // 1250891. Encoding: sf 0 0 100111 N Rm lsb Rn Rd.
        // bits[28:23] == 0b100111, op=bit30=0, S=bit29=0, N=bit22 must
        // equal sf. Rm = bits[21:16], lsb = bits[15:10], Rn = bits[9:5],
        // Rd = bits[4:0]. Rd = (Rm:Rn)[lsb+datasize-1 : lsb].
        // Rn/Rm=31 name XZR (not SP) for this class.
        if (word >> 23) & 0x3F == 0b100111
            && (word >> 30) & 1 == 0
            && (word >> 29) & 1 == 0
            && ((word >> 22) & 1) == ((word >> 31) & 1)
        {
            let sf = (word >> 31) & 1;
            let rm = ((word >> 16) & 0x1F) as usize;
            let lsb = (word >> 10) & 0x3F;
            let rn = ((word >> 5) & 0x1F) as usize;
            let rd = (word & 0x1F) as usize;
            let n_val = if rn == 31 {
                0
            } else {
                self.machine.cpu[0].regs[rn]
            };
            let m_val = if rm == 31 {
                0
            } else {
                self.machine.cpu[0].regs[rm]
            };
            let result = if sf == 1 {
                if lsb >= 64 {
                    return None; // UNDEFINED: lsb >= datasize.
                }
                let concat = ((m_val as u128) << 64) | (n_val as u128);
                ((concat >> lsb) & 0xFFFF_FFFF_FFFF_FFFF) as u64
            } else {
                if lsb >= 32 {
                    return None; // UNDEFINED: lsb >= datasize.
                }
                let n32 = n_val as u32 as u64;
                let m32 = m_val as u32 as u64;
                let concat = (m32 << 32) | n32;
                ((concat >> lsb) & 0xFFFF_FFFF) as u64
            };
            if rd != 31 {
                self.machine.cpu[0].regs[rd] = result;
            }
            self.machine.cpu[0].pc = pc.wrapping_add(4);
            return Some(Ok(()));
        }

        // 7. SP-relative load/store (unsigned immediate and pair).
        // GB-23: the Wave-4 IR has no SP, so U2 traps these. The fast path
        // handles the common forms directly: Rn=31 reads SP.
        // STR/LDR (unsigned imm): size(11=64-bit/10=32-bit) 111 V=0 01
        //   opc imm12 Rn Rt; opc 00 = STR, 01 = LDR. (GB-26: the old mask
        //   `(op10 & 0x3FC) == 0x3E0` matched bit24=0, which is STTR/LDTR,
        //   not STR/LDR -- the GB-23 test words were misassembled and the
        //   kernel's real `str x8, [sp, #8]` (0xF90007E8, bit24=1) missed
        //   the fast path entirely.)
        let op10 = (word >> 22) & 0x3FF;
        let rn = ((word >> 5) & 0x1F) as usize;
        // GB-26: extend to non-SP Rn for sub-word (B/H) sizes. U2 traps
        // B/H ("not expressible in IrOp"); the kernel hits `ldrh w11,
        // [x9, #0x2e2]` (0x7945C52B) at step 1250850. W/X with non-SP Rn
        // still go through U1/U2 (they handle those).
        let size_bits = (word >> 30) & 0x3;
        if rn == 31 || (((op10 >> 2) & 0x3F) == 0b111001 && size_bits <= 1) {
            // STR/LDR unsigned immediate, all sizes. GB-26: the kernel
            // hits `strb w8, [sp]` (0x390003E8) at step 1249745 -- the
            // old mask only covered 32/64-bit (op10 0x2E4/0x3E4).
            // bits[29:24] == 0b111001 pins the unsigned-immediate class
            // (op10 bits[7:2]); size = bits[31:30] (00=B, 01=H, 10=W,
            // 11=X), opc = bits[23:22] (00=STR, 01=LDR, 10=LDRS->W,
            // 11=LDRS->X).
            if ((op10 >> 2) & 0x3F) == 0b111001 {
                let size = size_bits;
                let opc = (word >> 22) & 0x3;
                let imm12 = ((word >> 10) & 0xFFF) as u64;
                let rt = (word & 0x1F) as usize;
                let nbytes: u64 = 1 << size;
                let base = if rn == 31 {
                    self.machine.cpu[0].sp
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                let va = base.wrapping_add(imm12 * nbytes);
                let is_store = opc == 0b00;
                let access = if is_store {
                    Access::Write
                } else {
                    Access::Read
                };
                let pa = match self.translate_data_orch(va, access) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, nbytes) {
                    Ok(off) => off,
                    Err(_) => {
                        // GB-26: for non-SP B/H, an address outside RAM
                        // (e.g. console MMIO) falls through to U1/U2, which
                        // know about MMIO. SP-relative keeps the old trap.
                        if rn != 31 {
                            return None;
                        }
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "sp-relative access outside RAM".to_string(),
                        }));
                    }
                };
                if is_store {
                    let val = if rt == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rt]
                    };
                    match size {
                        0 => self.machine.ram[off] = val as u8,
                        1 => self.machine.ram[off..off + 2]
                            .copy_from_slice(&(val as u16).to_le_bytes()),
                        2 => self.machine.ram[off..off + 4]
                            .copy_from_slice(&(val as u32).to_le_bytes()),
                        _ => self.machine.ram[off..off + 8].copy_from_slice(&val.to_le_bytes()),
                    }
                } else {
                    let val = match (size, opc) {
                        (0, 0b01) => self.machine.ram[off] as u64, // LDRB
                        (0, 0b10) => ((self.machine.ram[off] as i8) as u32) as u64, // LDRSB W
                        (0, _) => (self.machine.ram[off] as i8) as i64 as u64, // LDRSB X
                        (1, 0b01) => {
                            u16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as u64
                        } // LDRH
                        (1, 0b10) => {
                            ((i16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as i32) as u32) as u64
                        } // LDRSH W
                        (1, _) => {
                            (i16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as i64) as u64
                        } // LDRSH X
                        (2, 0b10) => {
                            (i32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())
                                as i64) as u64
                        } // LDRSW
                        (2, _) => {
                            u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())
                                as u64
                        } // LDR W
                        _ => u64::from_le_bytes(self.machine.ram[off..off + 8].try_into().unwrap()), // LDR X
                    };
                    if rt != 31 {
                        self.machine.cpu[0].regs[rt] = val;
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
            // LDUR/STUR and pre/post-index LDR/STR, SP-relative (single).
            // GB-26: the kernel hits `str x30, [sp, #-0x10]!` (0xF81F0FFE)
            // at step 1252125. U2 traps SP-relative ("no SP in Wave-4").
            // Encoding: size 111 V=0 00 opc imm9 idx Rn Rt.
            // idx = bits[11:10]: 00=unscaled, 01=post-index, 11=pre-index.
            // 10=unprivileged (LDTR/STTR): NOT handled here (privilege
            // semantics differ; falls through). imm9 is a signed BYTE
            // offset (not scaled). Rn=31 names SP (outer if guarantees).
            let is_sp_unscaled_idx = rn == 31
                && (word >> 27) & 0x7 == 0b111
                && (word >> 26) & 1 == 0
                && (word >> 24) & 0x3 == 0b00;
            if is_sp_unscaled_idx {
                let size = (word >> 30) & 0x3;
                let opc = (word >> 22) & 0x3;
                let imm9 = (((word >> 12) & 0x1FF) as i64) << 55 >> 55;
                let idx = (word >> 10) & 0x3;
                // Skip unprivileged (LDTR/STTR): wrong privilege semantics.
                if idx == 0b10 {
                    // Fall through to U1/U2.
                } else {
                    let rt = (word & 0x1F) as usize;
                    let nbytes: u64 = 1 << size;
                    let sp_val = self.machine.cpu[0].sp;
                    let addr = match idx {
                        0b11 => sp_val.wrapping_add(imm9 as u64), // pre-index
                        0b01 => sp_val,                           // post-index
                        _ => sp_val.wrapping_add(imm9 as u64),    // unscaled
                    };
                    let is_store = opc == 0b00;
                    let access = if is_store {
                        Access::Write
                    } else {
                        Access::Read
                    };
                    let pa = match self.translate_data_orch(addr, access) {
                        Ok(pa) => pa,
                        Err(reason) => return Some(Err(reason)),
                    };
                    let off = match self.ram_offset(pa, nbytes) {
                        Ok(off) => off,
                        Err(_) => {
                            return Some(Err(HaltReason::WasmTrap {
                                addr,
                                message: "sp-relative access outside RAM".to_string(),
                            }))
                        }
                    };
                    if is_store {
                        let val = if rt == 31 {
                            0
                        } else {
                            self.machine.cpu[0].regs[rt]
                        };
                        match size {
                            0 => self.machine.ram[off] = val as u8,
                            1 => self.machine.ram[off..off + 2]
                                .copy_from_slice(&(val as u16).to_le_bytes()),
                            2 => self.machine.ram[off..off + 4]
                                .copy_from_slice(&(val as u32).to_le_bytes()),
                            _ => self.machine.ram[off..off + 8].copy_from_slice(&val.to_le_bytes()),
                        }
                    } else {
                        let val = match (size, opc) {
                            (0, 0b01) => self.machine.ram[off] as u64,
                            (0, 0b10) => ((self.machine.ram[off] as i8) as u32) as u64,
                            (0, _) => (self.machine.ram[off] as i8) as i64 as u64,
                            (1, 0b01) => u16::from_le_bytes(
                                self.machine.ram[off..off + 2].try_into().unwrap(),
                            ) as u64,
                            (1, 0b10) => {
                                ((i16::from_le_bytes(
                                    self.machine.ram[off..off + 2].try_into().unwrap(),
                                ) as i32) as u32) as u64
                            }
                            (1, _) => {
                                (i16::from_le_bytes(
                                    self.machine.ram[off..off + 2].try_into().unwrap(),
                                ) as i64) as u64
                            }
                            (2, 0b10) => {
                                (i32::from_le_bytes(
                                    self.machine.ram[off..off + 4].try_into().unwrap(),
                                ) as i64) as u64
                            } // LDURSW
                            (2, _) => u32::from_le_bytes(
                                self.machine.ram[off..off + 4].try_into().unwrap(),
                            ) as u64,
                            _ => u64::from_le_bytes(
                                self.machine.ram[off..off + 8].try_into().unwrap(),
                            ),
                        };
                        if rt != 31 {
                            self.machine.cpu[0].regs[rt] = val;
                        }
                    }
                    // Writeback for pre-index and post-index.
                    if idx == 0b11 {
                        self.machine.cpu[0].sp = addr;
                    } else if idx == 0b01 {
                        self.machine.cpu[0].sp = sp_val.wrapping_add(imm9 as u64);
                    }
                    self.machine.cpu[0].pc = pc.wrapping_add(4);
                    return Some(Ok(()));
                }
            }
            // STP/LDP (pair), all indexing modes. GB-26 rewrote this arm:
            // the old op10 match (0x2A6/0x2A5/0x0A6/0x0A5) covered only a
            // mix of pre-index stores and signed-offset loads, missed e.g.
            // STP signed-offset (op10 0x2A4 -- the GB-26 kernel halt word
            // 0xa9017bfd), and silently dropped pre-index writeback.
            //   opc = bits[31:30]: 00 = 32-bit pair, 10 = 64-bit pair
            //     (01 = LDPSW: not handled here, falls through to U2)
            //   bits[29:25] = 0b10100 pins the pair class; bit26 V = 0
            //     selects integer registers (this arm)
            //   idx = bits[24:23]: 00/10 = signed offset (00 is STNP/LDNP;
            //     same addressing, and the non-temporal hint is
            //     unobservable on single-vCPU), 01 = post-index,
            //     11 = pre-index
            //   bit22 L: 0 = store (STP), 1 = load (LDP)
            let opc = (word >> 30) & 0x3;
            let is_sp_pair = (word >> 25) & 0x1F == 0b10100
                && (word >> 26) & 1 == 0
                && (opc == 0b00 || opc == 0b10);
            if is_sp_pair {
                let is_store = (word >> 22) & 1 == 0;
                let is64 = opc == 0b10;
                let scale: i64 = if is64 { 8 } else { 4 };
                let idx_mode = (word >> 23) & 0x3;
                let imm7 = ((word >> 15) & 0x7F) as i64;
                let imm7 = ((imm7 << 57) >> 57) * scale; // sign-extend, scale
                let rt2 = ((word >> 10) & 0x1F) as usize;
                let rt1 = (word & 0x1F) as usize;
                // Effective address per indexing mode. Post-index addresses
                // at SP; pre-index and signed-offset address at SP + imm.
                let sp = self.machine.cpu[0].sp as i64;
                let va = match idx_mode {
                    0b01 => sp as u64,
                    _ => sp.wrapping_add(imm7) as u64,
                };
                let access = if is_store {
                    Access::Write
                } else {
                    Access::Read
                };
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
                    let v1 = if rt1 == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rt1]
                    };
                    let v2 = if rt2 == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rt2]
                    };
                    if is64 {
                        self.machine.ram[off..off + 8].copy_from_slice(&v1.to_le_bytes());
                        self.machine.ram[off2..off2 + 8].copy_from_slice(&v2.to_le_bytes());
                    } else {
                        self.machine.ram[off..off + 4].copy_from_slice(&(v1 as u32).to_le_bytes());
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
                    let v1 = u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())
                        as u64;
                    let v2 =
                        u32::from_le_bytes(self.machine.ram[off2..off2 + 4].try_into().unwrap())
                            as u64;
                    if rt1 != 31 {
                        self.machine.cpu[0].regs[rt1] = v1;
                    }
                    if rt2 != 31 {
                        self.machine.cpu[0].regs[rt2] = v2;
                    }
                }
                // Writeback for pre-index and post-index (GB-26: the old arm
                // silently dropped pre-index writeback, so SP never moved).
                if idx_mode == 0b01 || idx_mode == 0b11 {
                    self.machine.cpu[0].sp = sp.wrapping_add(imm7) as u64;
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 8. Sub-word load/store, register offset: LDRB/LDRH/STRB/STRH
        // (and signed LDRSB/LDRSH). GB-26: the kernel hits
        // `ldrh w10, [x11, x8, lsl #1]` (0x7868796A) at step 1249461,
        // which U2 traps ("LoadStore: sub-word access width is not
        // expressible in IrOp"). The Wave-4 IR has no sub-word
        // accesses, so the fast path handles sizes 00 (byte) and 01
        // (halfword); 32/64-bit fall through to U1/U2/WASM as before.
        // Encoding: size 111 V=0 00 opc 1 Rm option S 10 Rn Rt --
        // bits[29:24] == 0b111000 pins the integer load/store class,
        // bit21 == 1 selects the register-offset form (0 = unscaled
        // immediate, handled elsewhere), and bits[11:10] == 0b10 pins
        // this form (atomics share the 111000 prefix but differ there;
        // e.g. 0x38236041 = ldumaxb has bits[11:10] == 0b00).
        if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 && (word >> 10) & 0x3 == 0b10 {
            let size = (word >> 30) & 0x3;
            if size <= 1 {
                let opc = (word >> 22) & 0x3;
                // opc: 00 = STRB/STRH, 01 = LDRB/LDRH (zero-extend),
                // 10 = LDRSB/LDRSH to W (sign-extend 32),
                // 11 = LDRSB/LDRSH to X (sign-extend 64).
                let rm = ((word >> 16) & 0x1F) as usize;
                let option = (word >> 13) & 0x7;
                let s = (word >> 12) & 1;
                let rn = ((word >> 5) & 0x1F) as usize;
                let rt = (word & 0x1F) as usize;
                let rm_val = if rm == 31 {
                    0
                } else {
                    self.machine.cpu[0].regs[rm]
                };
                // Extend the offset register: option 011/111 names the
                // 64-bit X[m]; the rest name W[m] with the extend applied.
                let offset = if option == 0b011 || option == 0b111 {
                    rm_val
                } else {
                    let w = rm_val as u32;
                    match option {
                        0b000 => (w as u8) as u64,         // UXTB
                        0b001 => (w as u16) as u64,        // UXTH
                        0b010 => w as u64,                 // UXTW
                        0b100 => (w as i8) as i64 as u64,  // SXTB
                        0b101 => (w as i16) as i64 as u64, // SXTH
                        _ => (w as i32) as i64 as u64,     // SXTW (0b110)
                    }
                };
                // S=1 shifts left by the access size in bytes (option
                // 011/111 with S=1 is the LSL alias; S=0 is UXTX/SXTX,
                // i.e. no shift).
                let offset = if s == 1 { offset << size } else { offset };
                let base = if rn == 31 {
                    self.machine.cpu[0].sp
                } else {
                    self.machine.cpu[0].regs[rn]
                };
                let va = base.wrapping_add(offset);
                let nbytes: u64 = if size == 0 { 1 } else { 2 };
                let is_store = opc == 0b00;
                let access = if is_store {
                    Access::Write
                } else {
                    Access::Read
                };
                let pa = match self.translate_data_orch(va, access) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, nbytes) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "sub-word access outside RAM".to_string(),
                        }))
                    }
                };
                if is_store {
                    let v = if rt == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rt]
                    };
                    if size == 0 {
                        self.machine.ram[off] = v as u8;
                    } else {
                        self.machine.ram[off..off + 2].copy_from_slice(&(v as u16).to_le_bytes());
                    }
                } else {
                    let val = match (size, opc) {
                        (0, 0b01) => self.machine.ram[off] as u64, // LDRB
                        (0, 0b10) => ((self.machine.ram[off] as i8) as u32) as u64, // LDRSB W
                        (0, _) => (self.machine.ram[off] as i8) as i64 as u64, // LDRSB X
                        (1, 0b01) => {
                            u16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as u64 // LDRH
                        }
                        (1, 0b10) => {
                            ((i16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as i32) as u32) as u64 // LDRSH W
                        }
                        _ => {
                            (i16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as i64) as u64 // LDRSH X
                        }
                    };
                    if rt != 31 {
                        self.machine.cpu[0].regs[rt] = val;
                    }
                }
                self.machine.cpu[0].pc = pc.wrapping_add(4);
                return Some(Ok(()));
            }
        }

        // 9. Sub-word load/store, immediate: LDRB/LDRH/STRB/STRH (and
        // signed LDRSB/LDRSH) with unscaled, post-index, or pre-index
        // addressing. GB-26: the kernel hits `ldrh w3, [x1], #2`
        // (0x78402423, post-index) at step 1249482, which U2 traps for
        // the same Wave-4 IR reason as the register form above.
        // Encoding: size 111 V=0 00 opc 0 imm9 idx Rn Rt --
        // bits[29:24] == 0b111000, bit21 == 0 selects the immediate
        // class (1 = register offset, arm 8 above); idx = bits[11:10]:
        // 00 = unscaled (LDUR/STUR), 01 = post-index, 10 =
        // unprivileged (LDTR/STTR: same addressing as unscaled on this
        // single-vCPU EL1 emulator), 11 = pre-index.
        if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 0 {
            let size = (word >> 30) & 0x3;
            if size <= 1 {
                let opc = (word >> 22) & 0x3;
                let imm9 = ((word >> 12) & 0x1FF) as i64;
                let imm9 = (imm9 << 55) >> 55; // sign-extend
                let idx = (word >> 10) & 0x3;
                let rn = ((word >> 5) & 0x1F) as usize;
                let rt = (word & 0x1F) as usize;
                // Post-index addresses at Rn; the rest address at Rn+imm.
                let base = if rn == 31 {
                    self.machine.cpu[0].sp as i64
                } else {
                    self.machine.cpu[0].regs[rn] as i64
                };
                let va = if idx == 0b01 {
                    base as u64
                } else {
                    base.wrapping_add(imm9) as u64
                };
                let nbytes: u64 = if size == 0 { 1 } else { 2 };
                let is_store = opc == 0b00;
                let access = if is_store {
                    Access::Write
                } else {
                    Access::Read
                };
                let pa = match self.translate_data_orch(va, access) {
                    Ok(pa) => pa,
                    Err(reason) => return Some(Err(reason)),
                };
                let off = match self.ram_offset(pa, nbytes) {
                    Ok(off) => off,
                    Err(_) => {
                        return Some(Err(HaltReason::WasmTrap {
                            addr: va,
                            message: "sub-word access outside RAM".to_string(),
                        }))
                    }
                };
                if is_store {
                    let v = if rt == 31 {
                        0
                    } else {
                        self.machine.cpu[0].regs[rt]
                    };
                    if size == 0 {
                        self.machine.ram[off] = v as u8;
                    } else {
                        self.machine.ram[off..off + 2].copy_from_slice(&(v as u16).to_le_bytes());
                    }
                } else {
                    let val = match (size, opc) {
                        (0, 0b01) => self.machine.ram[off] as u64, // LDRB
                        (0, 0b10) => ((self.machine.ram[off] as i8) as u32) as u64, // LDRSB W
                        (0, _) => (self.machine.ram[off] as i8) as i64 as u64, // LDRSB X
                        (1, 0b01) => {
                            u16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as u64 // LDRH
                        }
                        (1, 0b10) => {
                            ((i16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as i32) as u32) as u64 // LDRSH W
                        }
                        _ => {
                            (i16::from_le_bytes(self.machine.ram[off..off + 2].try_into().unwrap())
                                as i64) as u64 // LDRSH X
                        }
                    };
                    if rt != 31 {
                        self.machine.cpu[0].regs[rt] = val;
                    }
                }
                // Writeback for pre-index and post-index.
                if idx == 0b01 || idx == 0b11 {
                    let new_base = base.wrapping_add(imm9) as u64;
                    if rn == 31 {
                        self.machine.cpu[0].sp = new_base;
                    } else {
                        self.machine.cpu[0].regs[rn] = new_base;
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

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(ref mut w) = self.trace_writer {
                use std::io::Write;
                let cpu = &self.machine.cpu[0];
                let nzcv = (cpu.pstate >> 28) & 0xF;
                let _ = write!(w, "{} 0x{:016x} 0x{:08x}", self.steps, pc, word);
                for r in &cpu.regs {
                    let _ = write!(w, " 0x{:016x}", r);
                }
                let _ = writeln!(w, " 0x{:016x} 0x{:x}", cpu.sp, nzcv);
            }
        }

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
                        if self.survey_mode
                            && matches!(
                                reason,
                                HaltReason::Unsupported { .. }
                                    | HaltReason::IllegalInstruction { .. }
                            )
                        {
                            return self.survey_skip_instruction(pc, word, None);
                        }
                        self.halted = Some(reason.clone());
                        return StepOutcome::Halted(reason);
                    }
                }
            }
        }

        // Decode (U1).
        let kind = match u1_decode::decode(word) {
            DecodeResult::Illegal { word } => {
                if self.survey_mode {
                    return self.survey_skip_instruction(pc, word, None);
                }
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
            if self.survey_mode {
                return self.survey_skip_instruction(pc, word, Some(kind));
            }
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
        // Sync guest timer programming into irq state before ticking:
        // ENABLE bit from CNTP_CTL_EL0, compare from CNTP_CVAL_EL0.
        {
            let sysregs = &self.machine.cpu[0].sysregs;
            let irq = &mut self.machine.irq;
            if (sysregs.cntp_ctl_el0 & 0x1) != 0 {
                irq.enabled |= 1u32 << u5_gic_timer::TIMER_ENABLE_BIT;
            } else {
                irq.enabled &= !(1u32 << u5_gic_timer::TIMER_ENABLE_BIT);
            }
            irq.timer_compare = sysregs.cntp_cval_el0;
        }
        let (next_irq, _irqs) = u5_gic_timer::tick(&self.machine.irq, cycles);
        self.machine.irq = next_irq;
        // Sync the free-running counter back so MRS CNTPCT_EL0 sees it.
        self.machine.cpu[0].sysregs.cntpct_el0 = self.machine.irq.timer_count;
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

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Orchestrator {
    fn drop(&mut self) {
        self.flush_trace();
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
            // PL011 drivers use 32-bit readl/writel; the PLATFORM.md
            // byte-console used STRB/LDRB. Accept 1/2/4-byte accesses so a
            // real UART driver can probe this window without halting.
            // Added 2026-10-02 (fam/uart-sandbox).
            if !matches!(size, 1 | 2 | 4) {
                return Err("mem_load: console MMIO size must be 1, 2 or 4".to_string());
            }
            if pa == CONSOLE_RX {
                // RX is byte-oriented; wider reads return the byte in the
                // low bits, upper bits zero.
                return Ok(self.console.read_rx() as i64);
            }
            if pa == CONSOLE_PL011_FR {
                // Flag register: TX never full, RX always empty — a
                // polling PL011 driver proceeds to write DR immediately.
                return Ok(CONSOLE_PL011_FR_VALUE as i64);
            }
            // Defined MMIO region, other offsets: reads return 0.
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
            // Accept 1/2/4-byte accesses: a real PL011 driver writes DR
            // with 32-bit STR. The low byte is the transmitted character.
            // Added 2026-10-02 (fam/uart-sandbox).
            if !matches!(size, 1 | 2 | 4) {
                return Err("mem_store: console MMIO size must be 1, 2 or 4".to_string());
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
        let sel = SysRegs::from_index(reg).ok_or_else(|| format!("sysreg_load: bad index"))?;
        Ok(self.sysregs.load(sel) as i64)
    }

    fn sysreg_store(&mut self, reg: u8, val: i64) -> Result<(), String> {
        let sel = SysRegs::from_index(reg).ok_or_else(|| format!("sysreg_store: bad index"))?;
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
        "System: BRK exception",
        "System: HLT exception",
        "System: HVC exception",
        "System: SMC exception",
        "System: ERET exception return not yet implemented",
        "LoadStore: atomic CAS requires memory arbitration (unsupported in IR)",
        "LoadStore: exclusive monitor requires orchestrator state (unsupported in IR)",
        "LoadStore: atomic LSE operation (unsupported in IR)",
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

    /// 2026-10-02 (fam/uart-sandbox): the WASM host path (`mem_store` /
    /// `mem_load`) accepts 1/2/4-byte console accesses so a real PL011
    /// driver (32-bit readl/writel) can use the UART window. TX captures
    /// the low byte; FR reads report TXFE|RXFE.
    #[test]
    fn uart_tx_captures_low_byte_of_word_store() {
        let mut p = host_parts(SysRegs::default()); // MMU off: identity
        let r = with_host(&mut p, |h| {
            h.mem_store(CONSOLE_TX as i64, 4, 0x0000_0041)
        });
        assert_eq!(r, Ok(()));
        assert_eq!(p.console.tx_bytes, vec![b'A']);
        // Halfword store also captures the low byte.
        let r = with_host(&mut p, |h| h.mem_store(CONSOLE_TX as i64, 2, 0x4242));
        assert_eq!(r, Ok(()));
        assert_eq!(p.console.tx_bytes, vec![b'A', b'B']);
        // Byte store keeps the original PLATFORM.md behavior.
        let r = with_host(&mut p, |h| h.mem_store(CONSOLE_TX as i64, 1, b'C' as i64));
        assert_eq!(r, Ok(()));
        assert_eq!(p.console.tx_bytes, b"ABC");
    }

    #[test]
    fn uart_word_store_to_other_offsets_acknowledged() {
        let mut p = host_parts(SysRegs::default());
        // PL011 LCR_H / CR / IBRD writes: acknowledged, no TX bytes.
        for off in [0x02Cu64, 0x030, 0x024, 0x100] {
            let r = with_host(&mut p, |h| {
                h.mem_store((CONSOLE_BASE + off) as i64, 4, 0x70)
            });
            assert_eq!(r, Ok(()), "offset {:#x} rejected", off);
        }
        assert!(p.console.tx_bytes.is_empty());
    }

    #[test]
    fn uart_fr_read_reports_txfe_rxfe() {
        let mut p = host_parts(SysRegs::default());
        let got = with_host(&mut p, |h| h.mem_load(CONSOLE_PL011_FR as i64, 4));
        assert_eq!(got, Ok(0x90));
        // Byte and halfword reads see the same low bits.
        let got = with_host(&mut p, |h| h.mem_load(CONSOLE_PL011_FR as i64, 1));
        assert_eq!(got, Ok(0x90));
        let got = with_host(&mut p, |h| h.mem_load(CONSOLE_PL011_FR as i64, 2));
        assert_eq!(got, Ok(0x90));
        // Other PL011 offsets read as 0.
        let got = with_host(&mut p, |h| h.mem_load((CONSOLE_BASE + 0xFE0) as i64, 4));
        assert_eq!(got, Ok(0));
    }

    #[test]
    fn uart_rx_still_byte_oriented() {
        let mut p = host_parts(SysRegs::default());
        p.console.feed_rx(b"Z");
        let got = with_host(&mut p, |h| h.mem_load(CONSOLE_RX as i64, 1));
        assert_eq!(got, Ok(b'Z' as i64));
        let got = with_host(&mut p, |h| h.mem_load(CONSOLE_RX as i64, 1));
        assert_eq!(got, Ok(0), "queue drained");
    }

    #[test]
    fn uart_rejects_8byte_accesses() {
        let mut p = host_parts(SysRegs::default());
        let r = with_host(&mut p, |h| h.mem_store(CONSOLE_TX as i64, 8, 0x41));
        assert!(r.is_err(), "8-byte console store must still be rejected");
        let r = with_host(&mut p, |h| h.mem_load(CONSOLE_PL011_FR as i64, 8));
        assert!(r.is_err(), "8-byte console load must still be rejected");
    }

    // ---- timer (U5, injected clock) ----

    #[test]
    fn timer_fires_intid_27_on_injected_ticks_only() {
        let mut o = Orchestrator::new();
        // Enable the timer via CNTP_CTL_EL0 (bit 0) and arm the compare
        // via CNTP_CVAL_EL0 — the honest guest programming path.
        o.machine.cpu[0].sysregs.cntp_ctl_el0 = 1;
        o.machine.cpu[0].sysregs.cntp_cval_el0 = 5000;
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
        o.machine.cpu[0].sysregs.cntp_cval_el0 = 1;
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
        o.machine_mut().cpu[0].regs[2] = 0xFF; // (-5) & 0xFF != 0 -> Z=0

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
        // 0x4000_0010: CSINV X9,XZR,X5,EQ (0xDA8503E9) -> X9 = !0x10
        // 0x4000_0014: CSNEG X10,XZR,X5,EQ(0xDA8507EA) -> X10 = -0x10
        // 0x4000_0018: WFI
        // GB-26: the CSINV/CSNEG words were corrected to the real encoding
        // (op=bit30=1); GB-15's 0x9A850BE9/0x9A850FEA are unallocated
        // (capstone: INVALID) and now fall through to the U2 trap.
        let words = [
            0xF100_001F,
            0x9A85_07E7,
            0xF100_041F,
            0x9A85_07E8,
            0xDA85_03E9,
            0xDA85_07EA,
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
    fn gb26_csel_32bit_executes() {
        // GB-26: the 32-bit form is now implemented (GB-15 trapped it).
        // CSEL W5, W6, W5, HI (0x1A8580C5): C=1 -> W5 = W6 low 32 bits,
        // zero-extended.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x1A85_80C5);
        o.machine_mut().cpu[0].regs[6] = 0xFFFF_FFFF_ABCD_1234;
        o.machine_mut().cpu[0].regs[7] = 0xFFFF_FFFF_5678_9ABC;
        o.machine_mut().cpu[0].pstate = FLAG_C; // HI: C == 1 and Z == 0
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[5], 0xABCD_1234);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb15_csel_cond_nv_executes_always() {
        // CSEL X5, X6, X5, NV (0x9A85F0C5): cond=NV is legal and
        // always-true per ARM ARM (GB-26 fix: no longer trapped as
        // unallocated).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x9A85_F0C5);
        o.machine_mut().cpu[0].regs[6] = 0x1234_5678_9ABC_DEF0;
        o.machine_mut().cpu[0].regs[5] = 0xDEAD_BEEF_DEAD_BEEF;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[5], 0x1234_5678_9ABC_DEF0);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
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
        let got = with_host(&mut p, |h| h.mem_load(0xFFFF_FF80_096A_B158u64 as i64, 8));
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
                StepOutcome::Halted(HaltReason::FetchFault {
                    addr: 0xffff_ff80_096a_b000
                })
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
        // 0xF9000BE1 (capstone-verified; GB-26: the old 0xF8000BE1 word was
        // misassembled -- it decodes as STTR, not STR).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF900_0BE1);
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
        // 0xF9400BE2 (capstone-verified; GB-26: the old 0xF8400BE2 word was
        // misassembled -- it decodes as LDTR, not LDR).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF940_0BE2);
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
        // 0xB9000BE3 (capstone-verified; GB-26: the old 0xB8000BE3 word was
        // misassembled -- it decodes as STTR, not STR).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xB900_0BE3);
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
        // GB-23: STP X1, X2, [SP, #-16]! stores the pair at SP-16, SP-8.
        // 1010100110 imm7=-2(0x7E) 00010 11111 00001 = 0xA9BF0BE1.
        // GB-26: this is pre-index, so SP must be written back to SP-16
        // (the old fast path stored at the right address but dropped the
        // writeback; this assertion pins the fix).
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
        assert_eq!(o.machine().cpu[0].sp, sp - 16);
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
        o.machine_mut().ram[s + 8..s + 16].copy_from_slice(&0xBBBB_BBBB_BBBB_BBBBu64.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[1], 0xAAAA_AAAA_AAAA_AAAA);
        assert_eq!(o.machine().cpu[0].regs[2], 0xBBBB_BBBB_BBBB_BBBB);
        // Signed offset: no writeback, SP unchanged.
        assert_eq!(o.machine().cpu[0].sp, sp);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_sp_stp_signed_offset() {
        // GB-26: the exact kernel halt word 0xA9017BFD =
        // STP X29, X30, [SP, #16] (signed offset, no writeback).
        // The old fast path did not match op10 0x2A4 at all.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xA901_7BFD);
        o.machine_mut().cpu[0].regs[29] = 0xAAAA_AAAA_AAAA_AAAA;
        o.machine_mut().cpu[0].regs[30] = 0xBBBB_BBBB_BBBB_BBBB;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp + 16 - RAM_BASE) as usize;
        let mut b = [0u8; 8];
        b.copy_from_slice(&o.machine().ram[s..s + 8]);
        assert_eq!(u64::from_le_bytes(b), 0xAAAA_AAAA_AAAA_AAAA);
        b.copy_from_slice(&o.machine().ram[s + 8..s + 16]);
        assert_eq!(u64::from_le_bytes(b), 0xBBBB_BBBB_BBBB_BBBB);
        assert_eq!(o.machine().cpu[0].sp, sp);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_sp_ldp_preindex_writeback() {
        // GB-26: LDP X29, X30, [SP, #16]! loads from SP+16, SP+24, then
        // SP += 16. Word 0xA9C17BFD (capstone-verified).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xA9C1_7BFD);
        let s = (sp + 16 - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 8].copy_from_slice(&0xCCCC_CCCC_CCCC_CCCCu64.to_le_bytes());
        o.machine_mut().ram[s + 8..s + 16].copy_from_slice(&0xDDDD_DDDD_DDDD_DDDDu64.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[29], 0xCCCC_CCCC_CCCC_CCCC);
        assert_eq!(o.machine().cpu[0].regs[30], 0xDDDD_DDDD_DDDD_DDDD);
        assert_eq!(o.machine().cpu[0].sp, sp + 16);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_sp_stp_postindex_writeback() {
        // GB-26: STP X1, X2, [SP], #16 stores at SP, SP+8, then SP += 16.
        // Word 0xA8810BE1 (capstone-verified).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xA881_0BE1);
        o.machine_mut().cpu[0].regs[1] = 0x1111_1111_1111_1111;
        o.machine_mut().cpu[0].regs[2] = 0x2222_2222_2222_2222;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp - RAM_BASE) as usize;
        let mut b = [0u8; 8];
        b.copy_from_slice(&o.machine().ram[s..s + 8]);
        assert_eq!(u64::from_le_bytes(b), 0x1111_1111_1111_1111);
        b.copy_from_slice(&o.machine().ram[s + 8..s + 16]);
        assert_eq!(u64::from_le_bytes(b), 0x2222_2222_2222_2222);
        assert_eq!(o.machine().cpu[0].sp, sp + 16);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_sp_str_imm_64_halt_word() {
        // GB-26: the exact kernel halt word 0xF90007E8 =
        // STR X8, [SP, #8] (unsigned immediate, real STR encoding with
        // bit24=1 -- the old fast-path mask missed it).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF900_07E8);
        o.machine_mut().cpu[0].regs[8] = 0xCAFE_F00D_DEAD_BEEF;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp + 8 - RAM_BASE) as usize;
        let mut b = [0u8; 8];
        b.copy_from_slice(&o.machine().ram[s..s + 8]);
        assert_eq!(u64::from_le_bytes(b), 0xCAFE_F00D_DEAD_BEEF);
        assert_eq!(o.machine().cpu[0].sp, sp);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_sp_stp32_signed_offset() {
        // GB-26: STP W1, W2, [SP, #12] (32-bit signed offset, no writeback).
        // Word 0x29018BE1 (capstone-verified).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x2901_8BE1);
        o.machine_mut().cpu[0].regs[1] = 0xFFFF_FFFF_AAAA_AAAA;
        o.machine_mut().cpu[0].regs[2] = 0xFFFF_FFFF_BBBB_BBBB;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp + 12 - RAM_BASE) as usize;
        let mut b = [0u8; 4];
        b.copy_from_slice(&o.machine().ram[s..s + 4]);
        assert_eq!(u32::from_le_bytes(b), 0xAAAA_AAAA);
        b.copy_from_slice(&o.machine().ram[s + 4..s + 8]);
        assert_eq!(u32::from_le_bytes(b), 0xBBBB_BBBB);
        assert_eq!(o.machine().cpu[0].sp, sp);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb23_sp_store_does_not_clobber_xzr() {
        // GB-23: STR XZR, [SP, #0] stores 0 (XZR read), not SP.
        // 0xF90003FF (capstone-verified; GB-26: the old 0xF80003FF word was
        // misassembled -- it decodes as STTR, not STR).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF900_03FF);
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

    #[test]
    fn gb26_csinv_kernel_word_hs_taken() {
        // GB-26: exact kernel halt word 0xDA80202A = csinv x10, x1, x0, hs
        // (step 1249127). With C=1 (hs), X10 = X1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xDA80_202A);
        o.machine_mut().cpu[0].regs[0] = 0xAAAA_AAAA_AAAA_AAAA;
        o.machine_mut().cpu[0].regs[1] = 0x1234_5678_9ABC_DEF0;
        o.machine_mut().cpu[0].pstate = FLAG_C; // hs: C == 1
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[10], 0x1234_5678_9ABC_DEF0);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_csinv_kernel_word_hs_not_taken() {
        // Same word, C=0: X10 = ~X0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xDA80_202A);
        o.machine_mut().cpu[0].regs[0] = 0xAAAA_AAAA_AAAA_AAAA;
        o.machine_mut().cpu[0].regs[1] = 0x1234_5678_9ABC_DEF0;
        o.machine_mut().cpu[0].pstate = 0; // hs: C == 0
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[10], 0x5555_5555_5555_5555);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_csel_csinc_csneg_32bit() {
        // CSEL X5, X6, X7, EQ (0x9A8700C5): Z=1 -> X5 = X6.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x9A87_00C5);
        o.machine_mut().cpu[0].regs[6] = 11;
        o.machine_mut().cpu[0].regs[7] = 22;
        o.machine_mut().cpu[0].pstate = FLAG_Z;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[5], 11);

        // CSNEG X5, X6, X7, EQ (0xDA8704C5): Z=0 -> X5 = -X7.
        let mut o = sp_test_orchestrator(pc, sp, 0xDA87_04C5);
        o.machine_mut().cpu[0].regs[6] = 11;
        o.machine_mut().cpu[0].regs[7] = 22;
        o.machine_mut().cpu[0].pstate = 0;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[5], (-22i64) as u64);

        // CSINC W8, W9, W10, NE (0x1A8A_1528): Z=0 -> W8 = W9 low 32
        // bits, zero-extended (upper garbage in X9 must not leak).
        let mut o = sp_test_orchestrator(pc, sp, 0x1A8A_1528);
        o.machine_mut().cpu[0].regs[9] = 0xDEAD_BEEF_1234_5678;
        o.machine_mut().cpu[0].regs[10] = 5;
        o.machine_mut().cpu[0].pstate = 0;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[8], 0x1234_5678);

        // Same word, Z=1 (not taken): W8 = W10 + 1 = 6.
        let mut o = sp_test_orchestrator(pc, sp, 0x1A8A_1528);
        o.machine_mut().cpu[0].regs[9] = 0xDEAD_BEEF_1234_5678;
        o.machine_mut().cpu[0].regs[10] = 5;
        o.machine_mut().cpu[0].pstate = FLAG_Z;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[8], 6);
    }

    #[test]
    fn gb26_sub32_register_s0() {
        // Exact kernel word: SUB W0, W8, W19 (0x4B130100), S=0.
        // Measured halt at step 1249446 (pc 0xffffff8008df4fe0).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x4B13_0100);
        o.machine_mut().cpu[0].regs[8] = 100;
        o.machine_mut().cpu[0].regs[19] = 30;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[0], 70);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // 64-bit S=0 with Rd=31 writes SP (not XZR): ADD SP, SP, X1.
        // 0x8B0103FF (Rn=11111, Rd=11111, S=0): architecturally
        // ADD SP, SP, X1 (ARM ARM: Rn/Rd=31 name SP for S=0; Rm=31
        // would be XZR). capstone renders the 31s as xzr -- display
        // quirk, same as above.
        let mut o = sp_test_orchestrator(pc, sp, 0x8B01_03FF);
        o.machine_mut().cpu[0].regs[1] = 0x200;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].sp, sp + 0x200);
    }

    #[test]
    fn gb26_ldrh_register_offset() {
        // Exact kernel word: LDRH W10, [X11, X8, LSL #1] (0x7868796A).
        // Measured halt at step 1249461 (pc 0xffffff8008df463c).
        // option=011 (64-bit Rm), S=1: offset = X8 << 1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x7868_796A);
        let base = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[11] = base;
        o.machine_mut().cpu[0].regs[8] = 4;
        // X8 = 0xFFFF_FFFF_FFFF_FFF0 would sign-extend badly if Rm were
        // treated as 32-bit; option=011 names the 64-bit X8.
        let s = (base + 8 - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 2].copy_from_slice(&0xABCDu16.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[10], 0xABCD); // zero-extended
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_strb_register_offset() {
        // STRB W1, [X2, X3] (0x38236841, capstone-verified): stores the
        // low byte of W1 at X2 + X3 (option=011, S=0: no shift).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x3823_6841);
        let base = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[2] = base;
        o.machine_mut().cpu[0].regs[3] = 5;
        o.machine_mut().cpu[0].regs[1] = 0xDEAD_BEEF_CAFE_42FF;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (base + 5 - RAM_BASE) as usize;
        assert_eq!(o.machine().ram[s], 0xFF); // low byte only
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ldrh_post_index() {
        // Exact kernel word: LDRH W3, [X1], #2 (0x78402423, post-index).
        // Measured halt at step 1249482 (pc 0xffffff80082099ac).
        // Loads the halfword at [X1], then X1 += 2.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x7840_2423);
        let base = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[1] = base;
        let s = (base - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 2].copy_from_slice(&0x1234u16.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[3], 0x1234); // zero-extended
        assert_eq!(o.machine().cpu[0].regs[1], base + 2); // post-index writeback
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_sturh_unscaled() {
        // STURH W1, [X2, #-2]: unscaled-imm9 store, negative offset.
        // Encoding: size=01, opc=00, bit21=0, imm9=-2, idx=00, Rn=2, Rt=1.
        // word = 0x78000000 | (0x1FE << 12) | (0b00 << 10) | (2 << 5) | 1
        //      = 0x781FE041. Capstone-verified below via the test itself.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x781F_E041);
        let base = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[2] = base;
        o.machine_mut().cpu[0].regs[1] = 0xBEEF;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (base - 2 - RAM_BASE) as usize;
        assert_eq!(
            u16::from_le_bytes(o.machine().ram[s..s + 2].try_into().unwrap()),
            0xBEEF
        );
        // No writeback for unscaled.
        assert_eq!(o.machine().cpu[0].regs[2], base);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_madd32() {
        // Exact kernel word: MADD W8, W8, W10, W11 (0x1B0A2D08).
        // Measured halt at step 1249581 (pc 0xffffff8008df6d7c).
        // W8 = W11 + W8*W10, 32-bit wrapping, zero-extended.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x1B0A_2D08);
        o.machine_mut().cpu[0].regs[8] = 0x1_0000_0005; // W8 = 5
        o.machine_mut().cpu[0].regs[10] = 0xFFFF_FFFF; // W10 = -1
        o.machine_mut().cpu[0].regs[11] = 7; // W11 = 7
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        // 7 + 5 * 0xFFFFFFFF = 7 - 5 = 2 (mod 2^32).
        assert_eq!(o.machine().cpu[0].regs[8], 2);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_msub32() {
        // MSUB W0, W1, W2, W3 (0x1B028C20, capstone-verified):
        // W0 = W3 - W1*W2, 32-bit wrapping, zero-extended.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x1B02_8C20);
        o.machine_mut().cpu[0].regs[1] = 3;
        o.machine_mut().cpu[0].regs[2] = 4;
        o.machine_mut().cpu[0].regs[3] = 100;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[0], 88); // 100 - 12
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // Rd=31 (WZR) discards the result.
        let mut o = sp_test_orchestrator(pc, sp, 0x1B02_8C3F); // msub wzr, w1, w2, w3
        o.machine_mut().cpu[0].regs[1] = 3;
        o.machine_mut().cpu[0].regs[2] = 4;
        o.machine_mut().cpu[0].regs[3] = 100;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[0], 0);
    }

    #[test]
    fn gb26_strb_sp_imm() {
        // Exact kernel word: STRB W8, [SP] (0x390003E8, offset 0).
        // Measured halt at step 1249745 (pc 0xffffff8008df55c0).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x3900_03E8);
        o.machine_mut().cpu[0].regs[8] = 0xDEAD_BEEF_CAFE_42AB;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let s = (sp - RAM_BASE) as usize;
        assert_eq!(o.machine().ram[s], 0xAB); // low byte only
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ldrh_sp_imm() {
        // LDRH W1, [SP, #4] (0x79400BE1, capstone-verified): unsigned
        // imm12=2 scaled by 2. Zero-extends into W1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x7940_0BE1);
        let s = (sp + 4 - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 2].copy_from_slice(&0xCAFEu16.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[1], 0xCAFE);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_add_extended_sxtw() {
        // Exact kernel word: ADD X2, X22, W23, SXTW (0x8B37C2C2).
        // Measured halt at step 1250432 (pc 0xffffff800839b3d4).
        // X2 = X22 + SignExtend(W23), LSL #0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x8B37_C2C2);
        o.machine_mut().cpu[0].regs[22] = 0x1000;
        o.machine_mut().cpu[0].regs[23] = 0xFFFF_FFFF_FFFF_FFFB; // W23 = -5
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[2], 0xFFB); // 0x1000 - 5
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_cmp_extended_sxtw() {
        // Exact kernel word: CMP X22, W0, SXTW (0xEB20C2DF).
        // Measured halt at step 1250446 (pc 0xffffff800839b3e8).
        // X22 - SignExtend(W0); Rd=31 discards; NZCV set.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xEB20_C2DF);
        o.machine_mut().cpu[0].regs[22] = 0x1000;
        o.machine_mut().cpu[0].regs[0] = 5; // W0 = 5
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        // Result discarded (Rd=31), flags: 0x1000 - 5 > 0 => N=0,Z=0,C=1,V=0.
        let pstate = o.machine().cpu[0].pstate;
        assert_eq!(pstate & FLAGS_NZCV_MASK, 0x2000_0000); // C set
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_prfm_is_nop() {
        // Exact kernel word: PRFM PSTL1STRM, [X0] (0xF9800011).
        // Measured halt at step 1250435 (pc 0xffffff800873ecdc).
        // Prefetch is a hint: no register or memory changes, just pc+4.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF980_0011);
        o.machine_mut().cpu[0].regs[0] = RAM_BASE + 0x3000;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[0], RAM_BASE + 0x3000);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ldxr_stxr_pair() {
        // Exact kernel word: LDXR W16, [X0] (0x885F7C10).
        // Measured halt at step 1250436 (pc 0xffffff800873ece0).
        // Then STXR W1, W16, [X0] (0x88007C10): store commits, Ws=0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x885F_7C10);
        let addr = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[0] = addr;
        let a = (addr - RAM_BASE) as usize;
        o.machine_mut().ram[a..a + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[16], 0xDEAD_BEEF);
        assert_eq!(o.exclusive, Some((addr, 4)));

        // STXR W1, W16, [X0] = 0x88017C10 (capstone-verified).
        o.machine_mut().cpu[0].regs[16] = 0x1234_5678;
        // Re-point pc at the STXR word.
        let off = (pc - RAM_BASE) as usize;
        o.machine_mut().ram[off..off + 4].copy_from_slice(&0x8801_7C10u32.to_le_bytes());
        o.machine_mut().cpu[0].pc = pc;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[1], 0); // Ws=0: success
        assert_eq!(
            u32::from_le_bytes(o.machine().ram[a..a + 4].try_into().unwrap()),
            0x1234_5678
        );
        assert_eq!(o.exclusive, None);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_stxr_fails_without_monitor() {
        // STXR with no prior LDXR: no store, Ws=1.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x8801_7C10); // stxr w1, w16, [x0]
        let addr = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[0] = addr;
        o.machine_mut().cpu[0].regs[16] = 0x1234_5678;
        let a = (addr - RAM_BASE) as usize;
        o.machine_mut().ram[a..a + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[1], 1); // Ws=1: failure
        assert_eq!(
            u32::from_le_bytes(o.machine().ram[a..a + 4].try_into().unwrap()),
            0xDEAD_BEEF // untouched
        );
    }

    #[test]
    fn gb26_stlxr_release_variant() {
        // Exact kernel word: STLXR W17, W2, [X0] (0x8811FC02).
        // Measured halt at step 1250439 (pc 0xffffff800873ecec).
        // Release is a NOP on single-vCPU: same as STXR.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x885F_7C10); // ldxr w16, [x0]
        let addr = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[0] = addr;
        let a = (addr - RAM_BASE) as usize;
        o.machine_mut().ram[a..a + 4].copy_from_slice(&0xAAAA_BBBBu32.to_le_bytes());
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[16], 0xAAAA_BBBB);

        o.machine_mut().cpu[0].regs[2] = 0x1122_3344;
        let off = (pc - RAM_BASE) as usize;
        o.machine_mut().ram[off..off + 4].copy_from_slice(&0x8811_FC02u32.to_le_bytes());
        o.machine_mut().cpu[0].pc = pc;
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[17], 0); // Ws=0: success
        assert_eq!(
            u32::from_le_bytes(o.machine().ram[a..a + 4].try_into().unwrap()),
            0x1122_3344
        );
    }

    #[test]
    fn gb26_ccmp_immediate() {
        // Exact kernel word: CCMP X4, #0, #4, NE (0xFA401884).
        // Measured halt at step 1250639 (pc 0xffffff80095dfd54).
        // If NE holds: NZCV = X4 - 0. Else: NZCV = 0b0100 (C set).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;

        // Case 1: NE holds (Z=0). X4=5, 5-0 => N=0,Z=0,C=1,V=0.
        let mut o = sp_test_orchestrator(pc, sp, 0xFA40_1884);
        o.machine_mut().cpu[0].regs[4] = 5;
        o.machine_mut().cpu[0].pstate = 0; // Z=0 => NE true
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0x2000_0000);

        // Case 2: NE fails (Z=1). NZCV = nzcv_imm = 0b0100 => N=0,Z=1,C=0,V=0.
        let mut o = sp_test_orchestrator(pc, sp, 0xFA40_1884);
        o.machine_mut().cpu[0].regs[4] = 5;
        o.machine_mut().cpu[0].pstate = 0x4000_0000; // Z=1 => NE false
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0x4000_0000);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ccmn_immediate() {
        // CCMN X0, #1, #0, EQ (0xBA410800). The old mask forced op=1,
        // so CCMN was unreachable; this verifies the fix.
        // If EQ holds: NZCV = X0 + 1. Else: NZCV = 0b0000.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;

        // Case 1: EQ holds (Z=1). X0=MAX, MAX+1 => 0, C=1, Z=1.
        let mut o = sp_test_orchestrator(pc, sp, 0xBA41_0800);
        o.machine_mut().cpu[0].regs[0] = 0xFFFF_FFFF_FFFF_FFFF;
        o.machine_mut().cpu[0].pstate = 0x4000_0000; // Z=1 => EQ true
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0x6000_0000);

        // Case 2: EQ fails (Z=0). NZCV = 0.
        let mut o = sp_test_orchestrator(pc, sp, 0xBA41_0800);
        o.machine_mut().cpu[0].regs[0] = 0xFFFF_FFFF_FFFF_FFFF;
        o.machine_mut().cpu[0].pstate = 0; // Z=0 => EQ false
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ccmp_rn31_is_xzr() {
        // CCMP XZR, #0, #0, EQ (0xFA400BE0): Rn=31 names XZR, not SP.
        // EQ holds (Z=1): NZCV = XZR - 0 = 0 => N=0,Z=1,C=1,V=0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xFA40_0BE0);
        o.machine_mut().cpu[0].sp = 0xFFFF_FFFF_FFFF_0000; // nonzero: must not leak in
        o.machine_mut().cpu[0].pstate = 0x4000_0000; // Z=1 => EQ true
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0x6000_0000);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_cmp_extended_rn31_is_xzr() {
        // CMP XZR, WZR, SXTW (0xEB20C3FF): S=1, Rn=31 names XZR, not SP.
        // 0 - 0 = 0 => N=0,Z=1,C=1,V=0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xEB20_C3FF);
        o.machine_mut().cpu[0].sp = 0xFFFF_FFFF_FFFF_0000; // nonzero: must not leak in
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0x6000_0000);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_subs_imm_rn31_is_xzr() {
        // SUBS XZR, XZR, #0 (0xF10003FF): S=1, Rn=31 names XZR, not SP.
        // 0 - 0 = 0 => N=0,Z=1,C=1,V=0.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF100_03FF);
        o.machine_mut().cpu[0].sp = 0xFFFF_FFFF_FFFF_0000; // nonzero: must not leak in
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].pstate & FLAGS_NZCV_MASK, 0x6000_0000);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ldrsw_sign_extends() {
        // LDRSW X5, [SP, #16] (0xB98013E5): must sign-extend 32->64.
        // 0xFFFFFFFF in memory => X5 = 0xFFFFFFFFFFFFFFFF (not 0xFFFFFFFF).
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xB980_13E5);
        let s = (sp + 16 - RAM_BASE) as usize;
        o.machine_mut().ram[s..s + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[5], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_csel_always() {
        // CSEL X0, X1, X2, AL (0x9A82E020): AL/NV are legal, always-true.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x9A82_E020);
        o.machine_mut().cpu[0].regs[1] = 42;
        o.machine_mut().cpu[0].regs[2] = 7;
        o.machine_mut().cpu[0].pstate = 0; // flags irrelevant for AL
        assert!(matches!(o.step_vcpu(), StepOutcome::Continue));
        assert_eq!(o.machine().cpu[0].regs[0], 42);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_rev_64bit() {
        // Exact kernel word: REV X6, X6 (0xDAC00CC6).
        // Measured halt at step 1250673 (pc 0xffffff8008209fc8).
        // Reverses all 8 bytes.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xDAC0_0CC6);
        o.machine_mut().cpu[0].regs[6] = 0x0102_0304_0506_0708;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[6], 0x0807_0605_0403_0201);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ldrh_unsigned_imm_nonsp() {
        // Exact kernel word: LDRH W11, [X9, #0x2E2] (0x7945C52B).
        // Measured halt at step 1250850 (pc 0xffffff80095dfeec).
        // Non-SP Rn with halfword: U2 traps ("not expressible in IrOp"),
        // so the fast path handles it. Offset = 369*2 = 738.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x7945_C52B);
        let base = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[9] = base;
        // Write halfword 0xABCD at base+738.
        let off = (base - RAM_BASE) as usize + 738;
        o.machine_mut().ram[off..off + 2].copy_from_slice(&0xABCDu16.to_le_bytes());
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        assert_eq!(o.machine().cpu[0].regs[11], 0xABCD);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_ror_extr_alias() {
        // Exact kernel word: ROR X8, X8, #2 (0x93C80908), an EXTR alias
        // with Rm=Rn. Measured halt at step 1250891 (pc 0xffffff80095dff90).
        // ROR by 2: low 2 bits move to the top.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0x93C8_0908);
        o.machine_mut().cpu[0].regs[8] = 0x8000_0000_0000_0001;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        // Rotate right by 2: 0x8000...0001 -> 0x6000...0000.
        assert_eq!(o.machine().cpu[0].regs[8], 0x6000_0000_0000_0000);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_stlr_basic() {
        // Exact kernel word: STLR X19, [X8] (0xC89FFD13).
        // Measured halt at step 1250920 (pc 0xffffff80095dffcc).
        // Single-copy atomic store with release semantics (NOP on
        // single-vCPU); no exclusive-monitor interaction.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xC89F_FD13);
        let base = RAM_BASE + 0x3000;
        o.machine_mut().cpu[0].regs[8] = base;
        o.machine_mut().cpu[0].regs[19] = 0xDEAD_BEEF_CAFE_1234;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        let off = (base - RAM_BASE) as usize;
        assert_eq!(
            u64::from_le_bytes(o.machine().ram[off..off + 8].try_into().unwrap()),
            0xDEAD_BEEF_CAFE_1234
        );
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn gb26_str_preindex_sp() {
        // Exact kernel word: STR X30, [SP, #-0x10]! (0xF81F0FFE).
        // Measured halt at step 1252125 (pc 0xffffff80095d7540).
        // Pre-index: SP -= 16, then store at new SP.
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = sp_test_orchestrator(pc, sp, 0xF81F_0FFE);
        o.machine_mut().cpu[0].regs[30] = 0x1122_3344_5566_7788;
        let outcome = o.step_vcpu();
        assert!(
            matches!(outcome, StepOutcome::Continue),
            "expected Continue, got: {outcome:?}"
        );
        // SP moved down by 16.
        assert_eq!(o.machine().cpu[0].sp, sp - 16);
        // Value stored at new SP.
        let off = (sp - 16 - RAM_BASE) as usize;
        assert_eq!(
            u64::from_le_bytes(o.machine().ram[off..off + 8].try_into().unwrap()),
            0x1122_3344_5566_7788
        );
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn test_survey_mode_skips_illegal_word_and_continues() {
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;
        let mut o = Orchestrator::new();
        o.machine_mut().cpu[0].pc = pc;
        o.machine_mut().cpu[0].sp = sp;
        o.machine_mut().cpu[0].sysregs.sctlr_el1 = 0; // MMU off

        let off = (pc - RAM_BASE) as usize;
        // 0x0000_0000 is unallocated / illegal
        o.machine_mut().ram[off..off + 4].copy_from_slice(&0x0000_0000u32.to_le_bytes());
        // 0xD503_201F is NOP at pc + 4
        o.machine_mut().ram[off + 4..off + 8].copy_from_slice(&0xD503_201Fu32.to_le_bytes());

        // Without survey mode, it halts with IllegalInstruction
        let outcome_halt = o.step_vcpu();
        assert_eq!(
            outcome_halt,
            StepOutcome::Halted(HaltReason::IllegalInstruction {
                addr: pc,
                word: 0x0000_0000
            })
        );

        // Fresh orchestrator with survey mode enabled: skips illegal word and continues
        let mut o_survey = Orchestrator::new();
        o_survey.machine_mut().cpu[0].pc = pc;
        o_survey.machine_mut().cpu[0].sp = sp;
        o_survey.machine_mut().cpu[0].sysregs.sctlr_el1 = 0;
        o_survey.machine_mut().ram[off..off + 4].copy_from_slice(&0x0000_0000u32.to_le_bytes());
        o_survey.machine_mut().ram[off + 4..off + 8].copy_from_slice(&0xD503_201Fu32.to_le_bytes());

        o_survey.set_survey_mode(true);
        assert!(o_survey.survey_mode());

        // Step 1: hits illegal instruction, skips to pc + 4 and continues
        let outcome1 = o_survey.step_vcpu();
        assert_eq!(outcome1, StepOutcome::Continue);
        assert_eq!(o_survey.steps(), 1);
        assert_eq!(o_survey.machine().cpu[0].pc, pc + 4);
        assert_eq!(o_survey.survey_summary().len(), 1);
        assert_eq!(o_survey.survey_summary()[0].pc, pc);
        assert_eq!(o_survey.survey_summary()[0].word, 0x0000_0000);

        // Step 2: executes NOP at pc + 4 and continues to pc + 8
        let outcome2 = o_survey.step_vcpu();
        assert_eq!(outcome2, StepOutcome::Continue);
        assert_eq!(o_survey.steps(), 2);
        assert_eq!(o_survey.machine().cpu[0].pc, pc + 8);
    }

    // =========================================================================
    // F2-branch family tests (all 25 mnemonics, taken and not-taken witnesses)
    // =========================================================================

    #[test]
    fn f2_b_cond_all_16_conditions_taken_and_not_taken() {
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;

        // Conditions:
        // (cond, pstate_taken, pstate_not_taken, name)
        let test_cases: [(u8, u64, u64, &str); 16] = [
            (0, FLAG_Z, 0, "EQ"),
            (1, 0, FLAG_Z, "NE"),
            (2, FLAG_C, 0, "CS/HS"),
            (3, 0, FLAG_C, "CC/LO"),
            (4, FLAG_N, 0, "MI"),
            (5, 0, FLAG_N, "PL"),
            (6, FLAG_V, 0, "VS"),
            (7, 0, FLAG_V, "VC"),
            (8, FLAG_C, 0, "HI"), // C=1, Z=0 -> taken; C=0, Z=0 -> not taken
            (9, 0, FLAG_C, "LS"), // C=0 -> taken; C=1, Z=0 -> not taken
            (10, FLAG_N | FLAG_V, FLAG_N, "GE"), // N==V -> taken; N!=V -> not taken
            (11, FLAG_N, FLAG_N | FLAG_V, "LT"), // N!=V -> taken; N==V -> not taken
            (12, FLAG_N | FLAG_V, FLAG_Z | FLAG_N | FLAG_V, "GT"), // Z=0, N==V -> taken; Z=1 -> not taken
            (13, FLAG_Z, FLAG_N | FLAG_V, "LE"), // Z=1 -> taken; Z=0, N==V -> not taken
            (14, 0, 0, "AL"),                    // always taken
            (15, 0, 0, "NV"),                    // always taken
        ];

        for (cond, pstate_taken, pstate_not_taken, name) in test_cases {
            // b.cond +8 (imm19 = 2): word = 0x54000000 | (2 << 5) | cond = 0x54000040 | cond
            let word = 0x5400_0040 | (cond as u32);

            // 1. Taken test
            let mut o_taken = sp_test_orchestrator(pc, sp, word);
            o_taken.machine_mut().cpu[0].pstate = pstate_taken;
            assert_eq!(
                o_taken.step_vcpu(),
                StepOutcome::Continue,
                "B.{name} taken step failed"
            );
            assert_eq!(
                o_taken.machine().cpu[0].pc,
                pc + 8,
                "B.{name} should be taken to pc+8"
            );

            // 2. Not-taken test (for conditions other than AL and NV)
            if cond < 14 {
                let mut o_not_taken = sp_test_orchestrator(pc, sp, word);
                o_not_taken.machine_mut().cpu[0].pstate = pstate_not_taken;
                assert_eq!(
                    o_not_taken.step_vcpu(),
                    StepOutcome::Continue,
                    "B.{name} not-taken step failed"
                );
                assert_eq!(
                    o_not_taken.machine().cpu[0].pc,
                    pc + 4,
                    "B.{name} should fall through to pc+4"
                );
            } else {
                // For AL and NV, test that even with all flags set, they are ALWAYS taken
                let mut o_all_flags = sp_test_orchestrator(pc, sp, word);
                o_all_flags.machine_mut().cpu[0].pstate = FLAGS_NZCV_MASK;
                assert_eq!(
                    o_all_flags.step_vcpu(),
                    StepOutcome::Continue,
                    "B.{name} always-taken step with all flags failed"
                );
                assert_eq!(
                    o_all_flags.machine().cpu[0].pc,
                    pc + 8,
                    "B.{name} must be taken even with all flags set"
                );
            }
        }
    }

    #[test]
    fn f2_b_cond_negative_offset() {
        let pc = RAM_BASE + 0x2000;
        let sp = RAM_BASE + 0x3000;
        // B.NE -20 (-5 instructions, imm19 = 0x7FFFB): 0x54FF_FF61
        let word = 0x54FF_FF61;

        // Taken (Z=0):
        let mut o = sp_test_orchestrator(pc, sp, word);
        o.machine_mut().cpu[0].pstate = 0;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc - 20);

        // Not taken (Z=1):
        let mut o = sp_test_orchestrator(pc, sp, word);
        o.machine_mut().cpu[0].pstate = FLAG_Z;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn f2_tbz_tbnz_exhaustive_widths_and_edges() {
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;

        // TBZ W0, #0, +8 (0x3600_0040)
        let mut o = sp_test_orchestrator(pc, sp, 0x3600_0040);
        o.machine_mut().cpu[0].regs[0] = 0; // bit 0 == 0 -> taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0x3600_0040);
        o.machine_mut().cpu[0].regs[0] = 1; // bit 0 == 1 -> not taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // TBNZ W0, #31, +8 (0x37F8_0040)
        let mut o = sp_test_orchestrator(pc, sp, 0x37F8_0040);
        o.machine_mut().cpu[0].regs[0] = 0x8000_0000; // bit 31 == 1 -> taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0x37F8_0040);
        o.machine_mut().cpu[0].regs[0] = 0x7FFF_FFFF; // bit 31 == 0 -> not taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // TBZ X0, #32, +8 (0xB600_0040, b5=1, b40=0)
        let mut o = sp_test_orchestrator(pc, sp, 0xB600_0040);
        o.machine_mut().cpu[0].regs[0] = 0; // bit 32 == 0 -> taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0xB600_0040);
        o.machine_mut().cpu[0].regs[0] = 1 << 32; // bit 32 == 1 -> not taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // TBNZ X0, #63, +8 (0xB7F8_0040, b5=1, b40=31)
        let mut o = sp_test_orchestrator(pc, sp, 0xB7F8_0040);
        o.machine_mut().cpu[0].regs[0] = 1 << 63; // bit 63 == 1 -> taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0xB7F8_0040);
        o.machine_mut().cpu[0].regs[0] = 0; // bit 63 == 0 -> not taken
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // Edge case: Rt = 31 (WZR / XZR)
        // TBZ WZR, #0, +8 (0x3600_005F) -> always 0 -> taken
        let mut o = sp_test_orchestrator(pc, sp, 0x3600_005F);
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        // TBNZ WZR, #0, +8 (0x3700_005F) -> always 0 -> not taken
        let mut o = sp_test_orchestrator(pc, sp, 0x3700_005F);
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn f2_cbz_cbnz_32_and_64_bit() {
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;

        // 64-bit CBZ X0, +8 (0xB400_0040)
        let mut o = sp_test_orchestrator(pc, sp, 0xB400_0040);
        o.machine_mut().cpu[0].regs[0] = 0;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0xB400_0040);
        o.machine_mut().cpu[0].regs[0] = 1;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // 64-bit CBNZ X0, +8 (0xB500_0040)
        let mut o = sp_test_orchestrator(pc, sp, 0xB500_0040);
        o.machine_mut().cpu[0].regs[0] = 1;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0xB500_0040);
        o.machine_mut().cpu[0].regs[0] = 0;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // 32-bit CBZ W0, +8 (0x3400_0040)
        // Upper 32 bits non-zero, lower 32 bits zero -> must be taken!
        let mut o = sp_test_orchestrator(pc, sp, 0x3400_0040);
        o.machine_mut().cpu[0].regs[0] = 0xDEAD_BEEF_0000_0000;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        let mut o = sp_test_orchestrator(pc, sp, 0x3400_0040);
        o.machine_mut().cpu[0].regs[0] = 0x0000_0000_0000_0001;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // 32-bit CBNZ W0, +8 (0x3500_0040)
        // Upper 32 bits non-zero, lower 32 bits non-zero -> taken!
        let mut o = sp_test_orchestrator(pc, sp, 0x3500_0040);
        o.machine_mut().cpu[0].regs[0] = 0xDEAD_BEEF_0000_0001;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        // Upper 32 bits non-zero, lower 32 bits zero -> not taken!
        let mut o = sp_test_orchestrator(pc, sp, 0x3500_0040);
        o.machine_mut().cpu[0].regs[0] = 0xDEAD_BEEF_0000_0000;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);

        // Rt = 31 (XZR / WZR)
        // CBZ XZR, +8 (0xB400_005F) -> always 0 -> taken
        let mut o = sp_test_orchestrator(pc, sp, 0xB400_005F);
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        // CBNZ XZR, +8 (0xB500_005F) -> always 0 -> not taken
        let mut o = sp_test_orchestrator(pc, sp, 0xB500_005F);
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 4);
    }

    #[test]
    fn f2_unconditional_b_bl_br_blr_ret() {
        let pc = RAM_BASE + 0x1000;
        let sp = RAM_BASE + 0x2000;

        // B +8 (0x1400_0002)
        let mut o = sp_test_orchestrator(pc, sp, 0x1400_0002);
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);

        // BL +8 (0x9400_0002)
        let mut o = sp_test_orchestrator(pc, sp, 0x9400_0002);
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, pc + 8);
        assert_eq!(o.machine().cpu[0].regs[30], pc + 4);

        // BR X1 (0xD61F_0020)
        let target = RAM_BASE + 0x3000;
        let mut o = sp_test_orchestrator(pc, sp, 0xD61F_0020);
        o.machine_mut().cpu[0].regs[1] = target;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, target);

        // BLR X1 (0xD63F_0020)
        let mut o = sp_test_orchestrator(pc, sp, 0xD63F_0020);
        o.machine_mut().cpu[0].regs[1] = target;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, target);
        assert_eq!(o.machine().cpu[0].regs[30], pc + 4);

        // BLR X30 (0xD63F_03C0) — critical hazard: Rn == 30
        let mut o = sp_test_orchestrator(pc, sp, 0xD63F_03C0);
        o.machine_mut().cpu[0].regs[30] = target;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(
            o.machine().cpu[0].pc,
            target,
            "BLR X30 must branch to old X30 target"
        );
        assert_eq!(
            o.machine().cpu[0].regs[30],
            pc + 4,
            "BLR X30 must write link address into X30"
        );

        // RET (X30) (0xD65F_03C0)
        let ret_target = RAM_BASE + 0x4000;
        let mut o = sp_test_orchestrator(pc, sp, 0xD65F_03C0);
        o.machine_mut().cpu[0].regs[30] = ret_target;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, ret_target);

        // RET X2 (0xD65F_0040)
        let mut o = sp_test_orchestrator(pc, sp, 0xD65F_0040);
        o.machine_mut().cpu[0].regs[2] = ret_target + 0x100;
        assert_eq!(o.step_vcpu(), StepOutcome::Continue);
        assert_eq!(o.machine().cpu[0].pc, ret_target + 0x100);
    }
}
