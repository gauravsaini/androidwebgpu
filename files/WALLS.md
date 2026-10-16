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

