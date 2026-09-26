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
//! the WASM block with wasmtime (native; the browser will use its own engine
//! in Wave 4 — U3 emits standard WASM so the boundary is clean).
//!
//! U3's module shape (frozen, Wave 1): ONE function `() -> i64` with 256
//! zero-initialized `i64` locals, NO params, NO imports, NO exports. The i64
//! result is the exit address (`-1` = ExitVm sentinel). Consequences the
//! orchestrator honors, not works around:
//!
//! - **Register state does not survive a call.** Locals are zero on entry,
//!   and there is no channel to read them back — the only observable is the
//!   exit address. The orchestrator therefore treats each compiled block as a
//!   *control-flow evaluator*: it proves decode→lift→compile→execute chaining
//!   is real (exit addresses chain blocks), but it never pretends registers
//!   persist. Threading register state across calls needs a U3 module-shape
//!   revision (params/globals/shared memory) — Wave-4 work, stated here.
//! - **The function is not exported.** U3 emits sections 1/3/10 only. The
//!   runtime's linking step appends a minimal export section
//!   (`export "run" func 0`) to a *copy* of the bytes before instantiating.
//!   U3's own bytes (the cache key) are never mutated.
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

use std::collections::{HashMap, VecDeque};

use pathn_contracts::adapters::{BlobStore, InputSource};
use pathn_contracts::cpu::{
    BlockExit, DecodeResult, Instruction, IrBlock, IrOp, IrqState, MemFault, MmuState,
};
use pathn_contracts::device::{DevEvent, DevOut, GpuDevState, TransportState};
use pathn_contracts::machine::{CpuState, MachineState};
use sha2::{Digest, Sha256};

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
    IllegalInstruction { addr: u64, word: u32 },
    Unsupported { addr: u64, reason: &'static str },
    WasmTrap { addr: u64, message: String },
    FetchFault { addr: u64 },
    ExitVm,
    StepLimitExceeded,
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
    /// Injected cycle counter — the only "clock". Never wall time.
    pub clock_cycles: u64,
    pub steps: u64,
    pub halted: Option<HaltReason>,
    /// Per-queue last-notified used index for EVENT_IDX decisions.
    last_notified: Vec<u16>,
    engine: wasmtime::Engine,
    /// pc → linked wasmtime Module. Keyed by pc; assumes immutable code —
    /// call `invalidate_code_cache` after any RAM write to a code page.
    block_cache: HashMap<u64, wasmtime::Module>,
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
            clock_cycles: 0,
            steps: 0,
            halted: None,
            last_notified: Vec::new(),
            engine: wasmtime::Engine::default(),
            block_cache: HashMap::new(),
        }
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
        self.block_cache.len()
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
        self.block_cache.clear();
    }

    // ---- memory path (M0: MMU disabled, physical addresses = guest addresses)
    // ----

    /// Translate a guest physical address to a RAM offset. M0 runs with the
    /// MMU disabled, so this is identity-with-bounds-check, documented here.
    /// (When the MMU is enabled in M1+, this routes through U4 `translate`.)
    fn ram_offset(&self, pa: u64, len: u64) -> Result<usize, MemFault> {
        let off = pa
            .checked_sub(RAM_BASE)
            .ok_or(MemFault::TranslationFault { va: pa })?;
        let end = off
            .checked_add(len)
            .ok_or(MemFault::TranslationFault { va: pa })?;
        if end > self.machine.ram.len() as u64 {
            return Err(MemFault::TranslationFault { va: pa });
        }
        Ok(off as usize)
    }

    fn fetch_word(&self, pc: u64) -> Result<u32, HaltReason> {
        match self.ram_offset(pc, 4) {
            Ok(off) => Ok(u32::from_le_bytes(
                self.machine.ram[off..off + 4].try_into().unwrap(),
            )),
            Err(_) => Err(HaltReason::FetchFault { addr: pc }),
        }
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
            let halt = HaltReason::Unsupported { addr: pc, reason };
            self.halted = Some(halt.clone());
            return StepOutcome::Halted(halt);
        }

        // Block exits: an explicit Branch op wins, else fall through.
        let exits = match branch_target(&ops) {
            Some(t) => vec![BlockExit::Branch(t)],
            None => vec![BlockExit::FallThrough(pc.wrapping_add(4))],
        };
        let block = IrBlock {
            entry_addr: pc,
            ops,
            exits,
        };

        // Compile (U3) via the block cache, then link + execute.
        let module = match self.compile_cached(pc, &block) {
            Ok(m) => m,
            Err(message) => {
                let halt = HaltReason::WasmTrap { addr: pc, message };
                self.halted = Some(halt.clone());
                return StepOutcome::Halted(halt);
            }
        };
        match call_run(&self.engine, &module) {
            Ok(exit_addr) => {
                if exit_addr < 0 {
                    let halt = HaltReason::ExitVm;
                    self.halted = Some(halt.clone());
                    return StepOutcome::Halted(halt);
                }
                self.machine.cpu[0].pc = exit_addr as u64;
            }
            Err(message) => {
                let halt = HaltReason::WasmTrap { addr: pc, message };
                self.halted = Some(halt.clone());
                return StepOutcome::Halted(halt);
            }
        }

        self.steps += 1;
        // Injected clock: the timer advances only here, never wall time.
        self.tick_clock(TIMER_CYCLES_PER_STEP);
        StepOutcome::Continue
    }

    /// Run until halt or `max_steps`. Returns the halt reason (or
    /// `StepLimitExceeded` when the budget runs out first).
    pub fn run_until_halt(&mut self, max_steps: u64) -> HaltReason {
        for _ in 0..max_steps {
            match self.step_vcpu() {
                StepOutcome::Continue => {}
                StepOutcome::Halted(reason) => return reason,
            }
        }
        let halt = HaltReason::StepLimitExceeded;
        self.halted = Some(halt.clone());
        halt
    }

    /// Compile one block with U3 and link it (cache by pc).
    fn compile_cached(&mut self, pc: u64, block: &IrBlock) -> Result<wasmtime::Module, String> {
        if let Some(m) = self.block_cache.get(&pc) {
            return Ok(m.clone());
        }
        let wasm = u3_wasm_jit::compile(block);
        let linked = link_run_export(&wasm.bytes);
        let module =
            wasmtime::Module::new(&self.engine, &linked).map_err(|e| format!("module: {e}"))?;
        self.block_cache.insert(pc, module.clone());
        Ok(module)
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

/// The runtime's linking step: insert `export "run" func 0` into a COPY of
/// U3's module bytes. U3's own output (cache key) is never mutated.
/// WASM orders non-custom sections by increasing id, so the export section
/// (id 7) is inserted BEFORE the code section (id 10), never appended.
fn link_run_export(module_bytes: &[u8]) -> Vec<u8> {
    // Build the export-section payload first.
    let mut payload = Vec::new();
    payload.push(1u8); // one export
    payload.push(3u8); // name length
    payload.extend_from_slice(b"run");
    payload.push(0x00); // kind: func
    payload.push(0x00); // func index 0
    let mut section = Vec::with_capacity(payload.len() + 2);
    section.push(7u8); // export section id
    let mut sz = payload.len() as u32;
    loop {
        let b = (sz & 0x7F) as u8;
        sz >>= 7;
        if sz == 0 {
            section.push(b);
            break;
        }
        section.push(b | 0x80);
    }
    section.extend_from_slice(&payload);
    // Find the code section (id 10) and splice the export section before it.
    let mut i = 8; // skip magic + version
    let mut code_at = module_bytes.len(); // default: append (no code section)
    while i < module_bytes.len() {
        let id = module_bytes[i];
        i += 1;
        let (sec_len, n) = read_leb128(&module_bytes[i..]);
        i += n;
        if id == 10 {
            code_at = i - 1 - n; // section start = id byte
            break;
        }
        i += sec_len as usize;
    }
    let mut out = Vec::with_capacity(module_bytes.len() + section.len());
    out.extend_from_slice(&module_bytes[..code_at]);
    out.extend_from_slice(&section);
    out.extend_from_slice(&module_bytes[code_at..]);
    out
}

/// Read an unsigned LEB128 from the front of `bytes`; returns (value, bytes
/// consumed). Malformed input saturates instead of panicking.
fn read_leb128(bytes: &[u8]) -> (u64, usize) {
    let mut v: u64 = 0;
    let mut shift = 0u32;
    for (n, &b) in bytes.iter().enumerate() {
        if shift < 64 {
            v |= ((b & 0x7F) as u64) << shift;
        }
        shift += 7;
        if b & 0x80 == 0 {
            return (v, n + 1);
        }
    }
    (v, bytes.len())
}

/// Call the linked `run() -> i64`. The i64 is U3's exit address.
fn call_run(engine: &wasmtime::Engine, module: &wasmtime::Module) -> Result<i64, String> {
    let mut store = wasmtime::Store::new(engine, ());
    let linker = wasmtime::Linker::new(engine);
    let instance = linker
        .instantiate(&mut store, module)
        .map_err(|e| format!("instantiate: {e}"))?;
    let func = instance
        .get_typed_func::<(), i64>(&mut store, "run")
        .map_err(|e| format!("export 'run': {e}"))?;
    func.call(&mut store, ()).map_err(|e| format!("trap: {e}"))
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
        "Branch: only unconditional immediate B is lifted",
        "Branch: unsupported encoding",
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
}
