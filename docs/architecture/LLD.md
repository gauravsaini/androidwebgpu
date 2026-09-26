# LLD — Native ARM Path: Units & Strict Contracts

Companion: `HLD.md`, `SWARM.md`, `../../grandvision.md`.

## 0. How to read this doc

Har unit ek **completely independent module** hai. Units ke beech ek hi coupling allowed hai:
neeche frozen contracts. Koi unit doosre unit ke internals, globals, ya hidden state ko touch
nahi karega.

**Purity legend**
- `PURE` — no state, deterministic: same inputs → same outputs, always.
- `EXPLICIT-STATE` — state hai par struct mein, caller thread karta hai. Function signature mein dikhta hai.
- `QUARANTINED` — impurity allowed hai, par sirf is unit ke andar (adapters, orchestrator).

**Status legend:** `EXISTS` (Path E se reuse, tested) · `PARTIAL` (code hai, adhura/unwired) ·
`NEW` (likhna hai).

---

## Frozen contract schemas

Ye types sab units share karte hain. Inko badalna = contract break = swarm rebuild.

```rust
// ---- CPU / JIT ----
pub struct Instruction { pub addr: u64, pub word: u32, pub kind: InsnKind, /* ... */ }
pub enum DecodeResult { Ok(Instruction), Illegal { word: u32 } }

pub struct IrOp { /* single SSA-style op: add/mul/load/store/branch/syscall-trap */ }
pub struct IrBlock { pub entry_addr: u64, pub ops: Vec<IrOp>, pub exits: Vec<BlockExit> }

pub struct WasmModule { pub bytes: Vec<u8> }   // deterministic build of one IrBlock

// ---- Memory ----
pub struct MmuState { pub ttbr0: u64, pub ttbr1: u64, pub tcr: u64, pub sctlr: u64 }
pub enum MemFault { TranslationFault { va: u64 }, PermissionFault { va: u64 } }

// ---- Interrupts ----
pub struct IrqState { /* GIC redistributor + timer regs, values only */ }
pub struct Irq { pub num: u32 }

// ---- Devices ----
pub enum DevEvent {
    QueueNotify { queue_idx: u16 },
    ConfigWrite { offset: u64, value: u64 },
    ConfigRead  { offset: u64 },           // returns u64 via DevOut
    Reset,
}
pub enum DevOut {
    ConfigValue(u64),
    UsedRingUpdate { queue_idx: u16 },
    IrqAssert { num: u32 },
    GpuCommands(Vec<GpuCmd>),              // virtio-gpu device se GPU half tak
    NetPacket(Vec<u8>),                    // virtio-net se net adapter tak
}

// ---- GPU command stream (guest → host; virtio_gpu_bridge isi ko decode karta hai) ----
pub enum GpuCmd {
    Transfer2D { resource_id: u32, x: u32, y: u32, w: u32, h: u32, data: Vec<u8> },
    Submit3D   { ctx_id: u32, commands: Vec<u8> },   // GLES command buffer bytes
    Scanout    { resource_id: u32, w: u32, h: u32 },
    Fence      { id: u64 },
}

// ---- Machine / snapshot ----
pub struct MachineState {
    pub cpu: Vec<CpuState>,   // per-vCPU regs, incl. PC/SP/PSTATE
    pub mmu: MmuState,
    pub irq: IrqState,
    pub ram: Vec<u8>,         // guest physical RAM, max 2 GiB
    pub devices: Vec<DeviceState>,
}
pub struct Snapshot(pub Vec<u8>);   // opaque, versioned blob

// ---- APK pipeline ----
pub struct ApkMeta { pub package: String, pub engine: EngineKind, pub gles_version: (u8, u8) }
pub enum EngineKind { Unity, Unreal, Godot, Other }
```

---

## Units

### U1 — `aarch64-decode` · NEW · PURE
- Responsibility: 32-bit word → decoded AArch64 instruction. Sirf decode, koi execute nahi.
- In: `word: u32` · Out: `DecodeResult`
- Purity: PURE. `pub fn decode(word: u32) -> DecodeResult`
- Dependencies: none.
- Acceptance: official ARM encoding spot-checks; `decode(0xFFFFFFFF) == Illegal`. Fuzz: random words kabhi panic nahi.

### U2 — `arm-ir-lift` · NEW · PURE
- Responsibility: decoded instruction → IR ops (single static assignment style).
- In: `&Instruction` · Out: `Vec<IrOp>`
- Purity: PURE. `pub fn lift(insn: &Instruction) -> Vec<IrOp>`
- Dependencies: U1 ka `Instruction` type (contract only).
- Acceptance: har supported opcode ke liye golden IR snapshot test; unsupported → explicit `IrOp::Trap`.

