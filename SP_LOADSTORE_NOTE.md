# SP-Relative Load/Store Halt Note (LDPSW)

## Summary

After the memblock SUB fix (6da41b8), the kernel halted at **step 20,610,084**
on `ldpsw x9, x10, [sp]` — LDPSW (load pair, signed word) was excluded from
the u12 fast-path pair arm, and U2 traps all SP-relative pairs. Fixed by
handling LDPSW in the fast path with correct sign-extension. Full-boot RAM
search is clean; the kernel now reaches step 20,738,874 and a new, later halt.

**Date:** 2026-10-04
**Worktree:** ~/workspace/wt-sp-loadstore, branch feat/sp-loadstore (base 6da41b8)
**Status:** Fixed, unit-tested, full-boot RAM-search verified.

## The failing instruction

- **PC:** 0xffffff8008354c10 (halt trap address 18446743524091448336)
- **Word:** 0x69402BE9 (read from cached Image at file offset 0x2D4C10;
  VA→PA via the kernel's own page-table offset, verified against the
  memblock VA→PA pair from the range-trace track)
- **Decoded (capstone):** `ldpsw x9, x10, [sp]` — load pair signed word,
  SP-relative, signed offset 0

## Root cause

Two layers conspired:

1. **u12 fast path** (`units/u12-orchestrator/src/lib.rs`, pair arm): the
   `is_sp_pair` predicate accepted only opc 00 (32-bit pair) and 10 (64-bit
   pair). opc 01 (LDPSW) was explicitly excluded with the comment
   "(01 = LDPSW: not handled here, falls through to U2)".
2. **U2 IR lifter** (`units/u2-ir-lift/src/lib.rs`): lifts LDPSW fine for
   non-SP bases, but traps **every** Rn=31 pair with
   `R_LS_SP = "LoadStore: SP-relative address is not expressible
   (no SP in the Wave-4 register file)"`.

So SP-relative LDPSW had no handler anywhere: fast path skipped it, U2
trapped it. SP itself was never the problem — the machine state already
carries `cpu[0].sp` and the fast path uses it for all other SP-relative
forms (STR/LDR imm, LDUR/STUR, pre/post-index, STP/LDP pairs).

## Fix

In `units/u12-orchestrator/src/lib.rs` (pair arm):

- `is_sp_pair` now also matches `opc == 0b01 && is_load` (LDPSW is
  load-only; opc=01 with L=0 is an unallocated encoding and still falls
  through to U2's unsupported trap).
- New `is_ldpsw` load branch: loads two 32-bit words and **sign-extends**
  each to 64 bits (`i32 as i64 as u64`), unlike plain LDP which
  zero-extends. Rt=31 (XZR) still discards. Scale stays 4, and the
  existing pre/post-index SP writeback applies unchanged.

## Tests

New unit tests in `units/u12-orchestrator/src/lib.rs` (existing
`sp_test_orchestrator` pattern, MMU-off identity map):

- `gb27_sp_ldpsw_sign_extends` — the exact kernel halt word 0x69402BE9:
  `ldpsw x9, x10, [sp]`; RAM holds 0xFFFFFFFF (-1) and 0x7FFFFFFF;
  asserts x9 = 0xFFFFFFFFFFFFFFFF (sign-extended), x10 = 0x7FFFFFFF,
  SP/PC correct.
- `gb27_sp_ldpsw_preindex_writeback` — `ldpsw x0, x1, [sp, #-16]!`
  (0x69FE07E0, capstone-verified; my first hand-assembly 0x69DF43E0 was
  wrong and capstone caught it): asserts sign-extension, SP decremented
  by 16, load from the new SP.

Full lib suite: **120 passed, 0 failed** (5 known RAM-heavy snapshot
tests skipped — pre-existing OOM exclusions, unrelated).

## Verification (mandatory RAM search)

`units/u12-orchestrator/tests/sp_ldpsw_boot_verify.rs`:

- Booted **20,738,874 steps** — past the 20,610,084 LDPSW halt point.
- New halt: `Halted(FetchFault { addr: 0xffffff8009456158 })` — a later,
  different halt (separate issue, next track).
- RAM search for `Failed to allocate 0x` — **ABSENT**
- RAM search for `Kernel panic - not syncing: Failed` — **ABSENT**

**Result: RAM SEARCH CLEAN — no regression.** (First run of this test used
over-broad needles and matched the printk *format* strings
`"Failed to allocate %llu..."` / `"Kernel panic - not syncing: %s"` in
.rodata — false positives. Needles tightened to the formatted-message
forms, per the range-trace convention.)

## Process note

Mid-edit I fat-fingered an `muse.edit` that spliced the LDPSW block into
the ADD/SUB-immediate handler (87 identical `} else {` matches). Caught
via `git diff`, restored exactly, re-verified the final diff is surgical.
Lesson: use line-anchored context for edits in this file.

## Open next

The FetchFault at 0xffffff8009456158 is the next halt to chase (fetch from
an unmapped VA — likely a bad branch target or page-table gap, not a
load/store issue).
