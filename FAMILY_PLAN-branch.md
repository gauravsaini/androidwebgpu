# FAMILY_PLAN.md — F2 Control Flow (Branch) Instruction Family

## 1. Overview & Static Frequency Analysis

The **F2-branch** family represents **871,914 static instructions** in the target Linux 4.19 AOSP ARM64 kernel (`/mnt/sdb1/aosp/Image`), accounting for **20.0%** of all decoded instructions (excluding UDF).

### Mnemonic Priority Table (by static kernel occurrence)

| Priority | Mnemonic | Static Count | Description | Primary Group | Current Status |
|:---:|:---|---:|:---|:---|:---|
| 1 | `bl` | 239,166 | Branch with Link (immediate) | Unconditional imm | Implemented (U1/U2) |
| 2 | `b` | 170,274 | Branch (immediate) | Unconditional imm | Implemented (U1/U2) |
| 3 | `cbz` | 108,670 | Compare & Branch Zero (32/64-bit) | Comp & branch imm | Implemented (U1/U2) |
| 4 | `cbnz` | 57,671 | Compare & Branch Nonzero (32/64-bit) | Comp & branch imm | Implemented (U1/U2) |
| 5 | `b.ne` | 50,106 | Branch if Not Equal (Z == 0) | Conditional branch | Implemented (U1/U12 fast-path) |
| 6 | `tbnz` | 48,691 | Test & Branch Nonzero (b0..b63) | Test & branch imm | Implemented (U1/U12 fast-path) |
| 7 | `tbz` | 46,540 | Test & Branch Zero (b0..b63) | Test & branch imm | Implemented (U1/U12 fast-path) |
| 8 | `ret` | 44,846 | Return from subroutine | Unconditional reg | Implemented (U1/U2) |
| 9 | `b.eq` | 35,261 | Branch if Equal (Z == 1) | Conditional branch | Implemented (U1/U12 fast-path) |
| 10 | `b.hs` | 16,737 | Branch if Carry Set / Higher or Same (C == 1) | Conditional branch | Implemented (U1/U12 fast-path) |
| 11 | `blr` | 11,206 | Branch with Link to Register | Unconditional reg | Implemented (U1/U2) |
| 12 | `b.lo` | 9,506 | Branch if Carry Clear / Lower (C == 0) | Conditional branch | Implemented (U1/U12 fast-path) |
| 13 | `b.hi` | 9,220 | Branch if Higher (C == 1 && Z == 0) | Conditional branch | Implemented (U1/U12 fast-path) |
| 14 | `b.ls` | 6,134 | Branch if Lower or Same (C == 0 \|\| Z == 1) | Conditional branch | Implemented (U1/U12 fast-path) |
| 15 | `b.lt` | 4,483 | Branch if Less Than (N != V) | Conditional branch | Implemented (U1/U12 fast-path) |
| 16 | `b.gt` | 4,034 | Branch if Greater Than (Z == 0 && N == V) | Conditional branch | Implemented (U1/U12 fast-path) |
| 17 | `b.le` | 2,499 | Branch if Less or Equal (Z == 1 \|\| N != V) | Conditional branch | Implemented (U1/U12 fast-path) |
| 18 | `b.ge` | 2,344 | Branch if Greater or Equal (N == V) | Conditional branch | Implemented (U1/U12 fast-path) |
| 19 | `b.mi` | 1,091 | Branch if Minus / Negative (N == 1) | Conditional branch | Implemented (U1/U12 fast-path) |
| 20 | `br` | 1,034 | Branch to Register | Unconditional reg | Implemented (U1/U2) |
| 21 | `b.pl` | 734 | Branch if Plus / Positive or Zero (N == 0) | Conditional branch | Implemented (U1/U12 fast-path) |
| 22 | `b.vc` | 521 | Branch if Overflow Clear (V == 0) | Conditional branch | Implemented (U1/U12 fast-path) |
| 23 | `b.nv` | 419 | Branch if Never (Architectural Always) | Conditional branch | **Bug fix needed** (rejected in U1 & U12) |
| 24 | `b.vs` | 393 | Branch if Overflow Set (V == 1) | Conditional branch | Implemented (U1/U12 fast-path) |
| 25 | `b.al` | 334 | Branch if Always | Conditional branch | Implemented (U1/U12 fast-path) |

