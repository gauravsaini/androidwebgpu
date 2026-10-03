# SMADDL/SMSUBL/UMADDL/UMSUBL Implementation Note

Date: 2026-10-03. Worktree: `~/workspace/wt-smaddl`, branch `feat/smaddl`, base `de05b5c`.

## Problem

Boot halted at 20,555,721 steps:
- PC: `0xffffff8008355394`
- Word: `0x9B2C2328` = `smaddl x8, w25, w12, x8` (capstone-verified)
- Reason: DataProc: unsupported encoding

## Encoding Analysis

`0x9B2C2328` decodes as:
- sf=1 (64-bit)
- op54=00 (ADD)
- bits[28:24]=11011 (multiply group)
- op31=001 (signed long)
- Rm=12 (w12), Ra=8 (x8, accumulate), Rn=25 (w25), Rd=8 (x8)

Semantics: `X8 = X8 + SignExtend(W25) * SignExtend(W12)`

The family:
- SMADDL: op54=00, op31=001, Ra≠31 — `Xd = Xa + sext(Wn) * sext(Wm)`
- SMSUBL: op54=01, op31=001, Ra≠31 — `Xd = Xa - sext(Wn) * sext(Wm)`
- UMADDL: op54=00, op31=101, Ra≠31 — `Xd = Xa + zext(Wn) * zext(Wm)`
- UMSUBL: op54=01, op31=101, Ra≠31 — `Xd = Xa - zext(Wn) * zext(Wm)`

Note: When Ra=31 (XZR), these degenerate to SMULL/UMULL (already implemented).
The previous UMULL/SMULL handler trapped on Ra≠31 with R_DP_UNSUPPORTED.

## Implementation

### 1. New IrOps (contracts/src/cpu.rs)
- `Smaddl { dst, n, m, a }`
- `Smsubl { dst, n, m, a }`
- `Umaddl { dst, n, m, a }`
- `Umsubl { dst, n, m, a }`

### 2. Lifter (units/u2-ir-lift/src/lib.rs)
Replaced the UMULL/SMULL handler with a unified long-multiply handler:
- Matches: `o0 == 0 && (op31 == 0b101 || op31 == 0b001) && (op54 == 0 || op54 == 1)`
- If Ra==31 and op54==0: emit Umull/Smull (existing behavior preserved)
- If Ra==31 and op54==1: trap R_MADD_LONG (SUB with XZR accumulate is unusual)
- If Ra≠31: emit Smaddl/Smsubl/Umaddl/Umsubl based on op31/op54

### 3. WASM Backend (units/u3-wasm-jit/src/lib.rs)
- `emit_smaddl`: sext both, i64.mul, i64.add with Xa
- `emit_smsubl`: Xa, sext both, i64.mul, i64.sub
- `emit_umaddl`: zext both (mask), i64.mul, i64.add with Xa
- `emit_umsubl`: Xa, zext both, i64.mul, i64.sub

## Tests

### Lift tests (u2-ir-lift)
- `smaddl_lift`: 0x9B2C2328 → `Smaddl { dst: 8, n: 25, m: 12, a: 8 }`
- `smsubl_lift`: 0xBB2C2328 → `Smsubl { dst: 8, n: 25, m: 12, a: 8 }`
- `umaddl_lift`: 0x9BAC2328 → `Umaddl { dst: 8, n: 25, m: 12, a: 8 }`
- `umsubl_lift`: 0xBBAC2328 → `Umsubl { dst: 8, n: 25, m: 12, a: 8 }`

### Backend value tests (u15-exec-wasmi)
- `backend_smaddl_values`: 5 cases (wrapping add)
- `backend_smsubl_values`: 3 cases (wrapping sub)
- `backend_umaddl_values`: 4 cases (wrapping add)
- `backend_umsubl_values`: 2 cases (wrapping sub)

All verified against Rust wrapping arithmetic.

## Test Results
- u2-ir-lift: 126/126 passed
- u15-exec-wasmi: 17/17 passed
- u3-wasm-jit: [pending]

## Boot Remeasure
- Status: Completed 2026-10-03 ~21:45 AEST
- Cap: 30,000,000 steps
- **Steps executed: 20,647,443** (up from 20,555,721 — **+91,722 steps**)
- **Final PC: 0xffffff80085c9610**
- **Halt reason: IllegalInstruction** { addr: 18446743524094023184, word: 3670016297 }
- **Word: 0xDAC00129** (3670016297) — system instruction group (top byte 0xDA)
- Elapsed: 752.7s (27,432 steps/s)
- Outcome: Halted (not cap-reached)

**Verdict: SMADDL fix works.** The boot progressed 91,722 steps past the SMADDL halt.
The new halt is a different instruction (system group), not a regression.
