# Walls

## T499 live CSINV state divergence — engine

- Tag: **engine** (Rust emulator execution state).
- Pinned witness: `MANIFEST.json` input hashes and QEMU/Rust revision; [`qemu-csinv-live-hit2.log`](../evidence/t499/qemu-csinv-live-hit2.log) versus [`rust-csinv-live-hit2.log`](../evidence/t499/rust-csinv-live-hit2.log).
- Observation: both live paths reach Image word `0xda818058` at guest VA `0xffffff8008354018`; QEMU enters with PSTATE `0xa00001c5`, Rust with `0x60000000`, and X24 differs after the instruction.
- Scope: one manifest-bound live capture. This tag records the observed engine wall; TEN-PASSES was not performed, so no reproducibility claim is made.
- Control: [`qemu-csinv-micro.log`](../evidence/t499/qemu-csinv-micro.log) and [`rust-csinv-micro.log`](../evidence/t499/rust-csinv-micro.log) show equal result for matched input state.

## Setup observation not classified as a wall

`evidence/t499/qemu-gicv3-mismatch.log` records an initial QEMU GICv3 run that conflicted with the supplied DTB's GICv2 controller and stopped before `/init`. That configuration was outside the pinned manifest and is not tagged as a guest, WASM, or engine wall.
