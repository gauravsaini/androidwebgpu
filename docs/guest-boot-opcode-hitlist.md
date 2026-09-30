# Guest-boot opcode hit-list

Static instruction census of the real AOSP kernel binary, mapped against what
the guest-boot emulator (`units/u1-decode`, `units/u2-ir-lift`) can actually
execute. Purpose: tell the next implementation track exactly which instruction
classes buy the most boot progress per unit of work.

**Analysis only — no emulator code was changed for this document.**

## Method

- Binary: `/mnt/sdb1/aosp/Image` (23,073,280 bytes, read-only), window
  `0x0–0x1000000` (first 16 MB) = **4,194,304** 32-bit words.
- Classifier: `docs/guest-boot-opcode-histogram.py` — a coarse decision tree
  over fixed AArch64 encoding bits (one class per word, ~90 lines).
- Validation, three ways:
  1. `--self-test`: **107/107** class checks pass against ground truth from
     `aarch64-linux-gnu-as` (each check assembles a real instruction and
     asserts the class; catches mask collisions like MOVZ `0xD2` vs EOR-imm
     `0xD2`, distinguished by bit 23).
  2. Cross-check vs `aarch64-linux-gnu-objdump -D` exact mnemonic histogram
     over the same 16 MB (3,545,402 insns): `bl` 223,457, `b` 163,339,
     `cbz` 98,274, `tbz`/`tbnz`/`cbnz` all match **exactly**; class totals
     agree within ~1% after alias grouping.
  3. Confusion check on a 354,759-word sample (word, objdump-mnemonic pairs):
     every class is pure (e.g. `branch-imm-bl` = 100% `bl`,
     `dp-imm-pcrel` = `adrp`/`adr` only).

## Status legend

Measured against `units/u1-decode` (decode) + `units/u2-ir-lift` (lift to
executable IR) on branch `gb-audit`:

- **IMPLEMENTED** — U1 decodes the common forms AND U2 lifts them to real
  `IrOp`s (no trap on the hot path).
- **PARTIAL** — common forms lift, but meaningful sub-forms trap in U2
  (reason + trapped counts in the Notes column).
- **MISSING** — U1 rejects the class, or U2 traps all of it
  (`Branch: only B/BL/RET/CBZ/CBNZ are lifted`, `R_DP_UNSUPPORTED`, …).
- **MISSING-by-design** — correctly unimplemented (should halt/trap the guest:
  `BRK`/`SVC`/`ERET`).

## Top-30 instruction classes (first 16 MB of the kernel)

