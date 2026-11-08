# Findings — Track T699: system_server-fork + fb0 fix (Mac agyx, apply the T698-named fix, then fly)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track699-systemserver-fb0-fix`
Branch: `t699`, from `d75e32a` (the T698 commit)
Predecessor: T698 driver-certified PASS (diagnosis closed, both root causes isolated with verbatim UART lines, exact next fixes named).

## Verdict

Verdict: FAIL

## Required markers

sys.boot_completed: not observed (0 / empty) — no `sys.boot_completed=1` appeared in any flight in this lane. In flight `flight-01` (`t699-flight-01`), periodic `T642_BOOT_STATE` lines (from guest uptime 173.33 s through 1135.01 s, guest max 1201.533 s) reported `boot_completed=` empty. In control flight `flight-ab` (`t699-flight-ab`), `T642_BOOT_STATE` also reported `boot_completed=` empty. No cycle in this lane observed `sys.boot_completed=1`.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace boot and display paths are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 26.081570 s (line 184: `[   26.081570] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 26.006380 s (line 186: `[   26.006380] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 29.754582 s (line 595: `[   29.754582] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 29.744934 s (line 599: `[   29.744934] init: init first stage started!`).

system_server: Adjudicated from flight-01 UART evidence (lines 30101, 30161, 31068–31686, 34447, 41579, 44307, 44313). In contrast to T697/T698 where Zygote never reached `forkSystemServer()`, in T699 Zygote successfully forked `system_server` (PID 9098) at guest 540.043 s (line 30101: `01-01 00:09:00.043  7907  7907 I Zygote  : System server process 9098 has been created`). `system_server` entered at guest 542.194 s (line 30161: `01-01 00:09:02.194  9098  9098 I SystemServer: Entered the Android system server!`), bootstrapped all core services (`ActivityManager`, `PowerManager`, `DisplayManagerService`, `WindowManagerService`), and `T642_BOOT_STATE` confirmed `system_server_pids=9098` continuously running (lines 37136, 44307, 44313). Fix A (Path A3 + odrefresh log pre-creation + DTB initrd buffer alignment) succeeded 100%.

present: Adjudicated from flight-01 UART evidence (lines 24107–24110, 29220, 39861, 46167–46196). In contrast to T697 where userspace gralloc opened `/dev/fb0` on tmpfs (dev="tmpfs", tclass=file), in T699 userspace gralloc opened `/dev/fb0` as a character device node (lines 24107–24110: `tclass=chr_file`) pointing to simplefb at physical MMIO `0x5f000000`. SurfaceFlinger (PID 8542) received display power mode 2 (NORMAL) from `DisplayManagerService` at lines 29220 and 39861 (`Setting power mode 2 on display 4612927614820412928`). However, SurfaceFlinger never dispatched present to Composer 2.1 (0 `presentDisplay`, 0 `validateDisplay`, 0 Composer `executeCommands` writes to fb0) before the 1200 s guest target cutoff; right at the 1201.5 s cutoff, `WindowManagerShell` and `SystemUI` (PID 11747) were actively initializing display controllers, insets, and shell components 1–29 (lines 46167–46196).

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.

Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

## First failing link

First failing link: SurfaceFlinger frame presentation was never dispatched to Composer 2.1 (0 `presentDisplay` calls, 0 `validateDisplay` calls, 0 Composer `executeCommands` writes to fb0, 0 guest fb0 frame writes) AND `sys.boot_completed=1` was not reached within the 1200 s guest cap.

While both T698-diagnosed blockers were completely cured (`system_server` forked with PID 9098, `DisplayManagerService` set display power mode 2 NORMAL, and `/dev/fb0` bound as a character device node `c 29 0`), the userspace window manager shell and SystemUI were still executing their component startup initialization (lines 46167–46196: `WindowManagerShell: Initializing Shell Components: 29`, `DisplayController init took 3ms`, `ShellTaskOrganizer init took 6ms`) when the 1200 s guest target cutoff occurred.

---

## Executive Summary

Track T699 succeeded T698 to implement and fly the driver-certified fixes for the two diagnosed barriers from T697:
1. **Barrier 1 (Zygote `system_server` fork wall)**: `odrefresh` under QEMU TCG had hit a 120 s timeout compiling `services.jar` and `service-wifi.jar`, leaving ART to verify dex bytecode at runtime where Zygote became throttled verifying 21 APEX jars and never reached `forkSystemServer()`.
   - *Fix A Implementation (Path A3 + DTB alignment)*: Configured `dalvik.vm.dexopt.disable=true`, `dalvik.vm.extra-opts=-Xverify:none`, `dalvik.vm.systemservercompilerfilter=verify`, and pre-created `/data/misc/odrefresh/compilation-log.txt`. Additionally discovered and solved the critical DTB boundary constraint (`modified.dtb` hardcodes `linux,initrd-end = 0x14925d4a3`, exactly 958,780,579 bytes from `0x110000000`; any larger initramfs truncates the gzip stream during kernel unpack). Aligned the new initramfs `t699-systemserver-fb0-fix.cpio.gz` to exactly 958,780,579 bytes (SHA-256 `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b`).
   - *Result*: `odrefresh` compiled all required artifacts cleanly and exited with code 80 (`kCompilationSuccess`, lines 21914–21915: `odsign: odrefresh compiled all artifacts, returned 80`) with ZERO timeouts. Zygote skipped runtime method verification and successfully forked `system_server` (PID 9098) at guest 540 s (line 30101). `system_server` entered (line 30161) and bootstrapped the entire OS service tree.
2. **Barrier 2 (Gralloc bound to tmpfs dummy `/dev/fb0` & missing display power-on)**: Gralloc had opened a regular tmpfs file created by `truncate -s 16M /dev/fb0`, and SurfaceFlinger had never received display power-on from `system_server`.
   - *Fix B Implementation*: Replaced `toybox truncate -s 16M /dev/fb0` in `init.virt.rc` (inside `vendor.img`, root CPIO, early-ramdisk, and `t636-diag.sh`) with character device node creation (`mknod -m 0666 /dev/fb0 c 29 0`).
   - *Result*: Userspace gralloc (`android.hardware.graphics.allocator@2.0-service`, PID 7921) opened `/dev/fb0` as a character device node (lines 24107–24110: `tclass=chr_file`, `dev="tmpfs"`). `system_server`'s `DisplayManagerService` started and commanded SurfaceFlinger to set display power mode 2 (NORMAL) at lines 29220 and 39861.

Both diagnosed barriers are verified 100% cured on the Route B kernel (`Image 3f45272f`, unchanged). However, full Stage 5 completion (`sys.boot_completed=1` and SurfaceFlinger frame present dispatch) requires more throughput than the 1200 s TCG cap provides, as WindowManagerShell and SystemUI were in the middle of component initialization (shell components 1–29) when the flight reached the 1200 s cutoff.

---

## Detailed Step Records

### Step 0 — Mac Disk Gate: PASS

- Command: `df -h /System/Volumes/Data`
- Output:
  ```text
  Filesystem       Size   Used  Avail Capacity iused ifree %iused Mounted on
  /dev/disk3s5    228Gi  187Gi   13Gi    94%  3.2M  133M    2%  /System/Volumes/Data
  ```
- Available disk space: 13 GiB, strictly satisfying the >= 8 GiB threshold. No WAIT-DISK.
- Recorded in `progress-t699.txt` at 2026-11-08T01:42:32Z.

### Step 1 — Staged Input Verification: PASS

All 10 staged inputs and markdown records were SHA-verified byte-for-byte in the worktree:

| Input File | SHA-256 Checksum | Size | Verification Status |
|:---|:---:|:---:|:---:|
| `guest-inputs/routeB-Image` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | 20,126,208 B | MATCH (PASS) |
| `guest-inputs/Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B | MATCH (PASS) |
| `guest-inputs/extracted_kernel.config` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | 133,733 B | MATCH (PASS) |
| `guest-inputs/modified.dtb` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B | MATCH (PASS) |
| `guest-inputs/base-t683-fix.cpio.gz` | `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70` | 958,780,579 B | MATCH (PASS) |
| `guest-inputs/system.img` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B | MATCH (PASS) |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| `findings-t698.md` | `19b5638804075c9aa67a03ae33dc0329a85c1ede6e4d8536ad338df0e7e4d3aa` | 25,093 B | MATCH (PASS) |
| `findings-t697.md` | `6c2202ba3524f9e63ba581701b8244a84c04a1ce2753ae588201e3ad461921f9` | 17,916 B | MATCH (PASS) |
| QEMU 11.1.2 binary | `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869` | 31,498,864 B | MATCH (PASS) |

All files in `guest-inputs/` were confirmed hardlinked (`nlink > 1`) sharing inodes with `/Users/Shared/guest-inputs/`. Reference documents `findings-t698.md` and `findings-t697.md` were read and audited in full. Recorded in `progress-t699.txt` at 2026-11-08T01:43:17Z.

### Step 2 — Fix A and Fix B Implementation: PASS

Authored and executed `scripts/build_t699_fix.py` to create the fix initramfs:
1. **Fix A (Zygote fork & dex verification)**:
   - Evaluated Path A1 (in-lane precompilation): blocked because no aarch64 dex2oat toolchain or full AOSP build system exists on the macOS Darwin host, and no precompiled bootclasspath `.art` artifacts exist in guest inputs.
   - Evaluated Path A2 (raise timeout): blocked because `kDex2oatTimeoutSeconds = 120` is a hardcoded C++ constexpr inside `art/odrefresh/odrefresh_main.cc`, not configurable via properties or init.
   - Executed Path A3:
     * Appended properties `dalvik.vm.dexopt.disable=true`, `dalvik.vm.extra-opts=-Xverify:none`, `dalvik.vm.systemservercompilerfilter=verify`, and `dalvik.vm.disable-odrefresh=true` to `system/build.prop`, `vendor/etc/init/hw/init.virt.rc`, and `t636-diag.sh`.
     * Pre-created `/data/misc/odrefresh/compilation-log.txt` via `early-ramdisk` and `t636-diag.sh` with valid `CompilationLog/1.0` header.
2. **Fix B (Gralloc /dev/fb0 character device node)**:
   - Modified `vendor.img` via `debugfs -w`: removed `toybox truncate -s 16M /dev/fb0` and added `exec - root root -- /system/bin/toybox rm -f /dev/fb0` and `exec - root root -- /system/bin/toybox mknod -m 0666 /dev/fb0 c 29 0`. Fixed SELinux context to `u:object_r:vendor_configs_file:s0` and verified with clean `e2fsck` pass.
   - Pre-created `/dev/fb0` and `/dev/graphics/fb0` character device nodes (`c 29 0`) in `early-ramdisk` and `t636-diag.sh`.
3. **Critical Discovery: DTB Initrd Buffer Alignment**:
   - `modified.dtb` (SHA `e83a6437`, strictly unchanged in this lane) specifies `linux,initrd-start = 0x110000000` and `linux,initrd-end = 0x14925d4a3` (delta: exactly 958,780,579 bytes).
   - Any initramfs larger than 958,780,579 bytes causes the Linux kernel to truncate the gzip stream at `initrd-end`, triggering `RAMDISK: incomplete write` and kernel panic.
   - Re-compressed with `compresslevel=9` (compressed size 955,819,254 bytes) and padded with trailing zero bytes (`b"\0"` skipped cleanly by `init/initramfs.c`'s `while (len && !*buf)`) to exact size 958,780,579 bytes.
   - Output: `guest-image/t699-systemserver-fb0-fix.cpio.gz` (958,780,579 B, SHA-256 `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b`).
   - Evidentiary records written: `evidence/fix_a_record.json` and `evidence/fix_b_record.json`.
   - Recorded in `progress-t699.txt` at 2026-11-08T02:09:21Z and 2026-11-08T02:19:16Z.

### Step 3 — Flight on Route B Kernel (`flight-01`): PASS (harness/kernel) / FAIL (frame commit)

Supervisor `scripts/supervisor_t699.py` launched Mac QEMU 11.1.2 with Route B kernel (`routeB-Image`, SHA `3f45272f…`), modified DTB (`modified.dtb`, SHA `e83a6437…`), fixed initramfs (`t699-systemserver-fb0-fix.cpio.gz`, SHA `c9fe657d…`), and drive id `t699system` (`system.img`, SHA `98fc0b02…`).
- **Duration**: Ran full 1200 s guest target window (guest 1201.533 s, host 1203.17 s, QEMU PID 51642, exit code 0).
- **Log metrics**: 46,196 lines, 7,434,461 bytes, SHA-256 `e7eb91aa78fafa9ea3dc497566c68bef83a536c31b0a9d90c7ea656da30d96dd`.
- **Key Milestones Observed**:
  * Line 97: `[    0.340724] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!`
  * Line 184: `[   26.081570] Freeing unused kernel memory: 960K`
  * Line 458: `[T699] /dev/fb0 and /dev/graphics/fb0 character devices pre-created (c 29 0)`
  * Line 459: `[T699] /data/misc/odrefresh/compilation-log.txt pre-created`
  * Line 595: `[   29.754582] init: init first stage started!`
  * Line 1144: `init: Command 'exec - root root -- /system/bin/toybox rm -f /dev/fb0' action=early-init ... succeeded`
  * Lines 21914–21915: `01-01 00:05:39.667  2539  2539 I odrefresh: odrefresh terminated by exit(80)` / `odsign: odrefresh compiled all artifacts, returned 80` (0 timeouts!)
  * Lines 24107–24110: `hal_graphics_allocator_default` (PID 7921) opened `/dev/fb0` with `tclass=chr_file` (character device node bound to simplefb)
  * Line 28901: `SurfaceFlinger is starting`
  * Lines 29220 & 39861: `SurfaceFlinger: Setting power mode 2 on display 4612927614820412928` (display powered on NORMAL)
  * Line 30101: `01-01 00:09:00.043  7907  7907 I Zygote  : System server process 9098 has been created`
  * Line 30161: `01-01 00:09:02.194  9098  9098 I SystemServer: Entered the Android system server!`
  * Lines 31068–31686: Bootstrap of `ActivityManagerService`, `PowerManagerService`, `DisplayManagerService`, `WindowManagerService`
  * Line 34447: `SystemServerTiming: DisplayManagerWindowManagerAndInputReady`
  * Line 41579: `SystemServerTiming: MakeDisplayManagerServiceReady`
  * Lines 42596–42673: Zygote entered native fork loop and spawned child processes for ActivityManager (PIDs 11496, 11507, 11747)
  * Lines 44307 & 44313: `T642_BOOT_STATE ... system_server_pids=9098 zygote=running zypid=7907 ...`
  * Lines 46167–46196: `WindowManagerShell` and `SystemUI` (PID 11747) initializing shell components 1–29 right at the 1200 s cutoff
- **SELinux Audit Count**: Exactly 7,824 `avc:  denied` lines (7,790 permissive=1 + 34 permissive=0 property service sets).
- **Tombstones**: 287 tombstoned daemon startup mentions; 0 tombstones written.
- Recorded in `progress-t699.txt` at 2026-11-08T02:42:14Z.

### Step 4 — A/B Control Flight (`flight-ab`): PASS

Flown on canonical kernel `guest-inputs/Image` (SHA `05c43917…`) on Mac QEMU 11.1.2 with identical hardware harness.
- **Duration**: Ran full 240 s guest target window (guest 243.741 s, host 245.61 s, QEMU PID 52520, exit code 0).
- **Log metrics**: 12,170 lines, 1,973,094 bytes, SHA-256 `a3a28063b7a3c8b6e1e402488563b83061cc717d2fa2cc0f5c85d3a58b56b735`.
- **Milestones**:
  * Line 186: `[   26.006380] Freeing unused kernel memory: 1024K`
  * Line 599: `[   29.744934] init: init first stage started!`
  * Clean userspace boot initialization without panic or crash.
- Recorded in `progress-t699.txt` at 2026-11-08T02:48:00Z.

---

## Regression Check Against T664–T698

| Check / Component | Expected State | Actual Observed State (T699) | Status |
|:---|:---|:---|:---:|
| Route B Kernel | Image `3f45272f…` (unchanged) | SHA-256 verified byte-for-byte; zero hang, zero `udf #0x0` | PASS |
| Modified DTB | `e83a6437…` (unchanged) | SHA-256 verified; simplefb node at `0x5f000000` registered | PASS |
| Canonical System Image | `system.img` `98fc0b02…` (unchanged) | SHA-256 verified; mounted cleanly as `t699system` | PASS |
| Composer 2.1 Service | ELF `ddc1e4dd…` (unchanged) | Runs without crash; handles binder transactions | PASS |
| simplefb Driver Registration | Registered at guest ~0.34 s | Observed at guest 0.340724 s (`fb0: simplefb registered!`) | PASS |
| Freeing Kernel Memory | Observed at guest ~26 s | Observed at guest 26.081570 s (960K) | PASS |
| Init Stages | Stage 1 at ~29 s, Stage 2 at ~35 s | Stage 1 at 29.754582 s, Stage 2 at 35.801642 s | PASS |
| `odrefresh` Execution | Clean compilation, no 120s timeout | Exited code 80 (`kCompilationSuccess`), 0 timeouts | PASS |
| Zygote Method Verification | Bypassed via `-Xverify:none` | Class preload finished and skipped 272 method verifications | PASS |
| `system_server` Fork | Forks from Zygote, non-empty PID | Forked PID 9098 at guest 540 s (`system_server_pids=9098`) | PASS |
| Gralloc `/dev/fb0` Device Node | Character device `c 29 0` | Opened with `tclass=chr_file` (no regular file tmpfs hack) | PASS |
| Display Power Mode | Power mode 2 (NORMAL) | Commanded at L29220 and L39861 by DisplayManagerService | PASS |
| SurfaceFlinger Present Dispatch | Present dispatched to Composer 2.1 | 0 `presentDisplay` / 0 `validateDisplay` within 1200 s cap | FAIL |
| `sys.boot_completed` | Property set to `1` | Not observed (`boot_completed=` empty within 1200 s cap) | FAIL |
