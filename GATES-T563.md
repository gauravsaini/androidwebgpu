# Gates: Track T563 — dfr0-align

OWNS: units/u2-ir-lift/src/lib.rs, units/u12-orchestrator/tests/sysreg_p3.rs, units/u12-orchestrator/tests/kernel_boot_attempt.rs, GATES-T563.md, TIMELINE.md, /Users/Shared/agyx-track563-dfr0-align.md

Scope: Align ID_AA64DFR0_EL1 to QEMU Cortex-A53 value 0x10305106, add QEMU-matched regression tests, evaluate first-million aligned trace divergence vs pinned QEMU trace, and record findings.

- [x] G1: Base commit verified 1af87e805ab3335f1e744be96fe7465d32584799 on branch t563-dfr0-align.
  CHECK: git rev-parse HEAD
  EXPECT: 1af87e805ab3335f1e744be96fe7465d32584799
  EVIDENCE: git rev-parse HEAD returned 1af87e805ab3335f1e744be96fe7465d32584799

- [ ] G2: ID_AA64DFR0_EL1 mapped to 0x10305106 in units/u2-ir-lift/src/lib.rs.
  EVIDENCE: pending

- [ ] G3: QEMU-matched regression tests pass in sysreg_p3.
  EVIDENCE: pending

- [ ] G4: Clean build in separate CARGO_TARGET_DIR target-t563.
  EVIDENCE: pending

- [ ] G5: First-million aligned diff evaluated against pinned QEMU trace.
  EVIDENCE: pending

- [ ] G6: Findings file /Users/Shared/agyx-track563-dfr0-align.md delivered on disk.
  EVIDENCE: pending
