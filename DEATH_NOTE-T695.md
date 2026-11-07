# DEATH NOTE — T695

Lane: Track T695 — Re-flight on the Route A Image (Mac, agyx route)
Status: CLOSED — FAIL (Route A kernel panicked at guest 0.113s; empty initcall table under Clang 21 ThinLTO)
UTC close: 2026-11-07T22:05:00Z
Worktree: /Users/Shared/wt-track695-reflight-routea-agyx
Branch: t695 (from T694 commit e3e71ec)

## What ended

The lane was opened to confirm the pre-boot symbol gate on the Route A kernel
(T689 build, config-only LTO restore per T688 rebuild recipe) and re-fly it on
the Mac QEMU 11.1.2 harness.

The pre-boot symbol gate CONFIRMED PASS:
- `printk`: 0xffffff800811a4c4 (strictly below `_etext` 0xffffff8008b70000)
- `__die`: 0xffffff8008096bbc (strictly below `_etext` 0xffffff8008b70000)
- `panic`: 0xffffff80080b7770 (strictly below `_etext` 0xffffff8008b70000)
- `dump_stack`: 0xffffff8008b41a64 (strictly below `_etext` 0xffffff8008b70000)
- `show_regs`: 0xffffff800808ec28 (strictly below `_etext` 0xffffff8008b70000)
Init range [0xffffff80091c0000, 0xffffff80092b0000) contained none of the five.

However, the flight (Step 3, `t695-flight-01`) immediately FAILED at guest 0.113 s.
Route A does NOT fix the boot wall. While ThinLTO moved the text of cold functions
below `_etext`, it introduced a catastrophic compiler/linker section-naming defect:
Clang 21 ThinLTO suffixes all `.initcall*.init` section names with internal identifiers
(e.g., `.initcallrootfs.init..85_655_populate_rootfs`, `.initcall6.init..232_1153_inet6_init`).
Linux 4.19's `include/asm-generic/vmlinux.lds.h` matches only exact section names
(`KEEP(*(.initcall##level##.init))`) with no wildcard. Consequently, LLD orphan-placed
all 780+ initcall sections into `.data`, leaving the kernel's initcall table completely
empty:
`__initcall_start == __initcall_end == 0xffffff8009293c48` (0 bytes, 0 entries).

At runtime:
1. `do_initcalls()` executed zero initcalls.
2. No device drivers were registered (no PL011 UART console, no virtio-blk, no simplefb).
3. `populate_rootfs` never ran, so the 958 MB initramfs was never unpacked into rootfs.
4. `kernel_init_freeable()` failed to open `/dev/console` (`Warning: unable to open an initial console.`), failed to find `/early-ramdisk`, and fell back to `prepare_namespace()` -> `mount_root()` -> `mount_block_root()` -> `printk_all_partitions()` -> `class_dev_iter_next()` -> `klist_next()`.
5. In `klist_next()`, reading an uninitialized driver model pointer triggered a Data Abort (kernel oops 96000005) reading unreadable address `0x0000000000000100`.
6. Kernel panic: `Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b` at guest 0.113 s; QEMU rebooted and exited.

First failing link: Clang 21 ThinLTO section-suffixing (`.initcall*.init..<suffix>`)
orphaned all initcall sections into `.data`, leaving `__initcall_start == __initcall_end == 0xffffff8009293c48`
(empty initcall table, 0 initcalls executed) → populate_rootfs never ran → fallback
`prepare_namespace()` faulted on uninitialized block driver class (Oops 96000005 at 0x100 in `klist_next`) →
kernel panic `Attempted to kill init! exitcode=0x0000000b` at guest 0.113 s.

The A/B control (Step 4, `t695-flight-ab`) with the canonical Image (`05c43917…`) on the
identical Mac QEMU 11.1.2 harness booted cleanly to userspace:
- `Freeing unused kernel memory: 1024K` at guest 26.321218 s
- `init: init first stage started!` at guest 29.496160 s
- `init: init second stage started!` at guest 35.754196 s
- Ran to target guest 242.682 s (host 245.52 s, QEMU exit 0, 12157 lines, UART SHA `efe4da6d…`) with zero panic/oops.
This proves that the Mac QEMU harness, DTB, initramfs, and system image are 100% sound.
The failure is 100% guest kernel (Route A Image).

## Final artifacts

- `findings-t695.md` — full adjudication, root cause analysis, and evidence.
- `outcome-t695.txt` — single word `FAIL`.
- `progress-t695.txt` — Steps 0–4 completion log.
- `heartbeat.json` — updated timestamp and status.
- `scripts/supervisor_t695.py` and `scripts/launch_t695.py` — adapted flight supervisor and detach launcher.
- `diagnostics/`:
  - `t695-nm-symbols.txt` (symbol gate addresses)
  - `t695-routeA-empty-initcall-table.txt` (side-by-side nm comparison: Route A empty table vs T686 populated table)
  - `t695-routeA-readelf-sections.txt` (llvm-readelf showing `.initcall*.init..<suffix>` section names)
  - `t695-linker-script-initcalls.txt` (excerpt from `include/asm-generic/vmlinux.lds.h` showing exact match macros)
  - `t695-routeA-panic-uart.log` (105-line UART log of the Route A panic at 0.113 s)
  - `t695-canonical-ab-uart.log` (12157-line UART log of canonical A/B control boot)
- `flights/t695-flight-01/` — Route A flight end record, UART logs, preflight.
- `flights/t695-flight-ab/` — Canonical A/B flight end record, UART logs, preflight.

## What is not claimed

- No `sys.boot_completed=1`: Route A panicked before userspace; canonical A/B boot observed empty `boot_completed=` across all cycles up to 242.68 s.
- No frame commit: Route A never initialized simplefb (`fb0: simplefb registered!` never appeared). Canonical kernel has no `CONFIG_FB`.
- TEN-PASSES: NOT run (Route A failed on flight 1).
- No kernel rebuild performed in this lane: Route B rebuild belongs to the next lane.

## Handoff to Next Lane (Route B)

Route A (config-only LTO restore) is definitively DEAD.
ThinLTO under Clang 21 cannot be used without linker script adjustments because it mangles section names.

The next lane MUST execute Route B from `rebuild-recipe.md`:
1. Modify `include/asm-generic/vmlinux.lds.h` to add wildcards:
   - In `TEXT_TEXT`: add `*(.text.unlikely .text.unlikely.*)` and `*(.text.hot .text.hot.*)`. This fixes the T686 orphan defect directly without LTO.
   - If LTO is ever desired in the future, `INIT_CALLS_LEVEL` would also need `KEEP(*(.initcall##level##.init*))`.
   - Therefore, the cleanest and most robust fix is Route B: keep `CONFIG_LTO_NONE=y` (non-LTO, where initcalls are verified populated across 0xffffff800924ab48 to 0xffffff800924b65c) and add `.text.unlikely.*` to `TEXT_TEXT`.
2. Rebuild the kernel on the box using the exact command in `rebuild-recipe.md`.
3. Pre-boot gate on the Route B vmlinux:
   - Check that printk, __die, panic, dump_stack, show_regs are all below `_etext`.
   - Check that `__initcall_start != __initcall_end` (populated initcall table).
4. Re-fly on the proven Mac QEMU harness.
