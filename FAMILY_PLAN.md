# Family Plan: fam/fpsimd (ARM64 Floating-Point & SIMD)

## 1. Executive Summary & Mission
Track **fam/fpsimd** implements decode, interpreter semantics, unit tests, and QEMU-verified differential validation for AArch64 Floating-Point and Advanced SIMD instructions.
Per track instructions:
- Scope: F6 FP/SIMD (`/mnt/sdb1/gb-loop/families/F6-fp-simd.md`).
- Rule: Interpreter-first (U1/U2 only). Keep public contracts stable.
- First Deliverable: `FAMILY_PLAN.md` + Scalar FP basics (`fmov`, `fadd`, `fsub`, `fmul`, `fcmp`, `scvtf`, `fcvtzs`, plus sibling basics `fdiv`, `ucvtf`, `fcvtzu`, `fccmp`, `frinta`) with exact-word QEMU-differential verified tests.

---

## 2. Priority Hierarchy & Implementation Sequence

### Phase 1: Scalar FP Basics (First Deliverable)
Focus on core 32-bit (Single) and 64-bit (Double) scalar operations required for kernel boot arithmetic and type conversions:
1. **`fmov`** (imm/reg):
   - Register copy: `FMOV Sd, Sn` (32-bit), `FMOV Dd, Dn` (64-bit)
   - Immediate: `FMOV Sd, #imm8`, `FMOV Dd, #imm8` (8-bit modified float immediate)
   - General <-> FP register: `FMOV Wd, Sn`, `FMOV Xd, Dn`, `FMOV Sd, Wn`, `FMOV Dd, Xn`
2. **`fadd` / `fsub`**:
   - `FADD Sd, Sn, Sm`, `FADD Dd, Dn, Dm`
   - `FSUB Sd, Sn, Sm`, `FSUB Dd, Dn, Dm`
3. **`fmul` / `fdiv`**:
   - `FMUL Sd, Sn, Sm`, `FMUL Dd, Dn, Dm`
   - `FDIV Sd, Sn, Sm`, `FDIV Dd, Dn, Dm`
4. **`fcmp` / `fccmp`**:
   - `FCMP Sn, Sm`, `FCMP Dn, Dm` (sets PSTATE NZCV flags)
   - `FCMP Sn, #0.0`, `FCMP Dn, #0.0` (zero comparison)
   - `FCCMP Sn, Sm, #nzcv, cond`, `FCCMP Dn, Dm, #nzcv, cond` (conditional compare)
5. **`scvtf` / `ucvtf`**:
   - Signed integer to FP: `SCVTF Sd, Wn`, `SCVTF Dd, Xn`, `SCVTF Sd, Xn`, `SCVTF Dd, Wn`
   - Unsigned integer to FP: `UCVTF Sd, Wn`, `UCVTF Dd, Xn`, `UCVTF Sd, Xn`, `UCVTF Dd, Wn`
6. **`fcvtzs` / `fcvtzu`**:
   - FP to signed integer (round toward zero): `FCVTZS Wd, Sn`, `FCVTZS Xd, Dn`, `FCVTZS Wd, Dn`, `FCVTZS Xd, Sn`
   - FP to unsigned integer (round toward zero): `FCVTZU Wd, Sn`, `FCVTZU Xd, Dn`, `FCVTZU Wd, Dn`, `FCVTZU Xd, Sn`
7. **`frinta`**:
   - Round to nearest, ties away from zero: `FRINTA Sd, Sn`, `FRINTA Dd, Dn`

### Phase 2: Scalar FP Extended & Fused Multiply-Add
1. FMA: `fmadd`, `fmsub`, `fnmadd`, `fnmsub` (3-source fused multiply-add/subtract)
2. Min/Max: `fmax`, `fmin`, `fmaxnm`, `fminnm`
3. Unary: `fabs`, `fneg`, `fsqrt`
4. Rounding: `frintn`, `frintp`, `frintm`, `frintz`, `frinti`, `frintx`
5. Selection: `fcsel`
6. Precision convert: `fcvt` (S <-> D)

### Phase 3: FP Loads & Stores (SIMD&FP)
Coordinate with `fam/loadstore`:
1. `ldr` / `str` (SIMD&FP immediate unsigned offset, unscaled, pre/post indexed)
2. `ldp` / `stp` (SIMD&FP pair)

### Phase 4: SIMD Integer & Vector Operations
1. `dup` (element / general reg), `mov` (vector element)
2. Vector integer `add`, `sub`, `mul`
3. Vector shifts: `shl`, `ushr`, `sshr`
4. Vector table lookup / permute: `tbl`, `zip1/2`, `uzp1/2`
5. Structure load/store: `ld1`, `st1`

---