---

## 2. Encoding Schemes & Bit Ranges

### 2.1 Unconditional Branch (Immediate): `B`, `BL`
- **Range / Mask**: `bits[31] = op` (0 = `B`, 1 = `BL`), `bits[30:26] = 00010` (total `bits[31:26] = 0b000101` or `0b100101`)
- **Payload**: `imm26 = bits[25:0]`
- **Semantics**:
  - `offset = SignExtend(imm26 << 2, 64)` (+/- 128 MiB range)
  - `target = PC + offset`
  - `BL`: `X[30] = PC + 4` before branching

### 2.2 Compare and Branch (Immediate): `CBZ`, `CBNZ`
- **Range / Mask**: `bits[30:25] = 011010`, `bit[24] = op` (0 = `CBZ`, 1 = `CBNZ`), `bit[31] = sf` (0 = 32-bit `W`, 1 = 64-bit `X`)
- **Payload**: `imm19 = bits[23:5]`, `Rt = bits[4:0]`
- **Semantics**:
  - `offset = SignExtend(imm19 << 2, 64)` (+/- 1 MiB range)
  - `target = PC + offset`
  - If `sf == 0`: test low 32 bits `(regs[Rt] as u32)`. If `sf == 1`: test full 64 bits `regs[Rt]`.
  - Condition holds if `val == 0` (`CBZ`) or `val != 0` (`CBNZ`).
  - If Rt = 31: reads as `XZR` (0). `CBZ XZR` is always taken; `CBNZ XZR` is never taken.

### 2.3 Test and Branch (Immediate): `TBZ`, `TBNZ`
- **Range / Mask**: `bits[30:25] = 011011`, `bit[24] = op` (0 = `TBZ`, 1 = `TBNZ`), `bit[31] = b5`, `bits[23:19] = b40`
- **Payload**: `bit_pos = (b5 << 5) | b40` (0..63), `imm14 = bits[18:5]`, `Rt = bits[4:0]`
- **Semantics**:
  - `offset = SignExtend(imm14 << 2, 64)` (+/- 32 KiB range)
  - `target = PC + offset`
  - Bit extracted: `((regs[Rt] >> bit_pos) & 1)` (if Rt = 31, val = 0).
  - Condition holds if `bit == 0` (`TBZ`) or `bit != 0` (`TBNZ`).

### 2.4 Conditional Branch (Immediate): `B.cond`
- **Range / Mask**: `bits[31:24] = 0x54` (`01010100`), `bit[4] = 0`, `cond = bits[3:0]` (0..15)
- **Payload**: `imm19 = bits[23:5]`
- **Semantics**:
  - `offset = SignExtend(imm19 << 2, 64)` (+/- 1 MiB range)
  - `target = PC + offset`
  - Condition evaluation against `PSTATE.NZCV`:
    - `0000` EQ: `Z == 1`
    - `0001` NE: `Z == 0`
    - `0010` CS/HS: `C == 1`
    - `0011` CC/LO: `C == 0`
    - `0100` MI: `N == 1`
    - `0101` PL: `N == 0`
    - `0110` VS: `V == 1`
    - `0111` VC: `V == 0`
    - `1000` HI: `C == 1 && Z == 0`
    - `1001` LS: `C == 0 || Z == 1`
    - `1010` GE: `N == V`
    - `1011` LT: `N != V`
    - `1100` GT: `Z == 0 && N == V`
    - `1101` LE: `Z == 1 || N != V`
    - `1110` AL: Always taken (`true`)
    - `1111` NV: Always taken (`true`) — **CRITICAL AUDIT LESSON**: MUST NOT TRAP

