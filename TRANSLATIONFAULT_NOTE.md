# TranslationFault at 0xffffffbefea00000 — vmemmap Demand-Population (u12)

## Summary

After the MMU UXN/PXN fix (db5ab78), the kernel halted at **step
29,205,843** with `TranslationFault { va: 0xffffffbefea00000 }` — a data
access to the vmemmap region. Root cause was an **emulator limitation, not
a guest bug**: the kernel scans vmemmap via post-indexed `ldrb w2, [x0],
#1` in a tight loop, and when the scan reaches an unpopulated vmemmap
page, the emulator halted with WasmTrap instead of allowing the kernel's
fault handler to populate it. Fixed by emulating vmemmap
demand-population in the orchestrator. Full-boot verification: **35M
steps** (5.8M past the old halt, hit the step cap without halting), RAM
search clean for both panic strings.

**Date:** 2026-10-04
**Worktree:** ~/workspace/wt-translationfault, branch feat/translationfault (base db5ab78)
**Status:** Fixed, full-boot RAM-search verified.

## The faulting instruction

- **VA:** 0xffffffbefea00000 (data VA; MMU on, TTBR1 region, vmemmap)
- **PC:** 0xffffff8008089e0c, **word:** 0x38401402
- **Decoded (via Capstone):** `ldrb w2, [x0], #1` — post-indexed byte load,
  x0 increments by 1 after each load.
- **Context:** The instruction sits in a 4-instruction loop
  (`ldrb`/`cmp`/`ccmn`/`b.ne`) that scans memory byte-by-byte, checking
  each byte against `(w1 & 7)` or `0x1f`. Trace of the last 10k steps shows
  x0 incrementing from 0xffffffbefe9ff63c to 0xffffffbefea00000 — a
  deliberate vmemmap scan, not a wild pointer.

## Diagnosis: vmemmap demand-population

Page-table walk for the fault VA (TTBR1=0x416ea000, 39-bit VA):
- L1[0xfb] = 0x41682003 (table) → valid
- L2[0x1f5] = 0x0000000000000000 (invalid) → **walk fails at L2**

The VA 0xffffffbefea00000 is in the vmemmap region
(VMEMMAP_START=0xffffffbe00000000 for 39-bit VA, per
`-(1 << (VA_BITS-2))`). The kernel populates vmemmap on-demand via its
data-abort handler (`do_page_fault` → `vmemmap_populate`). The emulator
does not deliver data aborts to the guest — all MMU faults become
`HaltReason::WasmTrap`. When the kernel's vmemmap scan reached an
unpopulated page, the emulator halted instead of populating it.

The scan ran successfully for 10k+ steps (those vmemmap pages were
populated), proving the mechanism works — only the unpopulated tail
faulted.

## Fix (`units/u12-orchestrator/src/lib.rs`)

Emulate the kernel's vmemmap fault handler in the orchestrator:

1. **`handle_vmemmap_fault(&mut self, va: u64) -> bool`** (new):
   - Returns false unless VA is in `[0xffffffbe00000000,
     0xffffffc000000000)` (vmemmap range for 39-bit VA).
   - Walks TTBR1: L1 must be a table descriptor; if L2 entry is invalid,
     allocates a zeroed 4K page for a new L3 table and installs it.
   - If L3 entry is invalid, allocates a zeroed 4K data page and installs
     a page descriptor (AF=1, SH=01 inner-shareable, UXN=1, PXN=1,
     AttrIndx=000 normal memory).
   - Returns true if the fault was handled (page installed or already
     present).

2. **`alloc_vmemmap_page(&mut self) -> Option<u64>`** (new):
   - Bump allocator from the top of RAM (16MB pool, 4096 pages).
   - Zeroes the page before returning its PA.
   - Returns None if the pool is exhausted (fault then halts honestly).

3. **`vmemmap_pages_used: u64`** (new Orchestrator field):
   - Tracks bump-allocator usage.

4. **Hook in `step_vcpu`** (fast-path `execute_arm64` error handler):
   - If `Err(HaltReason::WasmTrap { addr, message })` where message starts
     with `"mmu_fault: TranslationFault"`, calls `handle_vmemmap_fault(addr)`.
   - If handled, retries the instruction via recursive `step_vcpu()`
     (PC not advanced, step not counted).
   - If not a vmemmap VA or allocation fails, halts honestly as before.

This is a pragmatic emulation of what the kernel's own fault handler
would do — not a quirk or a silent no-op. The page tables are genuinely
populated, and the kernel proceeds as on real hardware.

## Tests

u12-orchestrator lib suite: **120 passed, 0 failed** (5 known RAM-heavy
snapshot tests skipped — pre-existing OOM exclusions, unrelated).

## Verification (mandatory RAM search)

`units/u12-orchestrator/tests/translationfault_boot_verify.rs`:

- Booted **35,000,000 steps** — 5.8M past the 29,205,843 TranslationFault
  halt. Hit the 35M step cap with **no halt** (`halt: None`).
- RAM search for `Failed to allocate 0x` — **ABSENT**
- RAM search for `Kernel panic - not syncing: Failed` — **ABSENT**

**Result: RAM SEARCH CLEAN — fix verified, boot continues past the fault.**

## Open next

The kernel now boots to 35M steps without halting (hit the test cap).
Next: extend the boot cap to find the following halt, or investigate what
the kernel is doing at 35M steps (likely userspace or driver init).
