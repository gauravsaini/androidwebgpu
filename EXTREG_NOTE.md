# EXTREG_NOTE.md — 32-bit Extended-Register ADD/SUB

## Summary

Implemented the 32-bit form of ADD/SUB (extended register) in the u12-orchestrator
fast-path interpreter (`execute_arm64`, section 4c). The 64-bit form (section 4b)
was already handled; the 32-bit form fell through to U2 and halted the boot.

## The Halt

- **Steps**: 20,473,766
- **PC**: 0xffffff8008c736cc
- **Word**: 0x6B21011F
- **Decoded**: `subs wzr, w8, w1, uxtb` (32-bit SUBS, UXTB extend, Rd=WZR discard)
- **Reason**: DataProc: unsupported encoding

## Encoding

```
sf op S 01011 opt 1 Rm option imm3 Rn Rd
31 30 29 28-24  23 22 21 20-16 15-13 12-10 9-5 4-0
```

For 32-bit (sf=0), valid options are UXTB/UXTH/UXTW/SXTB/SXTH/SXTW
(000/001/010/100/101/110). UXTX/SXTX (011/111) are UNDEFINED and fall through.

## Implementation

Location: `units/u12-orchestrator/src/lib.rs`, section 4c (after the 64-bit 4b block).

- Rm=31 reads as WZR (0).
- Rn=31: WSP (low 32 bits of SP) for S=0; WZR (0) for S=1.
- Rd=31: SP (zero-extended write) for S=0; discard for S=1 (CMP/CMN).
- All operations 32-bit; results zero-extend to 64 bits.
- Flags via existing `nzcv_add32`/`nzcv_sub32` helpers.
- imm3 must be <= 4; invalid options fall through (do not claim).

## Tests

Three new tests (all pass):
- `gb27_subs_extended_uxtb_32`: Exact kernel word 0x6B21011F. W8=0x100, W1=0xFF.
  0x100-0xFF=1 => NZCV = C set (0x20000000).
- `gb27_add_extended_sxtb_32`: ADD W2, W3, W4, SXTB #1 (0x0B248462).
  W3=0x1000, W4=0xFB (-5 sign-extended, <<1 = -10). Result: 0xFF6.
- `gb27_sub_extended_uxtw_32_zero_extends`: SUB W5, W6, W7, UXTW (0x4B2740C5).
  Verifies 32-bit result zero-extends to 64 bits.

Full u12-orchestrator suite: **116 passed** (5 RAM-heavy skipped, standard).

## Boot Re-measurement

| Metric | Before | After |
|--------|--------|-------|
| Steps | 20,473,766 | **20,555,721** (+81,955) |
| Halt PC | 0xffffff8008c736cc | 0xffffff8008355394 |
| Halt word | 0x6B21011F (SUBS ext) | **0x9B2C2328** |
| Halt reason | DataProc: unsupported | DataProc: unsupported |

## New Halt: SMADDL

0x9B2C2328 = `smaddl x8, w25, w12, x8` (capstone-verified).
- X8 = X8 + SignExtend(W25) * SignExtend(W12)
- Signed multiply-add long. 32-bit operands sign-extended to 64, 64-bit multiply+add.
- This is OUT OF SCOPE for this task (data-proc add/sub only).
- Recommended next track: implement SMADDL/SMSUBL/UMADDL/UMSUBL (the "long" multiply group).

## Files Changed

- `units/u12-orchestrator/src/lib.rs`: Section 4c (32-bit extended ADD/SUB) + 3 tests.
- `EXTREG_NOTE.md`: This file.

Commit: `feat(emu): implement 32-bit extended-register ADD/SUB in fast-path`
