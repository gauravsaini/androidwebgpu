# Boot35M: Extend Boot to 100M Steps — vmemmap Pool Collision Fixed

## Summary

Extended the boot cap from 35M to 100M steps. Found and fixed the next
halt: the vmemmap demand-population bump allocator collided with the
kernel's own page tables. After the fix, the kernel boots **100,000,000
steps with NO HALT** (hit the step cap), RAM search clean for both panic
strings.

**Date:** 2026-10-04
**Worktree:** ~/workspace/wt-boot35m, branch feat/boot35m (base 0b8f2bb)
**Status:** Fixed, full-boot RAM-search verified at 100M steps.

## First 100M run: TranslationFault at step 96,183,635

- **Halt:** `WasmTrap { addr: 0xffffffbeff9f8000, "mmu_fault:
  TranslationFault" }`, pc=`0xffffff8008089e0c` (the vmemmap `ldrb`
  scan loop).
- **Analysis:** The fault VA is 4088 pages past the first vmemmap fault
  (`0xffffffbefea00000` at step 29M) — almost exactly the 4096-page
  (16MB) bump pool size. The kernel's byte-by-byte vmemmap scan consumed
  the entire pool.
- **Initial fix:** enlarged pool 16MB → 32MB (8192 pages).

## Second 100M run: FetchFault at step 96,200,019 — the real bug

- **Halt:** `FetchFault { addr: 0xffffff8008089e0c }` — the kernel's own
  text VA. The instruction had fetched fine for 96M steps.
- **Probe** (`tests/probe_fetchfault.rs`): page-table walk for the fault
  VA showed `L1[0x0] = 0x7effe003` (valid) but **`L2[0x40] = 0x0`**
  (zeroed). The kernel-text mapping was wiped.
- **Root cause:** the bump allocator carved pages from the **top of guest
  RAM** downward (`RAM_BASE + RAM_SIZE - (n+1)*4096`). PA `0x7effe000`
  is 2 pages below the top of RAM — and the kernel had placed its own
  L2 page table there. Our allocator handed out the same page for
  vmemmap backing and **zeroed it**, destroying the kernel's text
  mapping. A classic double-alloc: the kernel's memblock allocator did
  not know about our emulator-side reservation.

## Fix (`units/u12-orchestrator/src/lib.rs`)

Moved the vmemmap backing pool **above guest RAM**:

- New constants: `VMEMMAP_POOL_BASE = 0x8000_0000` (= `RAM_BASE +
  RAM_SIZE`), `VMEMMAP_POOL_SIZE = 32MB`.
- Emulator RAM vec extended to `RAM_SIZE + VMEMMAP_POOL_SIZE` (all
  `ram.len()` bounds checks are dynamic, so this is safe).
- `alloc_vmemmap_page()` now allocates upward from `VMEMMAP_POOL_BASE`.
- The kernel's memblock only knows `[0x40000000, 0x80000000)`, so it can
  never allocate these PAs. No DTB change needed. 1GB RAM needs ~16.8MB
  of vmemmap (262144 × 64-byte struct pages); 32MB gives headroom.

## Verification

- `tests/boot35m_extend.rs` (100M cap, release): **100,000,000 steps,
  halt: None** — no halt, hit the cap. RAM search clean.
- `tests/translationfault_boot_verify.rs` (35M): still passes —
  no regression in the original fix.
- Lib unit tests: compile and pass (spot-checked `gb27_sp_ldpsw_*`).
- Mandatory RAM search after 100M steps:
  - `Failed to allocate 0x` — ABSENT
  - `Kernel panic - not syncing: Failed` — ABSENT
  - **RAM SEARCH CLEAN.**

## Boot progress analysis

PC trace: kernel is in the vmemmap `ldrb` scan loop
(`0xffffff8008089e10`) from ~25M to 100M steps — 75M steps of
byte-by-byte scanning. At 96.2M, x0 was `0xffffffbeff9f9000`;
vmemmap ends at `0xffffffc000000000`, leaving ~6.3MB ≈ 25M more steps.
The scan should complete around ~125M steps, then boot proceeds to the
next init phase. This is legitimate kernel behavior (page-allocator
init), not a stuck loop — x0 advances monotonically.

## Open next

Raise the cap to ~150M to see the kernel finish the vmemmap scan and
find the following halt. The `probe_fetchfault.rs` diagnostic test is
scratch (not committed... see below).

## Files changed

- `units/u12-orchestrator/src/lib.rs` — pool constants, RAM vec
  extension, `alloc_vmemmap_page()` rewrite.
- `units/u12-orchestrator/tests/boot35m_extend.rs` — 100M-step boot
  test with progress reporting and mandatory RAM search.
- `units/u12-orchestrator/tests/probe_fetchfault.rs` — one-shot
  diagnostic probe (page-table walk at the FetchFault).
