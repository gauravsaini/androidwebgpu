# UMULL/SMULL Implementation Note

**Date:** 2026-10-03
**Branch:** feat/umull
**Base:** 22f97ea

## Problem

Boot halted at 58,603 steps, PC 0xffffff800821a430, instruction 0x9BAA7D29,
with "DataProc: unsupported encoding".

Decoded:
```
0x9BAA7D29 = umull x9, w9, w10
  sf=1, op54=00, op31=101, Rm=x10, Ra=x31 (XZR), Rn=x9, Rd=x9
  Xd = (u64)Wn * (u64)Wm (32-bit unsigned multiply, 64-bit result)
```

## Solution

Added `Umull` and `Smull` IrOps to `contracts/src/cpu.rs`:
- `Umull { dst, n, m }`: `dst = (n as u32 as u64) * (m as u32 as u64)`
- `Smull { dst, n, m }`: `dst = (n as i32 as i64) * (m as i32 as i64)`

### Lifter (u2-ir-lift)
Matches `op54=00, op31=101` (UMULL) and `op54=00, op31=001` (SMULL),
requires `sf=1` and `Ra=XZR`, emits the new IrOps.

### WASM Backend (u3-wasm-jit)
- `emit_umull`: masks both operands to 32 bits (`i64.and 0xFFFFFFFF`), then `i64.mul`.
- `emit_smulh_long`: sign-extends via `shl 32` + `shr_s 32`, then `i64.mul`.
- Both use only the stack (no additional locals needed).

## Tests

- `u2-ir-lift`: 122 passed (including `umull_lift`, `smull_lift` shape tests)
- `u3-wasm-jit`: 39 passed
- `u15-exec-wasmi`: 13 passed (including `backend_umull_values`, `backend_smull_values`
  with 6 cases each, verifying against Rust's wrapping_mul, including cases where
  upper 32 bits of the 64-bit registers are non-zero to verify masking)

## Boot Re-measurement

| | Before | After |
|---|---|---|
| Steps | 58,603 (halted) | **5,000,000 (hit cap, still running)** |
| Halt PC | 0xffffff800821a430 | N/A (no halt) |
| Reason | DataProc: unsupported encoding | N/A |

**The boot no longer halts!** It ran 5,000,000 steps (the BOOT_MAX_STEPS cap) without
encountering an unsupported instruction.

At the cap, the kernel is in a spin-wait loop at PC 0xffffff80085bf60c:
```
0x39400028, 0x39400009, 0x51000442, 0x7100005f,
0x38001408, 0x38001429, 0x54ffff4c (b.ne - loops back)
```

This appears to be polling a hardware register (likely waiting for a device that
isn't present in the emulator). This is a qualitative change: the kernel is now
*running* rather than crashing on unsupported instructions.

## Next Steps

The spin-wait loop needs investigation:
1. Identify what hardware register is being polled (decode the ldrb/strb addresses).
2. Determine if the emulator needs to model that device, or if the kernel can be
   configured to skip it.
3. Consider increasing BOOT_MAX_STEPS or adding a spin-detection heuristic.
