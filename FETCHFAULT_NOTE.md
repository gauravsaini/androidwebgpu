# FetchFault at 0xffffff8009456158 — UXN vs PXN Root Cause (u4-mmu)

## Summary

After the SP-relative LDPSW fix (ea3ad6c), the kernel halted at **step
20,738,874** with `FetchFault { addr: 0xffffff8009456158 }` — a fetch from
a VA the emulator claimed was unmapped. Root cause was an **emulator MMU
permission bug, not a guest page-table problem**: u4-mmu treated **UXN
(bit 54) as denying EL1 execute**. AArch64 Linux maps all kernel text
`UXN=1,PXN=0` (UXN only restricts EL0), so once the final page tables went
live, every kernel-text fetch faulted. Fixed by denying `Access::Execute`
on **PXN (bit 53) / PXNTable (bit 59) only**. Full-boot verification:
**29,205,843 steps** (8.5M past the old halt), RAM search clean for both
panic strings.

**Date:** 2026-10-04
**Worktree:** ~/workspace/wt-fetchfault, branch feat/fetchfault (base ea3ad6c)
**Status:** Fixed, unit-tested, full-boot RAM-search verified.

## The faulting instruction

- **VA:** 0xffffff8009456158 (fetch VA; MMU on, TTBR1 region)
- **Word:** 0xf944a708 (file offset 0x13d6158 in cached Image; VA→file
  mapping calibrated against the SP_LOADSTORE_NOTE reference
  VA 0xffffff8008354c10 ↔ offset 0x2D4C10)
- **Decoded:** `ldr x8, [x24, #2376]` — ordinary kernel text, mid-function.

## Diagnosis: the VA *was* mapped

A debug boot (`units/u12-orchestrator/tests/fetchfault_debug.rs`) dumped
MMU state at the halt and walked the page tables by hand:

- `sctlr_el1 = 0x34f5d91d` (MMU on), `tcr_el1` → TG1=4K, T1SZ=25 (39-bit IA)
- `ttbr1_el1 = 0x7efff000`
- L1[0x0] @0x7efff000 = `0x7effe003` (table) →
  L2[0x4a] @0x7effe250 = `0x7effb003` (table) →
  L3[0x56] @0x7effb2b0 = `0x0050000041456793` (page) → **PA 0x41456158**

The walk succeeds. The leaf descriptor decodes to **UXN=1 (bit 54),
PXN=0 (bit 53)** — exactly how Linux maps kernel text
(`PAGE_KERNEL_EXEC`). The emulator's `finish_block` denied
`Access::Execute` on `xn || pxn`, so the fetch faulted on UXN alone.

Why it booted 20.7M steps first: the early boot ran under the initial
page tables; the fault appeared only after the kernel switched to its
final tables carrying the standard UXN=1 text mappings.

## Fix (`units/u4-mmu/src/lib.rs`)

Architectural correction, not a quirk — the model is documented as
EL1-equivalent, and UXN restricts EL0 only (ARM ARM):

- `finish_block`: `Access::Execute` denied by **PXN (bit 53)** or the
  accumulated `no_exec` only; UXN (bit 54) no longer denies execute.
- Table-descriptor walk: only **PXNTable (bit 59)** accumulates into
  `no_exec`; UXNTable/XNTable (bit 60) is ignored (EL0-only).
- Module docs updated to state the corrected model.

## Tests

u4-mmu suite: **20 passed, 0 failed**, including:

- `golden_execute_allowed_on_uxn_page` (new) — UXN=1,PXN=0 allows
  `Access::Execute`. Regression test for this exact fault.
- `golden_permission_fault_execute_pxn_page` (new) — PXN=1 still denies
  execute; reads unaffected.
- `golden_uxntable_does_not_deny_execute` (new) — bit 60 on a table
  descriptor does not deny execute.
- `golden_permission_fault_execute_pxn_table_accumulates` (kept) — bit 59
  still denies execute below.
- (The old `golden_permission_fault_execute_xn_page` encoded the wrong
  behavior and was replaced.)

u12-orchestrator lib suite: **120 passed, 0 failed** (5 known RAM-heavy
snapshot tests skipped — pre-existing OOM exclusions, unrelated).

## Verification (mandatory RAM search)

`units/u12-orchestrator/tests/fetchfault_boot_verify.rs`:

- Booted **29,205,843 steps** — 8.5M past the 20,738,874 FetchFault halt.
- New halt: `WasmTrap { addr: 0xffffffbefea00000, message: "mmu_fault:
  TranslationFault { va: 0xffffffbefea00000 }" }` — a *later, different*
  halt (data-access translation fault at a vmalloc-region VA; next track).
- RAM search for `Failed to allocate 0x` — **ABSENT**
- RAM search for `Kernel panic - not syncing: Failed` — **ABSENT**

**Result: RAM SEARCH CLEAN — no regression, fix verified.**

## Open next

The TranslationFault at VA 0xffffffbefea00000 (data access, vmalloc
region `0xffffffbe...`) is the next halt to chase — likely a genuinely
unmapped VA (bad pointer or next page-table gap), not a permission-model
issue.