| # | Class | Count | % | Status | Notes |
|---|-------|------:|--:|--------|-------|
| 1 | `ldst-imm` | 578,169 | 13.78 | PARTIAL | LDR/STR unsigned-imm W/X lifted; **B/H sub-word 89,486 trap** (`R_LS_SUBWORD`); SP base traps |
| 2 | `dp-reg-logical` | 422,743 | 10.08 | PARTIAL | AND/ORR/EOR/ANDS shifted-reg lifted both widths; **32-bit ORR traps** (`R_ORR32`); `mov`/`mvn`/`tst` are aliases here |
| 3 | `dp-imm-addsub` | 375,024 | 8.94 | PARTIAL | 64-bit S=0 ADD/SUB lifted (246,281); **S=1 (ADDS/SUBS/CMP/CMN) 94,109 + 32-bit 34,634 trap** |
| 4 | `ldst-pair` | 301,519 | 7.19 | IMPLEMENTED | STP/LDP/LDPSW/STNP/LDNP, integer regs, all index modes; SP-base edge traps |
| 5 | `dp-imm-movewide` | 247,976 | 5.91 | IMPLEMENTED | MOVZ/MOVN/MOVK, all widths |
| 6 | `branch-imm-bl` | 223,457 | 5.33 | IMPLEMENTED | BL (link + static branch) |
| 7 | `dp-reg-addsub` | 177,813 | 4.24 | PARTIAL | **Only 22,797 (12.8%) lift**: 64-bit ADD S=0 LSL#0. SUB-reg, CMP-reg (51k+26k), shifted/extended, 32-bit, S=1 all trap |
| 8 | `branch-imm-b` | 163,339 | 3.89 | IMPLEMENTED | B (static branch) |
| 9 | `dp-imm-pcrel` | 143,913 | 3.43 | IMPLEMENTED | ADR/ADRP → static `Mov` |
| 10 | `branch-cond` | 142,540 | 3.40 | MISSING | B.cond: **U1 decodes it, U2 traps** (`R_BR_UNSUPPORTED`) — decode-only |
| 11 | `ldst-unscaled` | 132,592 | 3.16 | MISSING | LDUR/STUR (+LDTR): **U1 rejects** (unscaled kept out of scope) |
| 12 | `fp-simd` | 113,947 | 2.72 | MISSING | FP/SIMD data-processing: out of scope |
| 13 | `cbz` | 98,274 | 2.34 | PARTIAL | 64-bit (64,885) lifted; **32-bit (33,389) traps** (`R_CBZ32`) |
| 14 | `dp-imm-bitfield` | 80,064 | 1.91 | IMPLEMENTED | SBFM/BFM/UBFM (`lsl`/`lsr`/`ubfx`/`sbfiz` aliases) |
| 15 | `dp-imm-logical` | 66,763 | 1.59 | IMPLEMENTED | AND/ORR/EOR/ANDS-imm, both widths |
| 16 | `sve` | 61,240 | 1.46 | MISSING | SVE: out of scope |
| 17 | `branch-reg` | 60,750 | 1.45 | PARTIAL | RET (41,943) lifted; **BLR (14,628) + BR (4,179) rejected by U1** |
| 18 | `cbnz` | 46,440 | 1.11 | PARTIAL | 64-bit (19,323) lifted; **32-bit (27,117) traps** |
| 19 | `dp-reg-cond` | 42,549 | 1.01 | MISSING | CSEL/CSET/CSINC + CCMN/CCMP: U2 has no conditional IR |
| 20 | `tbnz` | 39,625 | 0.94 | MISSING | Decoded by U1, **U2 traps** |
| 21 | `dp-reg-mul` | 37,923 | 0.90 | MISSING | MADD/MUL/MSUB/SMULL/UMULL: U2 traps (`R_DP_UNSUPPORTED`) |
| 22 | `tbz` | 35,436 | 0.84 | MISSING | Decoded by U1, **U2 traps** |
| 23 | `ldst-reg` | 30,751 | 0.73 | PARTIAL | Reg-offset LDR/STR: **only LSL-extend (option 011) lifts**; other extends trap |
| 24 | `dp-reg-other` | 28,992 | 0.69 | MISSING | CSINV/CSNEG/SBCS/ADCS-with-carry: no IR |
| 25 | `system-hint` | 28,217 | 0.67 | IMPLEMENTED | NOP/HINT/DMB/DSB/ISB → honest empty; WFI → `IrOp::Wfi` |
| 26 | `ldst-literal` | 25,783 | 0.61 | IMPLEMENTED | LDR/LDRSW literal |
| 27 | `system-mrs` | 17,399 | 0.41 | PARTIAL | Only CTR_EL0/TPIDR_EL1/CurrentEL/SCTLR/DAIF/NZCV lifted; rest trap |
| 28 | `ldst-excl` | 16,551 | 0.39 | MISSING | LDXR/STXR/CAS/SWP atomics: no IR |
| 29 | `ldst-simd` | 13,316 | 0.32 | MISSING | SIMD&FP loads/stores |
| 30 | `dp-reg-2src` | 11,181 | 0.27 | PARTIAL | LSLV/LSRV/ASRV/RORV (9,301) lifted; **SDIV/UDIV (1,880) trap** |

Top-30 covers **89.71%** of all words. A further **9.56%** (`unknown`,
401,059) is data-as-code: zero padding (`0x00000000` × 7,471) and words
objdump itself prints as `.inst … ; undefined` (spot-verified).

## Remaining classes (31–37)

