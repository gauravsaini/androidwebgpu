# Findings — Track T695: Re-flight on the Route A Image (Mac, agyx route)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track695-reflight-routea-agyx`
Branch: `t695`, from `e3e71ec` (the T694 commit)
Predecessor: T694 driver-certified honest FAIL (opencode launcher path failure; route moved to Mac AGYX).

## Verdict

Verdict: FAIL

## Required markers

sys.boot_completed: not observed (0 / empty) — no `sys.boot_completed=1` appeared in any flight in this lane. The Route A kernel panicked at guest 0.113 s before reaching userspace. The canonical A/B control flight (`t695-flight-ab`) reached init first and second stage and ran to guest 242.682 s; its periodic `T642_BOOT_STATE` lines at guest uptimes 148.95 s, 176.09 s, 199.54 s, and 224.33 s all reported `boot_completed=` empty. No cycle in this lane observed `sys.boot_completed=1`.

WALL-TYPE-TAG: guest type-1 — Route A kernel panicked at guest 0.113s; empty initcall table under Clang 21 ThinLTO
Rust par bhi lagu: haan — the same guest kernel is required regardless of implementation language; this is a guest kernel compiler/linker section-placement wall, not a harness or language runtime wall.

Freeing unused kernel memory:
- Flight `t695-flight-01` (Route A kernel `11adf236…`): NOT observed (guest panicked at 0.113 s, QEMU exited at host 10.02 s).
- Flight `t695-flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 26.321218 s (`[   26.321218] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `t695-flight-01` (Route A kernel `11adf236…`): NOT observed.
- Flight `t695-flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 29.496160 s (`[   29.496160] init: init first stage started!`).

## First failing links

### First failing link of T686 (rebuilt kernel predecessor, restated)
Cold `.text.unlikely.` orphaned into `[__init_begin, __init_end]` → `free_initmem()` zeroes that range → `bl printk` at `free_reserved_area+420` lands on `udf #0x0` → nested `die()` self-deadlocks on `die_lock` at 0xffffff80093b514c value 0x101 on CPU0.

### First failing link of Route A (T695 subject kernel)
Clang 21 ThinLTO section-suffixing (`.initcall*.init..<id>_<counter>_<fn>`) orphaned all 780+ initcall sections into `.data`, leaving `__initcall_start == __initcall_end == 0xffffff8009293c48` (initcall table length 0, 0 initcalls executed) → zero drivers registered and `populate_rootfs` never ran → `kernel_init_freeable()` failed to open `/dev/console` and could not find `/early-ramdisk` on empty rootfs → fallback `prepare_namespace()` faulted on uninitialized block driver class (Data Abort, Oops 96000005 reading unreadable address `0x0000000000000100` in `klist_next+0x20/0xc4`) → `Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b` at guest 0.113 s.

## Executive Summary

Track T695 tested the Route A kernel Image (`11adf236…`, built in T689 via config-only LTO restore: `CONFIG_LTO=y`, `CONFIG_THINLTO=y`, `CONFIG_LTO_CLANG=y`) on the proven Mac QEMU 11.1.2 harness.

1. **Pre-boot symbol gate (Step 2)**: CONFIRMED PASS. The driver-certified addresses were verified verbatim via `llvm-nm` on `/home/muse/t689-rebuild/src/vmlinux` via box SSH:
   - `printk`: 0xffffff800811a4c4
   - `__die`: 0xffffff8008096bbc
   - `panic`: 0xffffff80080b7770
   - `dump_stack`: 0xffffff8008b41a64
   - `show_regs`: 0xffffff800808ec28
   - `_etext`: 0xffffff8008b70000
   - `__init_begin`: 0xffffff80091c0000
   - `__init_end`: 0xffffff80092b0000
   All five critical functions are strictly below `_etext` 0xffffff8008b70000. In terms of function text, ThinLTO did move cold code out of the init range.

