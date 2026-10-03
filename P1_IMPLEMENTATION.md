# P1 Implementation Summary

**Worktree:** `~/workspace/wt-emu-p1` (branch `feat/emu-p1-sysregs`)
**Date:** 2026-10-03
**Status:** Implementation complete, u12-orchestrator tests running.

## Finding: only 2 of 5 needed work

The task listed 5 registers, but code inspection showed 3 already had MRS:
- MAIR_EL1 (3,0,10,2,0), TCR_EL1 (3,0,2,0,2), TTBR0_EL1 (3,0,2,0,0) —
  MRS arms present in `lift_system` persistent match (GB-13/17/18).

Only 2 had the read-back asymmetry (MSR stored, MRS trapped with R_SYSTEM):
- **VBAR_EL1** (3,0,12,0,0) — S3_0_C12_C0_0
- **HCR_EL2** (3,4,1,1,0) — S3_4_C1_C1_0

## Changes (`units/u2-ir-lift/src/lib.rs` only)

No new SysReg variants, storage fields, or snapshot changes needed —
`SysReg::VbarEl1`/`HcrEl2`, `SysRegs.vbar_el1`/`hcr_el2`, read()/store() arms,
and u11-snapshot serialization all already existed (MSR path).

1. **MRS persistent match** (+2 arms after CONTEXTIDR_EL1):
   - `(3, 0, 12, 0, 0) => SysReg::VbarEl1`
   - `(3, 4, 1, 1, 0) => SysReg::HcrEl2`
2. **Test `gbsysreg2_mrs_msr_lift_to_persistent_ops`**: added both tuples
   to `mrs_cases` — verifies lift to `IrOp::ReadSys`.

## Test Results

- `u2-ir-lift`: 112 passed, 0 failed ✅ (includes updated sysreg test)
- `u12-orchestrator`: 113 passed, 0 failed ✅ (single-threaded, 870s —
  even the 3 previously OOM-prone snapshot tests passed this run)

## Semantics

Both registers read back the last MSR-written value (stored u64, default 0).
Honest behavior: the kernel programs these during boot and may read back to
verify; returning the stored value is architecturally correct. No behavior
attached (VBAR_EL1 doesn't change our trap vectors yet — no exception model;
HCR_EL2 doesn't change execution mode — we stay EL1).