### U3 — `wasm-jit` · NEW · PURE
- Responsibility: IR block → deterministic WASM module bytes.
- In: `&IrBlock` · Out: `WasmModule`
- Purity: PURE. `pub fn compile(block: &IrBlock) -> WasmModule` — same input, byte-identical output.
- Dependencies: U2 ka `IrBlock` (contract only).
- Acceptance: compiled block ko WASM runtime mein chalakar golden output match; determinism test (do baar compile → identical bytes).
- Cross-unit convention (U2→U3, recorded 2026-09-27): U2 materializes immediates via `Mov{dst: 32}` + `Add{b: 32}` — register index **32 is a reserved scratch slot**, never architectural. U3/the WASM runtime MUST provide ≥33 i64 locals; index 32 must not alias a guest register. Register-relative LDR/STR lift to `Trap` (frozen `IrOp::Store.addr` is static); dynamic-address memory ops need a contract amendment, not silent emission.

### U4 — `mmu` · NEW · EXPLICIT-STATE
- Responsibility: virtual → physical address translation (AArch64 4-level tables).
- In: `(&MmuState, ram: &[u8], va: u64, access: Access)` · Out: `Result<u64 /*pa*/, MemFault>`
- Purity: EXPLICIT-STATE — page tables `ram` mein hain, regs `MmuState` mein; kuch chhupa nahi.
- Dependencies: none (sirf `MmuState` schema).
- Acceptance: hand-built page tables par translation golden tests; fault cases exact `MemFault` variant.

### U5 — `gic-timer` · NEW · EXPLICIT-STATE
- Responsibility: interrupt controller + generic timer tick.
- In: `(&IrqState, elapsed_cycles: u64)` · Out: `(IrqState, Vec<Irq>)`
- Purity: EXPLICIT-STATE.
- Acceptance: timer compare match par exact IRQ number assert; no spurious IRQs in soak test.
- Cross-unit convention (recorded 2026-09-27): timer interrupt = **INTID 27** (ARM virtual-timer PPI, SBSA) as owned unit constant `TIMER_IRQ_NUM`; `enabled` bit 0 mirrors CNTV_CTL_EL0.ENABLE; line is **level-triggered** — at most one `Irq` per tick on the rising edge, guest re-arms by writing a new `timer_compare`. Orchestrator (Wave 4) must route INTID 27 to the vCPU IRQ line.

### U6 — `virtio-transport` · PARTIAL (Path E: PCI config + queues REAL) · EXPLICIT-STATE
- Responsibility: virtqueue ring parse, descriptor chains, used-ring update, config space.
- In: `(state: &TransportState, ev: DevEvent, ram: &mut [u8])` · Out: `(TransportState, Vec<DevOut>)`
- Purity: EXPLICIT-STATE.
- Dependencies: `DevEvent`/`DevOut` schemas.
- Acceptance: Path E ke 311 tests yahan move honge; plus malformed descriptor chain → graceful error, kabhi OOB read nahi.

### U7 — `virtio-gpu-device` · PARTIAL (Path E: 2D decode REAL, 3D rejected) · EXPLICIT-STATE
- Responsibility: virtio-gpu 3D command stream → typed `GpuCmd` events.
- In: `(state: &GpuDevState, ev: DevEvent, ram: &mut [u8])` · Out: `(GpuDevState, Vec<DevOut::GpuCommands>)`
- Purity: EXPLICIT-STATE.
- Dependencies: U6 transport (via `DevEvent`), `GpuCmd` schema.
- Acceptance: **Path E ka ulta** — `Submit3D` ab accept hota hai aur sahi `GpuCmd::Submit3D`
  banta hai (pehle test "rejected" assert karta tha; woh test ab flip hoga). Fence ordering preserved.
- Cross-unit convention (recorded 2026-09-27): frozen `DevOut` has no error variant, so U7 signals decode failures as `DevOut::ConfigValue` with class tags — `ERR_UNKNOWN_COMMAND (0x4750_5500_0000_0000) | cmd_id` and `ERR_TRUNCATED (0x4750_5501_0000_0000) | needed_len`. Orchestrator (Wave 4) must check for these tag bits before treating a ConfigValue as a real config read.

### U8 — `gpu-host-stack` · EXISTS (bridge+gles2wgpu+compositor+swapchain) · PURE-ish
- Responsibility: `GpuCmd` stream → WebGPU canvas frame. GLES→WGSL translate, layer composite, present.
- In: `&[GpuCmd]`, frame params · Out: presented frame (via adapter, U13)
- Purity: PURE core (translate/composite functions) + thin QUARANTINED present call.
- Dependencies: `GpuCmd` schema; WebGPU surface sirf U13 adapter ke through.
- Acceptance: existing tests green rehte hain + **naya** end-to-end: synthetic `Submit3D`
  (triangle) → canvas pixel golden test. Ye woh wiring hai jo Path E mein kabhi hui nahi.