2. **Re-flight (Step 3)**: IMMEDIATE FAIL. Flown with `modified.dtb` (`e83a6437…`), `base-t683-fix.cpio.gz` (`8fb3c9de…`), and `system.img` (`98fc0b02…`) under QEMU 11.1.2. The Route A kernel did not hang at ~25 s like T686; instead, it failed much earlier at guest 0.113 s with a Data Abort Oops and panic:
   `Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b`

3. **Root Cause Analysis of Route A**:
   Investigation of section headers in `src/vmlinux` via `llvm-readelf -S` revealed that Clang 21 with `CONFIG_THINLTO=y` emits `.initcall` sections with internal unique suffixes:
   `.initcallrootfs.init..85_655_populate_rootfs`
   `.initcall5.init..79_74_create_debug_debugfs_entry`
   `.initcall6.init..232_1153_inet6_init`
   Linux 4.19's `include/asm-generic/vmlinux.lds.h` defines:
   `#define INIT_CALLS_LEVEL(level) KEEP(*(.initcall##level##.init))`
   Because the macro matches exact names without wildcards, LLD orphan-placed every single initcall section into `.data` (at addresses `0xffffff80093ee810`–`0xffffff80093f06f8`). The linker symbols `__initcall_start`, `__initcall0_start`, ..., `__initcall7_start`, and `__initcall_end` all collapsed to `0xffffff8009293c48`.
   The initcall table was 100% empty. At boot, `do_initcalls()` called zero initcalls. No serial driver, no block driver, and no framebuffer driver initialized. `populate_rootfs` never executed, so the 958 MB initramfs was never unpacked. `kernel_init_freeable` failed to open `/dev/console`, fell through to `prepare_namespace()` -> `mount_block_root()` -> `printk_all_partitions()`, where dereferencing an uninitialized `klist` pointer caused a Data Abort at `0x0000000000000100` and killed init.

4. **A/B Control (Step 4)**: PASS. Booting the canonical Image (`05c43917…`) with the identical DTB, initramfs, system image, supervisor, and QEMU 11.1.2 shape on the same Mac reached `Freeing unused kernel memory: 1024K` at guest 26.321218 s, `init: init first stage started!` at guest 29.496160 s, `init: init second stage started!` at guest 35.754196 s, and ran cleanly to guest 242.682 s (host 245.52 s, exit 0, 12157 lines).
   This proves the Mac flight harness is completely sound.

5. **Decision & Handoff**: Route A (config-only LTO restore) is definitively DEAD. ThinLTO under Clang 21 cannot be used on Linux 4.19 without extensive linker-script section-matching modifications. The successor lane MUST proceed to Route B (non-LTO with linker script wildcards `*(.text.unlikely .text.unlikely.*)` in `TEXT_TEXT`).

---

## Detailed Step Records

### Step 0 — Mac disk gate: PASS

Observed reading from `df -h /System/Volumes/Data`:
```
Filesystem      Size    Used   Avail Capacity iused ifree %iused  Mounted on
/dev/disk3s5   228Gi   186Gi    14Gi    93%    3.2M  151M    2%   /System/Volumes/Data
```
Available space: 14 GiB, strictly above the 8 GiB threshold. No WAIT-DISK. Recorded in `progress-t695.txt` at 2026-11-07T21:48:33Z.

### Step 1 — Input Verification & T688 Record: PASS

All 8 staged inputs and required documents were SHA-verified in the worktree:
```
11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367  guest-inputs/routeA-Image
05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04  guest-inputs/Image
d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8  guest-inputs/extracted_kernel.config
e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389  guest-inputs/modified.dtb
8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70  guest-inputs/base-t683-fix.cpio.gz
98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d  guest-inputs/system.img
ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132  guest-inputs/android.hardware.graphics.composer@2.1-service
181a15807def6d9d2fd6f028b944342fc5578fc7043b5dcda9fc59bc599a1632  findings-t688.md
```
`findings-t688.md` and `rebuild-recipe.md` were read in full. No missing input or SHA mismatch. Recorded in `progress-t695.txt` at 2026-11-07T21:49:34Z.

### Step 2 — Confirm Pre-Boot Symbol Gate: CONFIRMED PASS

