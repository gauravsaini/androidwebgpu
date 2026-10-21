# Gates: Track T594 — Rust engine: BRK exception vectoring to VBAR_EL1+0x200 (Chain B)

OWNS: units/u12-orchestrator/**, units/u2-ir-lift/**, contracts/**, GATES.md, TIMELINE.md, files/WALLS.md, outcome-t594.txt

Scope: Implement architectural BRK debug exception vectoring to VBAR_EL1+0x200 (el1_sync) and VBAR_EL1+0x400 (el0_sync) in u12-orchestrator/u2-ir-lift, verify QEMU regression vectors, execute owned cold boot gate past step 145,410,118.

- [x] G594-1: Base commit fb394dcf0807003e155b075a1e4d4de76dd90c0d verified in worktree
  CHECK: cd /Users/Shared/wt-track594-brk-vector && git rev-parse HEAD
  EXPECT: fb394dcf0807003e155b075a1e4d4de76dd90c0d
  EVIDENCE: verified 2026-10-21 `fb394dcf0807003e155b075a1e4d4de76dd90c0d`

- [x] G594-2: BRK decode, ESR encoding (EC=0x3c, IL=1, ISS=imm16), ELR/SPSR save, DAIF mask, and VBAR vectoring implemented
  CHECK: grep -q "take_brk_exception" /Users/Shared/wt-track594-brk-vector/units/u12-orchestrator/src/lib.rs && echo BRK_IMPLEMENTED
  EXPECT: BRK_IMPLEMENTED
  EVIDENCE: verified `take_brk_exception` implemented in `u12-orchestrator` and `IrOp::Brk { imm }` in `u2-ir-lift`

- [x] G594-3: QEMU-matched regression tests passing
  CHECK: CARGO_TARGET_DIR=target-t594 cargo test --release --test brk_vbar_regression -- --nocapture
  EXPECT: test result: ok. 5 passed
  CWD: /Users/Shared/wt-track594-brk-vector/units/u12-orchestrator
  EVIDENCE: verified 5/5 passed (EL1 imm=0x800 T592 wall, EL0 imm=0x1 to VBAR+0x400, imm=0x0, DAIF masking, ERET round-trip)

- [x] G594-4: Release build of run_kernel with CARGO_TARGET_DIR=target-t594
  CHECK: test -x /Users/Shared/wt-track594-brk-vector/target-t594/release/run_kernel && echo BUILD_OK
  EXPECT: BUILD_OK
  EVIDENCE: verified executable at `target-t594/release/run_kernel` (SHA-256 fe054e10f5ca0dbf361a2d13a5f588d04d8b8e0c002f6db754a058ab7b655557)

- [x] G594-5: Cold reset boot gate executed under owned supervisor past step 145,410,118 with log capture and heartbeat
  CHECK: test -f /Users/Shared/wt-track594-brk-vector/outcome-t594.txt && echo BOOT_GATE_RECORDED
  EXPECT: BOOT_GATE_RECORDED
  EVIDENCE: verified `outcome-t594.txt` recorded 150M completion (step 150,000,000, UART 13,456 bytes, 1,984 IRQs, 1 BRK exception vectored at step 145,410,118, 0 emulator traps)

- [x] G594-6: Findings artifact delivered with required tags, verdict, and registry updated
  CHECK: test -f /Users/Shared/agyx-track594-brk-vector.md && grep -q "WALL-TYPE-TAG: engine" /Users/Shared/agyx-track594-brk-vector.md && echo FINDINGS_OK
  EXPECT: FINDINGS_OK
  EVIDENCE: verified `/Users/Shared/agyx-track594-brk-vector.md` delivered with Verdict: PASS and WALL-TYPE-TAG: engine
