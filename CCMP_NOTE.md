# CCMP/CCMN Implementation Note

**Date:** 2026-10-03
**Worktree:** ~/workspace/wt-ccmp
**Branch:** feat/ccmp
**Base:** 7a9598c

## Summary

Implemented CCMP (conditional compare) and CCMN (conditional compare negative)
in the u12-orchestrator fast-path interpreter (`execute_arm64`).

## The Halt

The 50M-step boot run reached **20,458,502 steps** before halting at:
- PC: `0xffffff800808a1f4`
- Word: `0x7A442060` = `ccmp w3, #4, #0, cs`
- Reason: `IllegalInstruction`

## Encoding

CCMP/CCMN: `sf op S 11010010 imm5/Rm cond 0 o0 Rn nzcv`
- `bits[30:21] == 0x3D2` (op=1: CCMP) or `0x1D2` (op=0: CCMN)
- `bit[10] (o0)`: 0 = immediate, 1 = register
- `bits[15:12]`: condition code
- `bits[3:0]`: nzcv immediate (used when condition is false)

**Correction during implementation:** Initially had the opcodes swapped
(0x1D2/0xD2). The correct values are 0x3D2 (CCMP) and 0x1D2 (CCMN),
verified against the actual halt word 0x7A442060.

## Implementation

Location: `units/u12-orchestrator/src/lib.rs`, section "7b. Conditional compare"

Executed directly in the fast-path interpreter (like CSEL), not via U2/U3:
- The condition is evaluated against live NZCV flags from `pstate`
- If condition holds: NZCV = SUBS (CCMP) or ADDS (CCMN) of Rn and operand
- If condition fails: NZCV = nzcv_imm (4-bit immediate)
- Uses existing helpers: `condition_holds`, `nzcv_sub64/sub32`, `nzcv_add64/add32`
- Rn=31 reads as XZR (not SP) for CCMP/CCMN
- Rm=31 reads as XZR for register variant

## Tests

5 tests in `units/u12-orchestrator/src/lib.rs`:
- `ccmp_imm_cond_true_sets_subs_flags`: CCMP W3,#4,#0,CS with C set → SUBS flags
- `ccmp_imm_cond_false_sets_nzcv_imm`: Same with C clear → NZCV=0
- `ccmn_imm_cond_true_sets_adds_flags`: CCMN W1,#5,#0,EQ with Z set → ADDS flags
- `ccmp_reg_cond_true`: CCMP X2,X3,#0,NE register variant → SUBS flags
- `gb26_ccmp_rn31_is_xzr`: **Pre-existing test** (was already in codebase, now passes)

Full u12-orchestrator suite: **113 passed**, 5 RAM-heavy skipped (standard).

## Boot Re-measurement

| Metric | Before | After |
|--------|--------|-------|
| Steps | 20,458,502 | **20,473,766** (+15,264) |
| Halt PC | 0xffffff800808a1f4 | 0xffffff8008c736cc |
| Halt word | 0x7A442060 (CCMP) | 0x6B21011F |
| Halt reason | IllegalInstruction | DataProc: unsupported encoding |

**Decoded new halt:** `0x6B21011F` = `subs wzr, w8, w1, uxtb` (32-bit SUBS
with unsigned byte extend, bit21=1 extended register form). This is a
different instruction family (extended ADD/SUB) — recommended as the next
track.

## Files Changed

- `units/u12-orchestrator/src/lib.rs`: +119 lines (CCMP/CCMN handler + 4 tests)
- `CCMP_NOTE.md`: this file

## Status

Ready for reviewer merge. Not pushed (per instructions).