The gate was checked via:
1. Local SHA check of `guest-inputs/routeA-Image` = `11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367` (20,384,256 B).
2. Live re-run of `llvm-nm` on the Route A `vmlinux` on the box (`/home/muse/t689-rebuild/src/vmlinux`) via SSH:
   `ssh -i ~/.ssh/id_ed25519_remote muse@100.104.140.2 '/home/muse/t681-link/toolchain/android-ndk-r29/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-nm /home/muse/t689-rebuild/src/vmlinux | grep -E " (printk|__die|panic|dump_stack|show_regs|_etext|__init_begin|__init_end|free_initmem)$"'`

Verbatim addresses obtained:
```
ffffff8008096bbc t __die
ffffff80091c0000 R __init_begin
ffffff80092b0000 R __init_end
ffffff8008b70000 T _etext
ffffff8008b41a64 T dump_stack
ffffff80080ab440 T free_initmem
ffffff80080b7770 T panic
ffffff800811a4c4 T printk
ffffff800808ec28 T show_regs
```
Preserved at `diagnostics/t695-nm-symbols.txt` (SHA-256 `f4213dd9…`).

Symbol comparison:
- `_etext`: `0xffffff8008b70000`
- `__init_begin`: `0xffffff80091c0000`
- `__init_end`: `0xffffff80092b0000`
- `printk`: `0xffffff800811a4c4` < `_etext` (`0xffffff8008b70000`) — PASS
- `__die`: `0xffffff8008096bbc` < `_etext` (`0xffffff8008b70000`) — PASS
- `panic`: `0xffffff80080b7770` < `_etext` (`0xffffff8008b70000`) — PASS
- `dump_stack`: `0xffffff8008b41a64` < `_etext` (`0xffffff8008b70000`) — PASS
- `show_regs`: `0xffffff800808ec28` < `_etext` (`0xffffff8008b70000`) — PASS
- `free_initmem`: `0xffffff80080ab440` < `_etext` (`0xffffff8008b70000`) — PASS

All five critical functions are strictly below `_etext` (and below `__init_begin`).
The pre-boot symbol gate was CONFIRMED PASS. Recorded in `progress-t695.txt` at 2026-11-07T21:50:06Z.

### Step 3 — Re-flight on Route A Kernel: FAIL

Supervisor `scripts/supervisor_t695.py` executed flight `t695-flight-01`:
- Kernel: `guest-inputs/routeA-Image` (SHA-256 `11adf236…`)
- DTB: `guest-inputs/modified.dtb` (SHA-256 `e83a6437…`)
- Initramfs: `guest-inputs/base-t683-fix.cpio.gz` (SHA-256 `8fb3c9de…`) via loader at `0x110000000`
- System: `guest-inputs/system.img` (SHA-256 `98fc0b02…`)
- Machine shape: virt,gic-version=2,virtio-mmio-transports=1, -cpu cortex-a57, -m 8192, -smp 4
- Host elapsed: 10.02 s
- Guest max time: 0.114 s
- QEMU returncode: 0 (exited after kernel panic and reboot timer)
- UART line count: 105 lines
- UART SHA-256: `72740f1ee25fcba63b6453825213ee72847b1989c4bb14dfbb65b044838e0f24`
- Preserved at `flights/t695-flight-01/uart.log` and `diagnostics/t695-routeA-panic-uart.log`.

