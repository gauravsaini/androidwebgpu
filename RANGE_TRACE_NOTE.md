# Range-Trace Investigation Note

## Summary

Register-level trace of `__next_free_mem_range_rev` found the mis-executed instruction behind the memblock "Failed to allocate" panic. **Root cause: emulator bug in ADD/SUB (shifted register).**

**Date:** 2026-10-04  
**Worktree:** ~/workspace/wt-range-trace, branch feat/range-trace  
**Status:** Root cause found, fix verified, regression test added.

## Phase 1: Failing call identification

The failing allocation is BL to `0xffffff8008353cc4` at step 20572038, called from `0xffffff800946b87c`, args x0=`0x1000` (size), x1=`0x1000` (align), x2=`0x0` (max_addr). It calls the iterator `0xffffff800835503c` 4 times, then returns 0; panic follows.

## Phase 2: Instruction trace of the failing call

The iterator (`__next_mem_range_rev`-shaped, 8 args) correctly computes:
- `*out_start = 0x480003eb`, `*out_end = 0x80000000` (first free range)

The caller reads these correctly via LDP. The bug is in the caller's alignment mask computation.

## Hand computation vs trace

Reference model: `/tmp/ref_model.py`.

Expected with verified data (mem cnt=1 [0x40000000,0x80000000), reserved [0x48000000,0x480003eb)):
- `*out_start = 0x480003eb`, `*out_end = 0x80000000`
- `cand = round_down(0x80000000 - 0x1000, 0x1000) = 0x7ffff000` >= `this_start` → should return 0x7ffff000.

**Divergence at step 20572169:**
- PC: 0xffffff8008353e6c, word: 0xcb1703f7 (`sub x23, xzr, x23`, NEG idiom)
- Before: x23 = 0x1000, SP = 0xffffff8009543e50
- Expected: x23 = 0 - 0x1000 = 0xfffffffffffff000
- Actual: x23 = SP - 0x1000 = 0xffffff8009542e50 ← **BUG**

The corrupted mask causes `and x0, x9, x23` to produce 0x9542000 instead of 0x7ffff000, which fails the `cmp` against this_start, causing 4 futile loop iterations and return 0 → panic.

## Suspect instruction

**`sub x23, xzr, x23` (0xcb1703f7) at 0xffffff8008353e6c**

The emulator's ADD/SUB (shifted register) handler with S=0 incorrectly treated Rn=31 as SP. Per ARM DDI 0487, the shifted-register form NEVER uses SP — Rn=31/Rd=31 is always XZR. (Only the immediate form uses SP.)

## Fix

In `units/u12-orchestrator/src/lib.rs`, changed the ADD/SUB (shifted register) S=0 handler:
- **Before:** `rn_val = if rn == 31 { sp } else { regs[rn] }`; `if rd == 31 { sp = res }`
- **After:** `rn_val = if rn == 31 { 0 } else { regs[rn] }`; `if rd != 31 { regs[rd] = res }`

## Verification (mandatory RAM search)

- [x] Full-boot RAM search for "Failed to allocate 0x1000 bytes below 0x0" — ABSENT ✓
- [x] Full-boot RAM search for "Kernel panic - not syncing: Failed to allocate" — ABSENT ✓
- [x] Booted 20,610,084 steps (past the 20,580,000 panic point) — kernel proceeds past memblock allocation.

**Result: RAM SEARCH CLEAN — panic is fixed.** The kernel halts later at step 20,610,084 on an unrelated SP-relative load/store issue (separate bug, not the memblock panic).

## Process fix (testgen)

Added `test_addsub_shifted_reg_rn31_is_xzr` in `units/u12-orchestrator/tests/testgen_vectors.rs` with 3 vectors:
1. `sub x23, xzr, x23` (0xcb1703f7) — the exact failing instruction
2. `add x0, xzr, x1` — Rn=31 as XZR source  
3. `sub xzr, x0, x1` — Rd=31 discards (not SP)

All pass. The instruction class cannot regress.