## 3. Static Frequency Ranking (from F6 Checklist)

| Mnemonic | Static Count | Scope Group | Priority |
| :--- | :--- | :--- | :--- |
| `fnmls` | 14,106 | Phase 2 (FMA) | High |
| `fcmla` | 5,368 | Phase 2 (Complex) | High |
| `fmls` | 3,714 | Phase 2 (FMA) | High |
| `fnmla` | 1,903 | Phase 2 (FMA) | Medium |
| `fnmsb` | 1,716 | Phase 2 (FMA) | Medium |
| `fmla` | 1,634 | Phase 2 (FMA) | Medium |
| `fmul` | 1,464 | Phase 1 (Scalar Arith) | **Deliverable 1** |
| `fnmadd` | 1,216 | Phase 2 (FMA) | Medium |
| `fnmad` | 1,105 | Phase 2 (FMA) | Medium |
| `fmadd` | 832 | Phase 2 (FMA) | Medium |
| `fnmsub` | 766 | Phase 2 (FMA) | Medium |
| `fmsub` | 592 | Phase 2 (FMA) | Medium |
| `fcmeq` | 542 | Phase 4 (Vector CMP) | Medium |
| `fmsb` | 533 | Phase 2 (FMA) | Medium |
| `fmad` | 512 | Phase 2 (FMA) | Medium |
| `fcmne` | 498 | Phase 4 (Vector CMP) | Medium |
| `fcsel` | 371 | Phase 2 (Select) | Medium |
| `fmopa` | 316 | SME / Matrix | Later |
| `fcvtzu` | 301 | Phase 1 (Convert) | **Deliverable 1** |
| `fmops` | 276 | SME / Matrix | Later |
| `facgt` | 271 | Phase 4 (Vector CMP) | Low |
| `fcvtzs` | 250 | Phase 1 (Convert) | **Deliverable 1** |
| `fcmgt` | 236 | Phase 4 (Vector CMP) | Low |
| `fmulx` | 233 | Phase 2 (Arith) | Low |
| `fccmp` | 200 | Phase 1 (Compare) | **Deliverable 1** |
| `fadd` | 179 | Phase 1 (Scalar Arith) | **Deliverable 1** |
| `ucvtf` | 140 | Phase 1 (Convert) | **Deliverable 1** |
| `scvtf` | 136 | Phase 1 (Convert) | **Deliverable 1** |
| `fmov` | 134 | Phase 1 (Move) | **Deliverable 1** |
| `fsub` | 125 | Phase 1 (Scalar Arith) | **Deliverable 1** |
| `fdiv` | 103 | Phase 1 (Scalar Arith) | **Deliverable 1** |
| `frinta` | 21 | Phase 1 (Round) | **Deliverable 1** |
| `fcmp` | 1 | Phase 1 (Compare) | **Deliverable 1** |

---

## 4. Encoding Bit-Patterns & Ranges

All instructions dispatched from `lib.rs` when `bits[28:25] in {0b0111, 0b1111, 0b0110, 0b1110}`.

### 4.1 Conversion between Floating-Point and Integer
Encoding: `sf 0 0 11110 type 1 rmode opcode Rn Rd`
- `bits[30:29] == 0b00`, `bits[28:24] == 0b11110`, `bit[21] == 1`
- `sf`: bit 31 (`0` = 32-bit GPR Wn/Wd, `1` = 64-bit GPR Xn/Xd)
- `type`: bits[23:22] (`00` = 32-bit Single Sn/Sd, `01` = 64-bit Double Dn/Dd)
- `rmode`: bits[20:19]
  - `00`:
    - `opcode == 010`: **`SCVTF`** (signed int -> float)
    - `opcode == 011`: **`UCVTF`** (unsigned int -> float)
    - `opcode == 110`: **`FMOV`** (FP to general register: `Wd = Sn` or `Xd = Dn`)
    - `opcode == 111`: **`FMOV`** (General register to FP: `Sd = Wn` or `Dd = Xn`)
  - `11`:
    - `opcode == 000`: **`FCVTZS`** (float -> signed int, round toward zero)
    - `opcode == 001`: **`FCVTZU`** (float -> unsigned int, round toward zero)

### 4.2 Floating-Point Data-Processing (1 source)
Encoding: `0 0 0 11110 type 1 0000 opcode Rn Rd`
- `bits[31:29] == 0b000`, `bits[28:24] == 0b11110`, `bit[21] == 1`, `bits[20:16] == 0b10000`
- `type`: bits[23:22] (`00` = S, `01` = D)
- `opcode`: bits[15:10]
  - `000000`: **`FMOV`** (register copy `Sd, Sn` or `Dd, Dn`)
  - `000001`: **`FABS`**
  - `000010`: **`FNEG`**
  - `000011`: **`FSQRT`**
  - `001100`: **`FRINTA`** (round to nearest, ties away)