**Flight execution trace**:
At guest 0.000 s: kernel boots, announces:
`[    0.000000] Linux version 4.19.130-g4da740c10 (muse@gsai) (Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0, LLD 21.0.0) #2 SMP PREEMPT Sat Nov 7 13:40:51 AEDT 2026`
At guest 0.091 s: `CPU0: update max cpu_capacity 1024`
Then, immediately:
`[    0.100868] Warning: unable to open an initial console.`
`[    0.102618] Failed to create /dev/root: -2`
`[    0.102890] List of all partitions:`
`[    0.103400] Unable to handle kernel read from unreadable memory at virtual address 0000000000000100`
`[    0.103734] Mem abort info: ESR = 0x96000005, Exception class = DABT (current EL)`
`[    0.105550] Internal error: Oops: 96000005 [#1] PREEMPT SMP`
`[    0.106110] Process swapper/0 (pid: 1, stack limit = 0x(____ptrval____))`
`[    0.107126] pc : klist_next+0x20/0xc4`
`[    0.107280] lr : class_dev_iter_next+0x18/0x44`
`[    0.107442] sp : ffffff800800bc60`
`[    0.110540] Call trace:`
`[    0.110684]  klist_next+0x20/0xc4`
`[    0.110816]  class_dev_iter_next+0x18/0x44`
`[    0.110976]  printk_all_partitions+0x5c/0x230`
`[    0.111136]  mount_block_root+0xd0/0x1e8`
`[    0.111278]  mount_root+0x84/0x94`
`[    0.111400]  prepare_namespace+0x1bc/0x1c0`
`[    0.111548]  kernel_init_freeable+0x110/0x120`
`[    0.111704]  kernel_init+0x10/0x114`
`[    0.111836]  ret_from_fork+0x10/0x18`
`[    0.113236] Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b`
`[    0.113986] Rebooting in 5 seconds..`

Neither `Freeing unused kernel memory` nor `init: init first stage started!` nor `simplefb registered!` appeared.
Step 3 is an honest FAIL. Recorded in `progress-t695.txt` at 2026-11-07T21:57:30Z.

### Step 4 — A/B Control Flight: PASS

Supervisor `scripts/supervisor_t695.py` executed flight `t695-flight-ab`:
- Kernel: canonical `guest-inputs/Image` (SHA-256 `05c43917…`)
- DTB: `guest-inputs/modified.dtb` (SHA-256 `e83a6437…`)
- Initramfs: `guest-inputs/base-t683-fix.cpio.gz` (SHA-256 `8fb3c9de…`) via loader at `0x110000000`
- System: `guest-inputs/system.img` (SHA-256 `98fc0b02…`)
- GDB port: 1237
- Machine shape: identical virt,gic-version=2,virtio-mmio-transports=1, -smp 4
- Host elapsed: 245.52 s
- Guest max time: 242.682 s (target 240 s reached cleanly)
- QEMU returncode: 0
- UART line count: 12157 lines
- UART SHA-256: `efe4da6d0bec43c64b0ef5227ff287bd51b224218690c430e4c61b1a1c4fcc30`
- Preserved at `flights/t695-flight-ab/uart.log` and `diagnostics/t695-canonical-ab-uart.log`.

Key markers observed:
- `[    0.364122] Trying to unpack rootfs image as initramfs...`
- `[   25.990270] uart-pl011 9000000.pl011: no DMA platform data`
- `[   26.321218] Freeing unused kernel memory: 1024K`
- `[   29.496160] init: init first stage started!`
- `[   35.754196] init: init second stage started!`
- Services spawned: `servicemanager`, `hwservicemanager`, `gatekeeperd`, `hal_fingerprint`, `mediaswcodec`, `gnss`.
- Zero panic / oops / hangs.
- Periodic `T642_BOOT_STATE` lines:
  - line 6986: uptime 148.95 s, `boot_completed=`
  - line 8829: uptime 176.09 s, `boot_completed=`
  - line 10038: uptime 199.54 s, `boot_completed=`
  - line 11736: uptime 224.33 s, `boot_completed=`

A/B Conclusion: The Mac QEMU 11.1.2 harness, DTB, initramfs, and system image boot cleanly and stably to userspace when paired with a working kernel. The Step 3 failure is 100% caused by the Route A Image. Step 4 PASS. Recorded in `progress-t695.txt` at 2026-11-07T22:02:46Z.

---

## Deep Dive: Why the Route A Pre-Boot Symbol Gate Passed While Runtime Boot Panicked

The T688 diagnosis proved that T686 hung because cold functions were placed in `.text.unlikely.` which was not matched by `TEXT_TEXT` and fell into the `__init` region.
The T688 rebuild recipe hypothesized Route A (restoring canonical LTO configs: `CONFIG_LTO=y`, `CONFIG_THINLTO=y`, `CONFIG_LTO_CLANG=y`) might pull cold functions back under `_etext`.

