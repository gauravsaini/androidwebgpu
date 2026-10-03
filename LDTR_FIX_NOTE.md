# LDTR Fix Note (actually: SXTW register-offset)

**Date:** 2026-10-03  
**Worktree:** `~/workspace/wt-ldtr`, branch `feat/ldtr-unprivileged` (base `2a01951`)  
**Status:** Implemented, tests green, boot re-measured. NOT pushed (reviewer gate merges).

## The misidentification

The parent task identified the 50,953-step halt instruction (`0xF869D949` at
`0xffffff8008217d80`) as **LDTR X9, [X10, #imm]** (unprivileged load).
Capstone disassembly proves this wrong:

```
0xF869D949 → ldr x9, [x10, w9, sxtw #3]
```

It is a **register-offset LDR** with the **SXTW** extend option — not an
unprivileged immediate load. Fields: `size=0b11`, `opc=0b01`, `Rm=x9`,
`option=0b110` (SXTW), `S=1` (shift), `Rn=x10`, `Rt=x9`, shift amount `#3`.

## Root cause

`units/u2-ir-lift/src/lib.rs`, `lift_load_store`, section 5 (register offset),
only accepted `option == 0b011` (LSL) and `option == 0b010` (UXTW).
`option == 0b110` (SXTW) fell through to `trap(R_LS_UNSUPPORTED)`.

## Fix

Extended the option check to `0b011 | 0b010 | 0b110 | 0b111`:
- **SXTW (0b110):** sign-extend Wm to 64-bit into SCRATCH via
  `Wm & Wm` (32-bit zero-extend) → `LSL #32` → `ASR #32`,
  then optional `LSL #shift_amt`, then `ADD Rn`.
- **SXTX (0b111):** 64-bit sign-extend is a no-op; falls through to the
  existing LSL path (same as `0b011`).

No changes to the atomic handler (checked first) or the immediate
pre/post-indexed handler (bit21==0 gate untouched).

## Tests

- New `ldr_reg_offset_sxtw`: lifts `0xF869D949` and asserts the exact
  6-op sequence (AndShift → OrrShift#32 → OrrShift#32-ASR → OrrShift#3 →
  Add → LoadDyn).
- Existing `gb1_ldr_reg_offset` (LSL) still passes — no regression.
- Full `u2-ir-lift` suite: **118 passed, 0 failed**.

## Boot re-measurement

| | Before (2a01951) | After (this fix) |
|---|---|---|
| Steps executed | 50,953 | **57,995** |
| Halt PC | `0xffffff8008217d80` | `0xffffff8008217f40` |
| Halt reason | `LoadStore: unsupported encoding` | **`DataProc: unsupported encoding`** |
| Faulting word | `0xF869D949` | `0x9BC97D08` (`umulh x8, x8, x9`) |

**The SXTW fix moved the halt forward by 7,042 steps.** The new blocker is
`UMULH X8, X8, X9` (Unsigned Multiply High, `Xd = (Xn*Xm)[127:64]`) —
a data-processing instruction family not yet in the lifter. Implementing
128-bit multiply-high is a separate track.

## Files changed

- `units/u2-ir-lift/src/lib.rs`: SXTW/SXTX in register-offset handler (+1 test).

## Commit

`feat/ldtr-unprivileged` — conventional commit, not pushed.