### 4.3 Floating-Point Data-Processing (2 sources)
Encoding: `0 0 0 11110 type 1 Rm opcode 10 Rn Rd`
- `bits[31:29] == 0b000`, `bits[28:24] == 0b11110`, `bit[21] == 1`, `bits[11:10] == 0b10`
- `type`: bits[23:22] (`00` = S, `01` = D)
- `opcode`: bits[15:12]
  - `0000`: **`FMUL`**
  - `0001`: **`FDIV`**
  - `0010`: **`FADD`**
  - `0011`: **`FSUB`**

### 4.4 Floating-Point Immediate
Encoding: `0 0 0 11110 type 1 imm8 100 00 Rd`
- `bits[31:29] == 0b000`, `bits[28:24] == 0b11110`, `bit[21] == 1`, `bits[12:10] == 0b100`, `bits[9:5] == 0b00000`
- `type`: bits[23:22] (`00` = S, `01` = D)
- `imm8`: bits[20:13] expanded per IEEE-754: `(-1)^s * 2^(exp - 3) * (1 + frac/16)`
- Mnemonic: **`FMOV`** (immediate: `FMOV Sd, #imm` or `FMOV Dd, #imm`)

### 4.5 Floating-Point Compare & Conditional Compare
Encoding:
- `FCMP (register)`: `0 0 0 11110 type 1 Rm 00100 Rn 00 0 00`
- `FCMP (zero)`: `0 0 0 11110 type 1 00000 00100 Rn 01 0 00`
- `FCCMP`: `0 0 0 11110 type 1 Rm cond 01 Rn nzcv`
Sets PSTATE NZCV flags:
- If unordered (either operand is NaN): `N=0, Z=0, C=1, V=1`
- If equal (`==`): `N=0, Z=1, C=1, V=0`
- If less than (`<`): `N=1, Z=0, C=0, V=0`
- If greater than (`>`): `N=0, Z=0, C=1, V=0`

---

## 5. Architectural Edge Cases & Semantic Rules

1. **Register Width & Aliasing**:
   - Scalar 32-bit FP ops write to `Sd` and clear bits[127:32] of `Vd`.
   - Scalar 64-bit FP ops write to `Dd` and clear bits[127:64] of `Vd`.
   - Reading `Sn` reads bits[31:0] of `Vn`; reading `Dn` reads bits[63:0] of `Vn`.
2. **Integer Register 31 (XZR / WZR)**:
   - For `FMOV Wd, Sn` / `FMOV Xd, Dn`: if `Rd == 31`, write is dropped (XZR semantics).
   - For `FMOV Sd, Wn` / `FMOV Dd, Xn`: if `Rn == 31`, reads `0` (XZR semantics).
   - For integer conversions `SCVTF` / `UCVTF`: if `Rn == 31`, reads `0`.
   - For integer conversions `FCVTZS` / `FCVTZU`: if `Rd == 31`, write is dropped.
3. **NaN Propagation & Floating-Point Flags**:
   - Comparisons with NaN set NZCV to `0b0011` (C=1, V=1, N=0, Z=0).
   - +0.0 and -0.0 compare equal (`Z=1, C=1`).
4. **Rounding Modes**:
   - `FCVTZS` / `FCVTZU`: explicit truncation (round toward zero `RZ`).
   - `FRINTA`: explicit round to nearest, ties away from zero.
   - Default IEEE arithmetic (`FADD`, `FSUB`, `FMUL`, `FDIV`): round to nearest, ties to even (`RN`).
5. **Integer Conversion Overflow / Underflow**:
   - If `FCVTZS` input exceeds signed int max (or is NaN / +Inf), result saturates to `INT_MAX`.
   - If input is -Inf (or below INT_MIN), result saturates to `INT_MIN`.
   - `FCVTZU` saturates to `UINT_MAX` (for positive overflow / NaN / +Inf) and `0` (for negative).

---

## 6. Verification & QEMU Differential Validation Plan

1. **Harness**:
   - Use `/home/muse/qemu-root/usr/bin/qemu-system-aarch64` (`-accel tcg,one-insn-per-tb=on -d cpu`) with bare-metal flat binary executing target instructions preceded by `CPACR_EL1` FPEN enablement.
   - Read back FP register state into integer registers via `FMOV` and inspect exact bitwise state.
2. **Unit Tests with Exact Word Witnesses**:
   - Every opcode form must include a dedicated unit test in `u1-decode` verifying `decode(word)` succeeds.
   - Semantics tests verifying exact arithmetic, conversion, flag setting, and NaN/zero behavior against QEMU golden outputs.