| Class | Count | % | Status | Notes |
|-------|------:|--:|--------|-------|
| `exception` | 11,144 | 0.27 | MISSING-by-design | BRK (9,462) / SVC / HVC / SMC: must trap or halt the guest, never execute |
| `system-msr-reg` | 8,712 | 0.21 | PARTIAL | Same story as `system-mrs`: whitelisted regs only |
| `dp-reg-1src` | 5,104 | 0.12 | MISSING | RBIT/REV/CLZ/CLS |
| `system-msr-imm` | 3,177 | 0.08 | MISSING | PSTATE writes (`msr daifset`, …): trap |
| `system-other` | 584 | 0.01 | MISSING | Unmatched system encodings |
| `system-sys` | 227 | 0.01 | IMPLEMENTED | DC/IC/TLBI cache/TLB maintenance → honest NOP |
| `eret` | 11 | 0.00 | MISSING-by-design | Exception return: halt, not execution |

## Sub-form breakdown (the numbers behind the PARTIALs)

Coarse classes merge sub-forms with different lift status. Exact counts from
the binary (`bit29` = S flag, `sf` = 64-bit):

**`dp-imm-addsub` (375,024)** — the `CMP`-imm problem:

| Sub-form | Count | U2 |
|----------|------:|----|
| 64-bit, S=0 (ADD/SUB) | 246,281 | lifted |
| 64-bit, S=1 (ADDS/SUBS/CMP/CMN) | 33,052 | **trap** |
| 32-bit, S=0 | 34,634 | **trap** (`R_ADD32_IMM`/`R_SUB32_IMM`) |
| 32-bit, S=1 | 61,057 | **trap** |

**`dp-reg-addsub` (177,813)** — only 12.8% lifts today:

