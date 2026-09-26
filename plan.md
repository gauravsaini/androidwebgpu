# Plan — Path N execution run

Date: 2026-09-26. Driver: Muse (autonomous). Discipline: unlazy (`docs/unlazy/SKILL.md`).
Branch: `feat/native-arm-vision`. Language: Rust (preferred).
(Path E's original plan preserved at `docs/path-e/plan.md`.)

## Objective

Implement and verify Path N **Wave 0 + Wave 1 + Wave 2** under unlazy gates.
Judged on final output — "Done" below is the rubric.

## Done (final output)

- **Wave 0** — `contracts/` crate: build green, tests green, clippy clean,
  LLD crossref green, gate-lint green. Parent-verified.
- **Wave 1** — 6 pure units, each implemented + own gates green + parent re-verified:
  - 2.1 U1 `aarch64-decode` — `decode(u32) -> DecodeResult`, major opcode classes, fuzz no-panic
  - 2.2 U2 `arm-ir-lift` — `lift(&Instruction) -> Vec<IrOp>`, golden snapshots, unsupported → Trap
  - 2.3 U3 `wasm-jit` — `compile(&IrBlock) -> WasmModule`, valid WASM bytes, byte-identical determinism
  - 2.4 U11 `snapshot` — `snapshot`/`restore` round-trip, versioned format, corrupt → typed error
  - 2.5 U10 `analyzer-conformance` — existing `crates/apk_gpu_analyzer` mapped to contract types, no rewrite
  - 2.6 U14 `metrics-core` — pure aggregation isolated + tested; draw stays quarantined
- **Wave 2** — 5 explicit-state units, same bar:
  - 3.1 U4 `mmu` — page-table walk, golden translations, exact fault variants
  - 3.2 U5 `gic-timer` — tick → exact IRQ numbers, no spurious IRQs
  - 3.3 U6 `virtio-transport` — queue/descriptor handling, malformed input → graceful error
  - 3.4 U7 `gpu-device` — virtio-gpu stream → typed `GpuCmd`; **Submit3D accepted** (Path E flipped)
  - 3.5 U8 `gpu-host-wiring` — `GpuCmd` → existing bridge/gles2wgpu/compositor path wired; triangle golden test
- Every leaf: unlazy 4 passes (implement → expert re-read → defect hunt → polish).
- Final report: measured met / unmet / abandoned counts per leaf, re-measured before reporting.

## Non-goals (this run)

- Wave 3 (AOSP guest image — multi-hour build), Wave 4 (orchestrator), Unicorn spike.
- No changes to Path E code except additive reuse.

## Execution order

1. Toolchain verify (rustup) + Wave 0 gates → parent verify.
2. Wave 1: author 6 leaf gate ledgers → dispatch 6 leaf agents in one wave (parallel).
3. Parent re-verify each leaf (re-run gates, not just status) → node-1 branch integration.
4. Wave 2: same pattern, 5 leaves → node-2.
5. Final audit: re-read request, re-measure every number, report with qualified ids.
6. Push branch `feat/native-arm-vision`.

## Autonomy rules (Gaurav's standing order, 2026-09-26)

- All questions answered autonomously. No mid-run approval prompts.
- "Start executing" counts as approval for Wave 0–2 CHECK oracles listed in gate ledgers.
- A gate that is genuinely impossible gets `ABANDON: <reason>` + handoff — never silent skip,
  never fake green.
- `.unlazy/PLAN.md` dispatch table + `status.log` updated on every state change.
- Stop hook: not installed (optional per skill; skipped by driver decision).
