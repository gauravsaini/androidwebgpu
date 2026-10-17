# Gates: Track T563 — dfr0-align

OWNS: units/u2-ir-lift/src/lib.rs, units/u12-orchestrator/tests/sysreg_p3.rs, units/u12-orchestrator/tests/kernel_boot_attempt.rs, GATES-T563.md, TIMELINE.md, /Users/Shared/agyx-track563-dfr0-align.md

Scope: Align ID_AA64DFR0_EL1 to QEMU Cortex-A53 value 0x10305106, add QEMU-matched regression tests, evaluate first-million aligned trace divergence vs pinned QEMU trace, and record findings.

- [x] G1: Base commit verified 1af87e805ab3335f1e744be96fe7465d32584799 on branch t563-dfr0-align.
  CHECK: git rev-parse HEAD~1
  EXPECT: 1af87e805ab3335f1e744be96fe7465d32584799
  EVIDENCE: Branch t563-dfr0-align created from base 1af87e805ab3335f1e744be96fe7465d32584799; commit fad1005381e0c8cbad07ceb07f23c0bfe649a652.

- [x] G2: ID_AA64DFR0_EL1 mapped to 0x10305106 in units/u2-ir-lift/src/lib.rs.
  EVIDENCE: units/u2-ir-lift/src/lib.rs line 501 updated to (3, 0, 0, 5, 0) => 0x1030_5106.

- [x] G3: QEMU-matched regression tests pass in sysreg_p3 and u2-ir-lift.
  EVIDENCE: sysreg_p3 passed 8/8 tests (including p3_dfr0_el1_matches_cortex_a53_qemu, p3_mmfr0_parange_mrs_to_tcr_matches_pinned_qemu, p3_ctr_el0_matches_cortex_a53_qemu); u2-ir-lift passed 129/129 unit tests.

- [x] G4: Clean build in separate CARGO_TARGET_DIR target-t563.
  EVIDENCE: Built target-t563/release/run_kernel without touching target/ (SHA-256 ae7829802c9e5706311d0e3069c4408854c8ef4609e30258daa27c3aa647b516).

- [x] G5: First-million aligned diff evaluated against pinned QEMU trace.
  EVIDENCE: DFR0 divergence at Rust step 7,464 / QEMU step 7,470 completely removed. Steps 7,464..7,475 match QEMU. Next divergence identified at Rust step 7,578 / QEMU step 7,584 (MRS X3, DCZID_EL0: Rust 0x10 [DZP=1] vs QEMU 0x04 [DZP=0]), causing control flow branch fork at Rust step 7,579 / QEMU step 7,585 (TBNZ W3, #4).

- [x] G6: Findings file /Users/Shared/agyx-track563-dfr0-align.md delivered on disk.
  EVIDENCE: Delivered /Users/Shared/agyx-track563-dfr0-align.md with Verdict, Knowledge Artifact (CLAIM, SCOPE, WITNESS), exact code change, regression results, diff results, binary revision, WALL-TYPE-TAG, and "Rust par bhi lagu" line.