The pre-boot symbol gate specifically checked:
`printk`, `__die`, `panic`, `dump_stack`, `show_regs` vs `_etext` / `__init_begin`.
Under ThinLTO in Clang 21, the code generator indeed grouped these functions under `_etext` (`0xffffff8008b70000`), so the pre-boot symbol gate reported PASS.

However, ThinLTO caused a **second, worse orphan section defect** that was not monitored by the symbol gate:
In Linux 4.19 `include/linux/init.h`:
```c
#define __initcall_id(fn, id) __initcall_##id##_##fn
```
Each initcall is assigned to a section such as `.initcall0.init`, `.initcallrootfs.init`, etc.
When Clang 21 compiles with `CONFIG_THINLTO=y`, it appends module and function disambiguation suffixes to sections, producing names like:
- `.initcallrootfs.init..85_655_populate_rootfs`
- `.initcall5.init..79_74_create_debug_debugfs_entry`
- `.initcall2.init..80_150_debug_monitors_init`
- `.initcall6.init..232_1153_inet6_init`

Linux 4.19's `include/asm-generic/vmlinux.lds.h` specifies:
```c
#define INIT_CALLS_LEVEL(level)						\
		__initcall##level##_start = .;				\
		KEEP(*(.initcall##level##.init))			\
		KEEP(*(.initcall##level##s.init))			\
```
Because there is no wildcard (no `*(.initcall##level##.init*)`), LLD matched NONE of the suffixed initcall sections.
LLD treated all 780+ initcall sections as orphans and placed them after `.data` (`0xffffff80093ee810`–`0xffffff80093f06f8`).

Evidence from `diagnostics/t695-routeA-empty-initcall-table.txt`:
In Route A `vmlinux`:
```
ffffff8009293c48 D __initcall0_start
ffffff8009293c48 D __initcall1_start
ffffff8009293c48 D __initcall2_start
ffffff8009293c48 D __initcall3_start
ffffff8009293c48 D __initcall4_start
ffffff8009293c48 D __initcall5_start
ffffff8009293c48 D __initcall6_start
ffffff8009293c48 D __initcall7_start
ffffff8009293c48 D __initcall_end
ffffff8009293c48 D __initcall_start
ffffff8009293c48 D __initcallrootfs_start
```
`__initcall_start` equals `__initcall_end` (size = 0 bytes).

In contrast, in T686 `vmlinux` (non-LTO):
```
ffffff800924ab48 D __initcall_start
ffffff800924ab9c D __initcall0_start
ffffff800924abac D __initcall1_start
ffffff800924ac80 D __initcall2_start
ffffff800924acec D __initcall3_start
ffffff800924ad20 D __initcall4_start
ffffff800924ae98 D __initcall5_start
ffffff800924af44 D __initcallrootfs_start
ffffff800924af48 D __initcall6_start
ffffff800924b5dc D __initcall7_start
ffffff800924b65c D __initcall_end
```
The initcall table spanned `0xffffff800924ab48` to `0xffffff800924b65c` (2,836 bytes of populated function pointers).

Because the initcall table in Route A was empty, the kernel booted with zero driver or subsystem initialization, leading directly to the guest panic at 0.113 s.

---

## Step 5 — Adjudication

### Verdict
Verdict: FAIL

### First Failing Link
Route A kernel panicked at guest 0.113 s due to an empty initcall table caused by Clang 21 ThinLTO section suffixing.

### sys.boot_completed
`sys.boot_completed: not observed (0 / empty)` across all flights and cycles.

### Display / Frame Commit
No frame-commit claim: Route A never initialized `simplefb` (`fb0: simplefb registered!` never appeared). Canonical A/B control kernel does not contain `CONFIG_FB`.

### Reproducibility (TEN-PASSES)
TEN-PASSES was NOT run. Route A failed on flight 1; A/B control was executed once to verify the harness.

### Regressions Check
- `modified.dtb` (`e83a6437…`): untouched and verified.
- `base-t683-fix.cpio.gz` (`8fb3c9de…`): untouched and verified.
- `system.img` (`98fc0b02…`): untouched and verified.
- Composer ELF (`ddc1e4dd…`): untouched and verified.
- No kernel rebuild performed in this lane.

