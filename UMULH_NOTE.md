# UMULH/SMULH Implementation Note

**Date:** 2026-10-03  
**Worktree:** `~/workspace/wt-umulh`, branch `feat/umulh` (based at `595c5ed`)  
**Status:** Implemented, tests green. NOT pushed, NOT merged (reviewer gate merges).

## Problem

Kernel boot halted at 57,995 steps, PC `0xffffff8008217f40`, instruction `0x9BC97D08`
(`umulh x8, x8, x9`), with "DataProc: unsupported encoding".

`UMULH Xd, Xn, Xm`: `Xd = ((Xn as u128 * Xm as u128) >> 64) as u64`
(128-bit unsigned product, high 64 bits).

## Encoding (verified via capstone)

- UMULH: `sf=1, op54=00, op31=110, o0=0, Rm, Ra=XZR(31), Rn, Rd`
  - Example: `0x9BC97D08` = `umulh x8, x8, x9`
- SMULH: `sf=1, op54=00, op31=010, o0=0, Rm, Ra=XZR(31), Rn, Rd`
  - Example: `0x9B497D08` = `smulh x8, x8, x9`
- Both require `sf=1` (64-bit only) and `Ra=31`; otherwise trap.

## Implementation

### 1. New IrOps (`contracts/src/cpu.rs`)

```rust
Umulh { dst: u8, n: u8, m: u8 },  // dst = ((n as u128 * m as u128) >> 64) as u64
Smulh { dst: u8, n: u8, m: u8 },  // dst = ((n as i128 * m as i128) >> 64) as u64
```

### 2. Lifter (`units/u2-ir-lift/src/lib.rs`)

Added before the `R_DP_UNSUPPORTED` trap in the 3-source handler:
- Matches `op54==0 && o0==0 && (op31==0b110 || op31==0b010)`
- Validates `sf==1` and `Ra==31`, else traps
- Emits `IrOp::Umulh` or `IrOp::Smulh`

### 3. WASM backend (`units/u3-wasm-jit/src/lib.rs`)

**UMULH** via 32-bit split (Hacker's Delight):
```
a_lo = a & 0xFFFFFFFF; a_hi = a >> 32  (same for b)
p00 = a_lo*b_lo; p01 = a_lo*b_hi; p10 = a_hi*b_lo; p11 = a_hi*b_hi
sum = p01 + p10; ov = (sum < p01)  // unsigned overflow check
hi = p11 + (sum >> 32) + (ov ? 2^32 : 0)
t = (sum & 0xFFFFFFFF) << 32
carry = ((t + p00) < t)
result = hi + carry
```

Uses 10 i64 locals (local 0 = SCRATCH, locals 1-10 = temporaries).
**Critical bug found during testing:** `OP_I64_LT_U` was defined as `0x53`
(which is `i64.lt_s`, signed!). Correct is `0x54` (`i64.lt_u`, unsigned).
The signed comparison gave wrong carry for large values (off by one).

**SMULH** via identity:
```
smulh(a,b) = umulh(a,b) - (a<0 ? b : 0) - (b<0 ? a : 0)
```
Implemented as: compute umulh, then subtract `(a>>63)&b` and `(b>>63)&a`
(arithmetic shift creates all-1s mask for negatives).

### 4. Local count change

WASM function now declares 12 i64 locals (was 1):
- Local 0: SCRATCH (unchanged)
- Locals 1-10: UMULH/SMULH temporaries
- Local 11: unused (alignment)

Updated `wave4_structural_one_scratch_local` test to expect 12.

## Tests

| Suite | Result |
|-------|--------|
| `u2-ir-lift` (lift shape) | 120 passed (incl. `umulh_lift`, `smulh_lift`) |
| `u3-wasm-jit` | 39 passed (incl. updated structural test) |
| `u15-exec-wasmi` (value) | 11 passed (incl. `backend_umulh_values`, `backend_smulh_values`) |

Value tests compare against Rust `u128`/`i128` across 8 cases each,
including `u64::MAX * u64::MAX` and mixed sign cases.

## Boot re-measurement

| | Before | After |
|---|---|---|
| Steps | 57,995 | **58,603** (+608) |
| Halt PC | `0xffffff8008217f40` | `0xffffff800821a430` |
| Reason | `DataProc: unsupported` (UMULH) | `DataProc: unsupported` (UMULL) |
| Word | `0x9BC97D08` | `0x9BAA7D29` = `umull x9, w9, w10` |

**UMULH executed successfully in the boot** (step 58595: `0x9BCA7D08` = `umulh x8, x8, x10`
at `0xffffff800821a410` ran without halt). The fix works.

## Next blocker

`UMULL Xd, Wn, Wm` (0x9BAA7D29): `Xd = (Wn as u64) * (Wm as u64)` 
(32-bit unsigned multiply to 64-bit). Encoding: `sf=1, op54=00, op31=101`.
SMULL (signed) likely nearby. These are simpler than UMULH (no 128-bit needed).