### 2.5 Unconditional Branch (Register): `BR`, `BLR`, `RET`
- **Range / Mask**: `bits[31:25] = 1101011`, `bits[24:21] = 0000`, `bits[15:10] = 000000`, `bits[4:0] = 00000`
- **Opcodes**:
  - `BR`: `bits[20:16] = 00000`, `word & 0xFFFF_FC1F == 0xD61F_0000`
  - `BLR`: `bits[20:16] = 00001`, `word & 0xFFFF_FC1F == 0xD63F_0000`
  - `RET`: `bits[20:16] = 00010`, `word & 0xFFFF_FC1F == 0xD65F_0000`
- **Payload**: `Rn = bits[9:5]`
- **Semantics**:
  - Target = `regs[Rn]`
  - `BLR`: `X[30] = PC + 4`. When `Rn == 30`, branch target is the old `X[30]` before linking.
  - `RET`: Defaults to `Rn = 30` (`0xD65F03C0`).

---

## 3. Discovered Bugs & Edge Cases

1. **`B.cond` NV Condition Exclusion Bug**:
   - In `units/u1-decode/src/branch.rs`: `if cond < 0b1111` dropped `cond == 15` (`b.nv`, 419 static occurrences).
   - In `units/u12-orchestrator/src/lib.rs`: `if cond < 15` dropped `b.nv` execution.
   - **Fix**: Accept `cond <= 15` (or remove `< 15` check, since 4 bits can only be 0..15).
2. **Testgen GAP-1 (Oracle PC Capture)**:
   - `scripts/testgen/oracle.py` hardcoded `expected_pc = initial_pc + 4` by subtracting `test_{i}_after - test_{i}_insn` statically.
   - Branches jumping to taken landing pad bypassed state capture or produced wrong PC.
   - **Fix**: Provide dynamic fallthrough and taken landing pads in runner assembly, capturing dynamic branch outcome and actual landing PC.
3. **`BLR X30` Alias Hazard**:
   - Setting `X30 = PC + 4` before indirect jump must not overwrite the target register when `Rn == 30`. Verified that `u2-ir-lift` preserves old X30 via `SCRATCH`.
4. **Zero Register `XZR` (R31) Semantics**:
   - `CBZ XZR` is unconditionally taken; `CBNZ XZR` is unconditionally fallthrough.
   - `TBZ XZR, #bit` is unconditionally taken; `TBNZ XZR, #bit` is unconditionally fallthrough.

---

## 4. Action Plan & Test Strategy

1. **Fix `B.cond` NV (15) acceptance**:
   - Update `units/u1-decode/src/branch.rs` to allow `cond == 15`.
   - Update `units/u12-orchestrator/src/lib.rs` to allow `cond == 15`.
2. **Add exhaustive unit test coverage**:
   - `u1-decode`: Test all 16 conditions of `B.cond` (0..=15), 32-bit & 64-bit `CBZ`/`CBNZ`, `TBZ`/`TBNZ`, `B`, `BL`, `BR`, `BLR`, `RET`.
   - `u2-ir-lift`: Test lift output for 64-bit and 32-bit `CBZ`/`CBNZ`, `B`, `BL`, `RET`, `BR`, `BLR`.
   - `u12-orchestrator`: Test taken and not-taken execution with exact witness words for every condition and branch type.
3. **Fix Testgen GAP-1 & Generate Test Vectors**:
   - Update `oracle.py` and `families.py` in `/mnt/sdb1/wt-testgen`.
   - Generate verified vectors for `b_cond`, `cbz`, `cbnz`, `tbz`, `tbnz`.
   - Validate against emulator pipeline.
4. **QEMU differential trace verification**:
   - Run trace differ against QEMU boot trace to confirm zero divergence on branch instructions.