### QEMU Version String (Verbatim)
```
QEMU emulator version 11.1.2
Copyright (c) 2003-2026 Fabrice Bellard and the QEMU Project developers
```

### Supervisor Adaptations (from `scripts/supervisor_t687.py`)
1. Kernel path defaulted to `guest-inputs/routeA-Image` (SHA-256 `11adf236…`), with env override `T695_KERNEL` / `T695_KERNEL_SHA`.
2. QEMU drive ID changed from `t687system` to `t695system`.
3. Kernel command-line append updated with `androidboot.t695_kernel_sha256`.
4. Host cap and guest target defaulted to 1200 s, configurable via `T695_HOST_CAP` and `T695_GUEST_TARGET`.
5. Added unique TCP GDB port per flight (`1236` for flight-01, `1237` for flight-ab).
6. Track constants and flight naming updated from T687 to T695.

---

## Preserved Evidence Inventory

All register values, PCs, and addresses cited in this report appear verbatim in the following preserved files:

```
72740f1ee25fcba63b6453825213ee72847b1989c4bb14dfbb65b044838e0f24  flights/t695-flight-01/uart.log
f58555eba3387c4c8a94f2e0e831a28b021960e1b7e1feb2eb35318e7959ba07  flights/t695-flight-01/flight-end.json
efe4da6d0bec43c64b0ef5227ff287bd51b224218690c430e4c61b1a1c4fcc30  flights/t695-flight-ab/uart.log
3574b8ecd8ac89a053b30a3d43a75fa2b6334cb4163a10776c493e2d17802f30  flights/t695-flight-ab/flight-end.json
f4213dd9644d10f59695b0b22836f388ca45bb392b756954adb5eea66d3b56b2  diagnostics/t695-nm-symbols.txt
fbd678c0e5c7093a976d2cdc66d7b2a5f464d71b3f5e60b58411951dbe7bfbe7  diagnostics/t695-routeA-empty-initcall-table.txt
d767f805cca3b6926dbed0ff0a7e0febb2d496e53b4acb9e053482a1b7c86b9a  diagnostics/t695-routeA-readelf-sections.txt
13fb5b0378983252d3eca8d3641a16e2cc20d3968dd40260a7f87edb2b1a6b3e  diagnostics/t695-linker-script-initcalls.txt
72740f1ee25fcba63b6453825213ee72847b1989c4bb14dfbb65b044838e0f24  diagnostics/t695-routeA-panic-uart.log
efe4da6d0bec43c64b0ef5227ff287bd51b224218690c430e4c61b1a1c4fcc30  diagnostics/t695-canonical-ab-uart.log
```

Specific citations:
- Data abort ESR: `0x96000005`, faulting PC: `klist_next+0x20/0xc4`, LR: `class_dev_iter_next+0x18/0x44`, faulting address: `0000000000000100` (`flights/t695-flight-01/uart.log` lines 50–57).
- Route A symbol addresses: `diagnostics/t695-nm-symbols.txt`.
- Route A empty initcall table: `diagnostics/t695-routeA-empty-initcall-table.txt`.
- Canonical A/B markers: `flights/t695-flight-ab/uart.log` line 186 (`Freeing unused kernel memory: 1024K`), line 597 (`init: init first stage started!`), line 730 (`init: init second stage started!`).

---

## Handoff to Next Lane (Route B)

Route A is dead. Next lane must implement Route B:
1. Linker script modification in `include/asm-generic/vmlinux.lds.h`:
   Add `.text.unlikely.*` and `.text.hot.*` wildcards to `TEXT_TEXT`.
2. Keep non-LTO configuration (`CONFIG_LTO_NONE=y`), which preserves the fully working initcall table.
3. Rebuild the kernel and verify both:
   - Cold functions are below `_etext`.
   - Initcall table `__initcall_start` to `__initcall_end` is populated.
4. Flown on the Mac QEMU 11.1.2 harness.