### U9 — `guest-image` · PARTIAL (kernel REAL, system.img STUB) · build-time unit
- Responsibility: AOSP arm64 source/config → bootable `system.img` + `vendor.img` + patched HALs.
- In: build config (manifest revision, patch set, feature flags) · Out: versioned image artifacts + SBOM
- Purity: build determinism — same config → bit-identical images (reproducible builds).
- Dependencies: HAL patches (U9 ke andar; C++ jo Path E mein compile nahi hua tha).
- Acceptance: `verify-image` script: kernel boots, init reaches launcher, images reproducible
  (do builds → identical hash). Path E ka `verify-runtime-artifacts.mjs` isi ka ancestor hai.

### U10 — `apk-pipeline` · EXISTS (analyzer) + NEW (sideload)
- Responsibility: APK bytes → `ApkMeta` (engine detect) → guest mein install → launch intent.
- In: `apk_bytes: &[u8]` · Out: `ApkMeta`; install: `(ApkMeta, AdbChannel) -> InstallResult`
- Purity: analyzer PURE (`pub fn analyze(apk: &[u8]) -> Result<ApkMeta, ApkError>`); sideload
  QUARANTINED (adb protocol I/O sirf U13 ke socket adapter par).
- Dependencies: `ApkMeta` schema.
- Acceptance: fixtures (`unity_cube.apk`, `godot_gles2.apk`) par engine detection golden;
  sideload ka mock-adb test.

### U11 — `snapshot` · NEW · PURE
- Responsibility: `MachineState` ↔ versioned blob.
- In: `&MachineState` → `Snapshot`; `&[u8]` → `Result<MachineState, SnapshotError>`
- Purity: PURE serde. `pub fn snapshot(s: &MachineState) -> Snapshot`, `pub fn restore(b: &[u8]) -> ...`
- Dependencies: `MachineState` schema.
- Acceptance: round-trip property test (random states → restore → identical); corrupt blob →
  typed error, kabhi panic nahi; version mismatch → explicit `VersionMismatch`, silent load nahi.

### U12 — `orchestrator` · NEW · QUARANTINED (the single impure coordinator)
- Responsibility: **eklauta** unit jisko threads, wall-clock, aur event loop chalane ki ijazat hai.
  vCPU threads spawn karta hai, `DevEvent`s route karta hai, explicit states ko thread karta hai.
- Rule: orchestrator koi business logic nahi rakhta — sirf state threading + scheduling.
  Agar logic yahan dikhe to woh kisi unit mein hona chahiye tha (code review red flag).
- Dependencies: sab units — par sirf unke frozen contracts ke through.
- Acceptance: boot-to-shell integration test; determinism test: same snapshot + scripted inputs →
  same state hash (wall-clock inject kiya jata hai, real time nahi).

### U13 — `browser-adapters` · NEW · QUARANTINED (externals boundary)
- Responsibility: bahar ki duniya se baat — WebGPU surface, WebSocket (net/adb), input events,
  storage (snapshot persist). **Saari side-effects yahin quarantine.**
- Contract per adapter (example):
  - `GpuSurface`: `present(frame: &Frame) -> ()` — WebGPU calls sirf yahan.
  - `NetSocket`: `send(&[u8])`, `recv() -> Option<Vec<u8>>` — virtio-net/adb isi par.
  - `InputSource`: `poll() -> Vec<NormalizedInput>` — sanitize + normalize yahin, raw DOM events andar nahi aate.
  - `BlobStore`: `save(key, &[u8])`, `load(key) -> Option<Vec<u8>>` — snapshot persist.
- Rule: adapters ke andar koi emulation logic nahi; emulation units adapters ko kabhi directly
  call nahi karte — orchestrator beech mein hai.
- Acceptance: har adapter ka mock test; real-browser smoke test (headless Chrome).

### U14 — `metrics` · EXISTS (`metrics_overlay`) · PURE core
- Responsibility: FPS, frame time, draw calls, guest RAM — HUD overlay.
- In: event stream (`FrameCompleted { dt_ms, draw_calls }`) · Out: aggregated stats + overlay draw cmds
- Purity: aggregation PURE; draw QUARANTINED via U13 surface.
- Acceptance: synthetic event stream par stats golden values.

---

## Cross-unit rules (swarm law)

1. Koi unit doosre unit ka module import nahi karega. Shared sirf ye file ke schemas.
2. Har unit apna test suite khud ship karega; cross-unit tests sirf contract schemas par.
3. Jahan determinism possible hai, wahan property test mandatory (U1 fuzz, U3 byte-identical, U11 round-trip).
4. Impurity sirf U12 aur U13 mein. Kisi aur unit mein `Date::now()`, thread spawn, ya I/O dikha
   to review fail.
5. Contract change = version bump + swarm-wide announcement. Silent change nahi.
