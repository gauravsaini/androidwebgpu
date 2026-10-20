# Walls

## T499 live CSINV state divergence — engine

- Tag: **engine** (Rust emulator execution state).
- Pinned witness: `MANIFEST.json` input hashes and QEMU/Rust revision; [`qemu-csinv-live-hit2.log`](../evidence/t499/qemu-csinv-live-hit2.log) versus [`rust-csinv-live-hit2.log`](../evidence/t499/rust-csinv-live-hit2.log).
- Observation: both live paths reach Image word `0xda818058` at guest VA `0xffffff8008354018`; QEMU enters with PSTATE `0xa00001c5`, Rust with `0x60000000`, and X24 differs after the instruction.
- Scope: one manifest-bound live capture. This tag records the observed engine wall; TEN-PASSES was not performed, so no reproducibility claim is made.
- Control: [`qemu-csinv-micro.log`](../evidence/t499/qemu-csinv-micro.log) and [`rust-csinv-micro.log`](../evidence/t499/rust-csinv-micro.log) show equal result for matched input state.

## Setup observation not classified as a wall

`evidence/t499/qemu-gicv3-mismatch.log` records an initial QEMU GICv3 run that conflicted with the supplied DTB's GICv2 controller and stopped before `/init`. That configuration was outside the pinned manifest and is not tagged as a guest, WASM, or engine wall.

## T535 GICv2 MMIO unhandled fault (45.64M arch_timer wall) — engine

- Tag: **engine** (type-3 wall: missing device model / MMIO handler).
- Step: `45,636,759` (within the `45,576,164` – `45,844,254` post-`NR_IRQS` window).
- PC: `0xffffff8008621324` (`gic_init_bases` + 0x34).
- Instruction: `ldr w8, [x8]` (opcode: `0xb9400508`), where `x8` points to VA `0xffffff8008010004` (PA `0x08000004`, `GICD_TYPER`).
- Fault: `HaltReason::WasmTrap { addr: 0xffffff8008621324, message: "mem_load fault: TranslationFault { va: 134217732 }" }`.
- Root Cause: Missing GICv2 distributor (`0x08000000`) and CPU interface (`0x08010000`) MMIO handling in `WasmHost::mem_load`/`mem_store`. Address `0x08000004` falls outside RAM (`0x40000000..0x80000000`), triggering an unhandled `TranslationFault`.
- Oracle: QEMU returns `0x00000008` for `GICD_TYPER` and cleanly configures GIC distributor and CPU interface, enabling virtual timer PPI 27 (`0x08000000` into `GICD_ISENABLER0`) at step `45,834,542` and reaching `arch_timer` probe at step `45,844,254`.

## T585/T588 netd-updatable-crashing / zygote-clean-exit-respawn-loop — guest

- Tag: **guest** (Type-1 guest image wall).
- Rust par bhi lagu: haan (applies to both QEMU virt and Rust emulator Android guest userspace).
- Pinned witness: `/Users/Shared/agyx-track585-sf-fabricate-display.md` lines 16868, 17108, 17219, 17280, 17477 (`init: process with updatable components 'netd' exited 4 times before boot completed`), lines 16929, 17488 (`init: Service 'zygote' (pid ...) exited with status 0`).
- Observation: In headless virt guest without physical network / eBPF routing interfaces, `netd` fails to initialize and crash-loops with exit status 1. Once `netd` exits 4 times, Android init sets `sys.init.updatable_crashing=1` and initiates a userspace daemon reset cascade. Simultaneously, Zygote exits cleanly with status 0, triggering its `onrestart` stanza and bouncing core daemons (`surfaceflinger`, `audioserver`, `cameraserver`, `media`), while `vendor.camera-provider-ranchu` exits with status 1 (no camera hardware).
- T588 Status: Hard gate NO-MISSING-INPUTS triggered. Base tree `/Users/Shared/wt-track585-sf-fabricate-display` was pruned during host disk-guard sweeps, so the display fabrication artifacts could not be directly inherited without re-derivation.



## T586 sbcs-carry-flag-unimplemented (49.69M wall) — engine [RESOLVED in T587]

- Tag: **engine** (DataProc ADC/SBC/ADCS/SBCS carry flag arithmetic in `u2-ir-lift`/`u12-orchestrator`).
- Rust par bhi lagu: haan (applies directly to the Rust engine fast-path interpreter and lifter).
- Step: `49,688,549`.
- PC: `0xffffff8008259154` (`lib/find_bit.c:__find_next_bit`).
- Instruction: `sbcs xzr, x10, x8` (`0xfa08015f`).
- Fault: `HaltReason::Unsupported { addr: 18446743524090417492, reason: "DataProc: ADC/SBC/ADCS/SBCS need NZCV carry flag, not expressible in IrOp" }`.
- Status: **RESOLVED in T587**. Full AArch64 architectural `AddWithCarry` flag propagation implemented for 32-bit and 64-bit ADC, SBC, ADCS, and SBCS in `units/u12-orchestrator/src/lib.rs` and `arm64.rs`. Verified against 16 QEMU-matched regression test vectors (`adc_sbc_regression.rs`). Boot cleanly advanced past step 49,688,549 to step 49,688,578.

## T587 TranslationFault va:0 / NULL dereference (49.69M wall) — engine

- Tag: **engine** (unhandled MMU synchronous exception / TranslationFault on zero address in `u12-orchestrator`).
- Rust par bhi lagu: haan (applies to Rust emulator MMU / exception vectoring).
- Step: `49,688,578` (29 steps past bypassed T586 wall at step 49,688,549).
- PC: `0xffffff80082591e4` (`lib/find_bit.c:__find_next_bit`).
- Instruction: `ldr x8, [x23, #1344]` (`0xf942a2e8`), where `x23 = 0x0` at halt.
- Fault: `HaltReason::WasmTrap { addr: 0, message: "mmu_fault: TranslationFault { va: 0 }" }`.
- Root Cause: In Linux ARM64 with CPU PAN emulation, `__find_next_bit` executes post-carry bit search and user access routines where a memory access with `x23=0` triggers a stage-1 Translation Fault on VA 0. In bare-metal ARM64 / QEMU, translation faults generate synchronous Data Abort exceptions vectoring to `VBAR_EL1 + 0x200` with `ESR_EL1` and `FAR_EL1` set, allowing Linux kernel exception fixup tables (`__ex_table`) to handle the fault gracefully. The Rust emulator currently treats non-vmemmap `TranslationFault` as an unhandled fatal `WasmTrap` rather than vectoring a synchronous Data Abort to `VBAR_EL1`.
- Console bytes: 3,936 bytes (clean, 0 dynamic panics in 1 GiB guest RAM scan).