| Sub-form | Count | U2 |
|----------|------:|----|
| 64-bit ADD, S=0, LSL#0 | 22,797 | lifted |
| 64-bit SUBS-reg, S=1 (mostly `cmp xN, xM`) | 54,478 | **trap** |
| 32-bit SUBS-reg, S=1 | 30,877 | **trap** |
| 64-bit SUB-reg, S=0 | 19,522 | **trap** |
| 32-bit ADD/SUB-reg, S=0 | 28,133 | **trap** |
| shifted (any LSL/LSR/ASR #≠0), S=0/1 | ~13,000 | **trap** (`R_ADD_SHIFT`) |
| remainder (extended-register forms etc.) | ~9,000 | **trap** |

**`ldst-imm` (578,169)** — widths: 8B 335,086 / 4B 153,597 lifted;
2B 33,092 + 1B 56,394 = **89,486 trap** (`R_LS_SUBWORD`).
**`ldst-unscaled` (132,592)** — 8B 110,534 / 4B 12,536 / 2B 3,518 / 1B 6,004,
all missing. **`cbz`/`cbnz`** — 32-bit traps: cbz 33,389 of 98,274;
cbnz 27,117 of 46,440. **`branch-reg`** — ret 41,943 lifted;
blr 14,628 + br 4,179 rejected. **`dp-reg-2src`** — shift-variable 9,301
lifted; SDIV/UDIV 1,880 trap.

## Top-10 missing (by un-lifted instruction count)

| # | Group | Missing | Why it matters |
|---|-------|--------:|----------------|
| 1 | ADD/SUB-reg sub-forms (CMP-reg, SUB-reg, shifted, 32-bit, S=1) | 155,016 | compares drive every loop/branch in boot code |
| 2 | B.cond | 142,540 | the other half of every conditional |
| 3 | LDUR/STUR (unscaled) | 132,592 | 83% are 8-byte; stack spills live here |
| 4 | ADD/SUB-imm S=1 or 32-bit (CMP-imm, ADDS/SUBS) | 128,743 | `cmp xN, #imm` is the single most common compare |
| 5 | FP/SIMD data-processing | 113,947 | high count, but likely crypto/memcpy — verify dynamically before investing |
| 6 | LDRB/STRB/LDRH/STRH | 89,486 | byte/halfword MMIO + string ops |
| 7 | TBZ/TBNZ | 75,061 | bit-tests guarding feature flags |
| 8 | SVE | 61,240 | like FP/SIMD: count first, ask questions later |
| 9 | CBZ/CBNZ 32-bit | 60,506 | same shape as the 64-bit already lifted |
| 10 | CSEL/CSINC/CSINV/CCMP (+carry group) | ~71,000 | branchless selects; CCMP gates compound conditions |

## Next-10 implementation targets (count × feasibility)

Ordered for boot-progress per effort, not raw count. Static ranking only —
re-order against real `halt_step` traces once the emulator runs further.

1. **B.cond** (142,540) — U1 already decodes; add per-condition `CondBranch`
   in U2. Pairs with every CMP this document lists.
2. **LDUR/STUR** (132,592) — signed-imm9 twin of the already-lifted
   unsigned-imm path; near-trivial.
3. **TBZ/TBNZ** (75,061) — decoded already; needs a bit-test conditional
   branch (same IR shape as B.cond).
4. **ADDS/SUBS/CMP/CMN-imm** (94,109) + **32-bit ADD/SUB-imm** (34,634) —
   needs NZCV flags in the IR; the single biggest semantic gap.
5. **SUB-reg / CMP-reg / shifted ADD-reg** (155,016) — same flags work plus
   shifted-register operands; unlocks together with #4.
6. **LDRB/STRB/LDRH/STRH** (89,486) — sub-word `LoadDyn`/`StoreDyn` widths.
7. **CBZ/CBNZ 32-bit** (60,506) — zero-test on the low 32 bits; trivial.
8. **CSEL/CSINC/CSINV/CCMP** (~71,000) — conditional-select IR op.
9. **MADD/MUL/MSUB** (37,923) — new `Mul` IR op; self-contained.
10. **BLR/BR** (18,807) — `BranchDyn` already exists for RET; widen the U1
    mask (`0xD61F…`/`0xD63F…`).

Deliberately deferred: FP/SIMD (113,947), SVE (61,240), atomics
(LDXR/STXR/CAS/SWP, 16,551), SIMD&FP ldst (13,316), the system-register long
tail — high effort or the wrong layer for integer boot. Revisit only if
`halt_step` profiling points at them.

## Limits (read before planning from this)

- **Static, not dynamic.** Counts are occurrences in the binary, not executed
  instructions. Boot code is loop-heavy; a rare class on a hot path beats a
  common class on a cold one. Re-rank against `halt_step` traces.
- **Data as code.** The kernel Image mixes code and data; ~9.6% of words are
  unclassifiable (zeros, constants objdump calls `.inst … ; undefined`).
  The long tail of any class contains some data noise.
- **Coarse classes.** One class can span IMPLEMENTED and MISSING sub-forms
  (see the sub-form table); the Status column reflects the dominant lift
  outcome, the Notes column the trapped remainder.
- **16 MB window.** Only the first 16 MB was scanned; later regions
  (drivers, initramfs-adjacent code) may shift the ranking.
- **Aliases.** objdump mnemonics like `mov`, `cmp`, `tst`, `lsl`, `cset`
  are aliases; the classifier groups by encoding, so `mov x0, x1` counts
  under `dp-reg-logical` (ORR) and `cmp x0, #1` under `dp-imm-addsub`
  (SUBS-imm). This is intentional — lift works on encodings.

## Reproducing

```bash
python3 docs/guest-boot-opcode-histogram.py --self-test   # 107/107 required
python3 docs/guest-boot-opcode-histogram.py --kernel /mnt/sdb1/aosp/Image
```

Cross-checks used: `aarch64-linux-gnu-objdump -b binary -m aarch64 -D
--stop-address=0x1000000` (3,545,402 insns, 1418 distinct mnemonics) and a
354,759-word (word, mnemonic) confusion sample. Scratch artifacts from the
audit (`/tmp/audit/hist.py`, `/tmp/audit/sample.txt`) are superseded by the
in-repo script above.
