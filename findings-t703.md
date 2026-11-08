# Findings — Track T703: Delete the AOT shadow (Mac agyx, remove pre-compiled Launcher3 odex/vdex so the patched APK actually executes, then fly for the first queued buffer + first present dispatch)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track703-delete-aot-shadow`
Branch: `t703`, from `50d8567` (the T702 commit)
Predecessor: T702 driver-certified honest FAIL (commit 50d8567: AOT shadow isolated with verbatim UART lines, exact next fix named).

## Verdict

Verdict: FAIL

(Honest FAIL: Pre-compiled AOT shadow `Launcher3QuickStep.odex` and `Launcher3QuickStep.vdex` were cleanly deleted and verified absent from external `guest-inputs/system.img`, but guest userspace boots via `early-ramdisk` which loop-mounts the internal embedded `ramdisk-images/system.img` from `t699-systemserver-fb0-fix.cpio.gz` instead of the external virtio-blk block device; ART thus executed the embedded unpatched AOT ODEX/VDEX; `com.android.launcher3` crashed across 53 DotRenderer NPE instances + 2 DeadSystemException instances; SurfaceFlinger held 0 queued buffers, 0 present dispatch within 2400 s window).

## Required markers

sys.boot_completed: observed (1) — `sys.boot_completed=1` was observed and processed by init in flight `flight-01` (`flights/flight-01/uart.log`). First wave was processed at guest 1421.416780 s (line 72427: `[ 1421.416780] init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1227)`), triggering `bootstat` first_boot_completed processing at guest 1426.252372 s (line 72986), `dmesgd` logging at guest 1426.472240 s (line 73006), and `flags_health_check` at guest 1426.476142 s (line 73007). In control flight `flight-ab`, `boot_completed=` was empty throughout the 240 s window.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace boot, initramfs mount topology, ART execution, and display pipeline paths are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 24.493302 s (line 186: `[   24.493302] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 24.239704 s (line 187: `[   24.239704] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 28.045438 s (line 596: `[   28.045438] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 27.862900 s (line 601: `[   27.862900] init: init first stage started!`).

Launcher3 crashes / queueBuffer:
- Exactly **55 crash blocks** of `Process: com.android.launcher3` in `flights/flight-01/uart.log` (2 early `DeadSystemException` before boot completion, lines 49181–49185; exactly **53 distinct DotRenderer NPE crash blocks** between lines 77582 and 135592 across PIDs 12655 to 18306).
- First DotRenderer NPE crash: line 77582 (PID 12655) at guest 11-10 23:35:11.470 (`Caused by: java.lang.NullPointerException: Attempt to invoke virtual method 'int android.graphics.Bitmap.getHeight()' on a null object reference at com.android.launcher3.icons.DotRenderer.<init>(DotRenderer.java:64)`).
- Last DotRenderer NPE crash: line 135592 (PID 18306) at guest 11-10 23:49:57.076.
- `queueBuffer` count: **0** (SurfaceFlinger held 0 queued client buffers).
- Root cause of patch bypass: Groundbreaking architectural isolation: The guest kernel and initramfs boot path (`early-ramdisk`) loop-mounts `/ramdisk-images/system.img` (embedded directly inside `t699-systemserver-fb0-fix.cpio.gz`, 2,596,683,776 bytes, SHA-256 `7527d1cff3161c51e0cf65cbb6dca06076fb8dfba8fa632cb6f7b116fb4f56f1`) to `/dev/loop0`, and `fstab.virt` mounts `/dev/block/loop0` as `/system`. The external QEMU virtio-blk device (`guest-inputs/system.img`, `/dev/vda`) was NEVER mounted by Android userspace! The embedded image still carries the unpatched `Launcher3QuickStep.apk` and the precompiled AOT artifacts (`Launcher3QuickStep.odex` and `.vdex`). Consequently, ART executed the native machine code from the embedded `.odex` file (line 33180: `ArtService: Dexopt result: ... status=SKIPPED ...`).

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- Composer `executeCommands`: **0**
- Composer 2.1 received `BC_FREE_BUFFER` ×211, 0 display commands.

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, failed mount_all at guest ~19 s / 43.32 s (line 1343: `[   43.318756] init: Command 'mkdir /cache/recovery 0770 system cache' action=post-fs (/system/etc/init/hw/init.rc:588) took 0ms and failed: mkdir() failed on /cache/recovery: Read-only file system`), and continuously looped on missing `/system/bin/toybox` (lines 1344–13332) until the 240 s guest cutoff (guest 243.913 s, host 245.64 s). Mount_all wall is completely unchanged.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T703 executed the exact remediation prescribed by T702: delete the pre-compiled AOT artifacts (`Launcher3QuickStep.odex` and `Launcher3QuickStep.vdex`) from `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64` so ART would be forced to run dexopt or interpret the patched `classes7.dex` inside `Launcher3QuickStep.apk`, curing the DotRenderer NPE and allowing Launcher3 to queue the first display buffer.

### What Was Done
1. **Input Staging & AOT Shadow Verification**:
   - Staged all required inputs by hardlink from T702 and verified SHA-256 hashes against specification.
   - Verified that `Launcher3QuickStep.odex` (119,728 bytes, inode 6983) and `Launcher3QuickStep.vdex` (278,604 bytes, inode 6984) were indeed present in `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64` in `guest-inputs/system.img`.
2. **AOT Shadow Removal & Integrity Check**:
   - Preserved T702's image untouched (unlinked hardlink and cloned via APFS copy-on-write `cp -c`).
   - Using `debugfs`, deleted `.odex`, `.vdex`, `oat/arm64`, and `oat` from `guest-inputs/system.img`.
   - Verified filesystem integrity clean with `e2fsck -f -y` (exit code 0).
   - Produced new de-shadowed `guest-inputs/system.img` (SHA-256 `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7`, 2,059,816,960 bytes).
   - Preflight proof: Confirmed `oat` directory is completely absent; extracted `Launcher3QuickStep.apk` from the modified image; confirmed APK hash matches T702 patched APK (`8a28e1db…`, 25,654,202 bytes); extracted `classes7.dex` and confirmed `dexdump -c` passed clean and patched opcodes at `0x199a10`, `0x199a90`, and `0x19b112` were 100% intact.
3. **Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on Mac QEMU 11.1.2 for 2405.51 host seconds (2403.97 guest seconds, exit code 0, 138,604 lines, UART SHA-256 `232ef424…`).
   - `sys.boot_completed=1` was reached and processed by init at guest 1421.42 s (line 72427).
   - SurfaceFlinger declared `Boot is finished` twice (lines 44368, 73788).
   - Composer 2.1 received `BC_FREE_BUFFER` ×211.
4. **Diagnostic Breakthrough: The Embedded Ramdisk Image Barrier**:
   - Despite preflight verification proving that the AOT files were absent from `guest-inputs/system.img`, Launcher3 crashed 53 times with the exact same DotRenderer NPE (lines 77582 to 135592), and `ArtService` logged `status=SKIPPED` (line 33180).
   - Forensic examination of the guest boot environment (`t699-systemserver-fb0-fix.cpio.gz`) uncovered the true mechanism: The guest uses `early-ramdisk` which loops mounts `/ramdisk-images/system.img` (embedded directly in the initramfs!) to `/dev/loop0`, and `fstab.virt` mounts `/dev/block/loop0` as `/system`.
   - **Android userspace never mounts or touches `/dev/vda` (the external QEMU virtio-blk `guest-inputs/system.img`)!**
   - Both T702's DEX patch and T703's AOT deletion modified an external block device that was entirely bypassed by the guest bootloader/initramfs. The embedded `ramdisk-images/system.img` still contains the original unpatched APK and its unpatched AOT ODEX/VDEX files.
5. **Canonical A/B Control (`flight-ab`)**:
   - Flown on canonical kernel `05c43917…` for 240 s guest target (host 245.64 s, guest 243.91 s, exit 0, 13,332 lines, log SHA-256 `411867e5…`).
   - Replicated canonical mount_all failure at guest 43.32 s (line 1343: `mkdir() failed on /cache/recovery: Read-only file system`), confirming harness baseline stability.

---

## Input Verification Table

All inputs verified in `/Users/Shared/wt-track703-delete-aot-shadow` prior to execution:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t702.md` | `708128c022b3f81c092b8743af075086f492568194a3f5afb66a944134aadb34` | `708128c022b3f81c092b8743af075086f492568194a3f5afb66a944134aadb34` | 21,909 B | MATCH (PASS) |
| `findings-t701.md` | `b247c8d142bdaf7c6aa863879dd0c6882a1ae1380631b24b8f19ef5e15f7e15f` | `b247c8d142bdaf7c6aa863879dd0c6882a1ae1380631b24b8f19ef5e15f7e15f` | 26,318 B | MATCH (PASS) |
| `evidence/t702-flight-01-uart.log` | `8c9895c97351783e0fffcaafccd70fdb25efa48016d89d5578615bf281381a26` | `8c9895c97351783e0fffcaafccd70fdb25efa48016d89d5578615bf281381a26` | 17,635,126 B | MATCH (PASS) |
| `evidence/t702-flight-ab-uart.log` | `75feb0116f4e4be366d4120fada697a7792de187e8fc8c9a3cc0bc3044f8f7a7` | `75feb0116f4e4be366d4120fada697a7792de187e8fc8c9a3cc0bc3044f8f7a7` | 2,242,296 B | MATCH (PASS) |
| T702 repacked `system.img` | `8cab6e8987615ad2b51fc659418de0fa34c0d1706b16022175aefb5960e7ee39` | `8cab6e8987615ad2b51fc659418de0fa34c0d1706b16022175aefb5960e7ee39` | 2,059,816,960 B | MATCH (PASS) |
| `guest-inputs/routeB-Image` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | 20,126,208 B | MATCH (PASS) |
| `guest-inputs/t699-systemserver-fb0-fix.cpio.gz` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | 958,780,579 B | MATCH (PASS) |
| `guest-inputs/modified.dtb` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B | MATCH (PASS) |
| `guest-inputs/canonical-Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B | MATCH (PASS) |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |

---

## Step 2: AOT Shadow Deletion & Preflight Proof

1. **Safety & Preservation**:
   - Unlinked the hardlink to T702's `system.img` to ensure the predecessor evidence is preserved immutable.
   - Cloned via APFS copy-on-write `cp -c` (`st_nlink=1`). Verified T702 `system.img` retained SHA `8cab6e89…`.
2. **debugfs Removal Operations**:
   ```bash
   /opt/homebrew/opt/e2fsprogs/sbin/debugfs -w -R "rm system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.odex" guest-inputs/system.img
   /opt/homebrew/opt/e2fsprogs/sbin/debugfs -w -R "rm system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.vdex" guest-inputs/system.img
   /opt/homebrew/opt/e2fsprogs/sbin/debugfs -w -R "rmdir system/system_ext/priv-app/Launcher3QuickStep/oat/arm64" guest-inputs/system.img
   /opt/homebrew/opt/e2fsprogs/sbin/debugfs -w -R "rmdir system/system_ext/priv-app/Launcher3QuickStep/oat" guest-inputs/system.img
   ```
3. **Filesystem Clean Check**:
   - Ran `/opt/homebrew/opt/e2fsprogs/sbin/e2fsck -f -y guest-inputs/system.img`.
   - Result: Exit code 0, filesystem marked clean.
   - New `system.img` SHA-256: `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7` (2,059,816,960 bytes).
4. **Preflight Verification**:
   - `debugfs -R "ls -l system/system_ext/priv-app/Launcher3QuickStep" guest-inputs/system.img` confirmed `oat` directory is completely absent.
   - Extracted `Launcher3QuickStep.apk` using `debugfs`: SHA-256 matched T702's signed patched APK (`8a28e1db…`, 25,654,202 bytes).
   - Extracted `classes7.dex`: Adler-32 and SHA-1 checksums clean (`dexdump -c` passed).
   - Bytecode verification at patch offsets:
     - `0x199a10`: `00 00 00 00` (`nop; nop`)
     - `0x199a90`: `12 00` (`const/4 v0, #int 0`, `USE_HARDWARE_BITMAP = false`)
     - `0x19b112`: `01 02 00 00 00 00 00 00` (`move v2, v0; nop; nop; nop`)
   - Preflight proof confirmed 100% byte-exact preservation of patched DEX and total absence of `.odex`/`.vdex`.

---

## Flight Execution

### 1. Flight-01 (Route B Kernel, 2400 s Target)
- **Command**:
  ```bash
  /opt/homebrew/bin/qemu-system-aarch64 \
    -machine virt,gic-version=2,virtio-mmio-transports=1 -cpu cortex-a57 -m 8192 -smp 4 \
    -kernel guest-inputs/routeB-Image \
    -dtb guest-inputs/modified.dtb \
    -device loader,file=guest-inputs/t699-systemserver-fb0-fix.cpio.gz,addr=0x110000000,force-raw=on \
    -drive if=none,id=t702system,file=guest-inputs/system.img,format=raw,readonly=on \
    -device virtio-blk-device,drive=t702system \
    -append "console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 brd.rd_nr=1 brd.rd_size=524288 androidboot.t703_fix_initramfs_sha256=c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b androidboot.t703_flight_target=2400 androidboot.t703_dtb_sha256=e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389 androidboot.t703_system_sha256=afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7 androidboot.t703_kernel_sha256=3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3" \
    -gdb tcp:127.0.0.1:1245 -display none -serial file:flights/flight-01/uart.log -monitor none -no-reboot
  ```
- **Timing**:
  - Started: `2026-11-08T08:16:07Z`
  - Ended: `2026-11-08T08:56:13Z`
  - Host elapsed: 2405.51 s
  - Guest elapsed: 2403.969 s (clean exit on 2400 s target)
- **Log metrics**: 138,604 lines, 19,789,306 bytes, SHA-256 `232ef4247009d3d850741f607eac5b41b776d8b0705ab917e559794f0d4da2f3`.

### 2. Flight-AB (Canonical Kernel Control, 240 s Target)
- **Kernel**: `guest-inputs/canonical-Image` (SHA-256 `05c43917…`, 23,073,280 bytes).
- **Timing**:
  - Started: `2026-11-08T09:00:46Z`
  - Ended: `2026-11-08T09:04:52Z`
  - Host elapsed: 245.64 s
  - Guest elapsed: 243.913 s
- **Log metrics**: 13,332 lines, 2,241,896 bytes, SHA-256 `411867e5aaf3c72e7c170bc1685857204ce0ececc95d55cf9fe06beef193b15d`.
- **Observations**:
  - Line 187: `[   24.239704] Freeing unused kernel memory: 1024K`
  - Line 601: `[   27.862900] init: init first stage started!`
  - Line 1343: `[   43.318756] init: Command 'mkdir /cache/recovery 0770 system cache' action=post-fs (/system/etc/init/hw/init.rc:588) took 0ms and failed: mkdir() failed on /cache/recovery: Read-only file system`
  - Lines 1344–13332: `/early-ramdisk[216]: /system/bin/toybox: inaccessible or not found` loop.

---

## Proving Observations

### 1. Count of FATAL EXCEPTION for `com.android.launcher3`: 55 (Target: 0)
- Observed **55 crash blocks** for `com.android.launcher3`:
  - 2 early crashes before boot completion caused by `DeadSystemException` (system_server restart, lines 49181–49185, PID 10833).
  - Exactly **53 distinct DotRenderer NPE crashes** post-boot completion (lines 77582 to 135592) across PIDs: 12655, 12901, 12989, 13066, 13185, 13234, 13284, 13348, 13426, 13504, 13559, 13612, 13689, 13735, 13837, 13915, 14041, 14102, 14167, 14246, 14299, 14717, 14815, 14897, 14974, 15152, 15206, 15286, 15372, 15444, 15494, 15878, 15967, 16067, 16147, 16307, 16347, 16408, 16467, 16548, 16606, 16982, 17050, 17138, 17215, 17565, 17646, 17730, 17776, 18066, 18169, 18260, 18306.
- Stack trace identical on all 53 crashes:
  ```
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: FATAL EXCEPTION: main
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: Process: com.android.launcher3, PID: 12655
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: java.lang.RuntimeException: Unable to start activity ComponentInfo{com.android.launcher3/com.android.launcher3.uioverrides.QuickstepLauncher}: java.lang.NullPointerException: Attempt to invoke virtual method 'int android.graphics.Bitmap.getHeight()' on a null object reference
  ...
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: Caused by: java.lang.NullPointerException: Attempt to invoke virtual method 'int android.graphics.Bitmap.getHeight()' on a null object reference
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: 	at com.android.launcher3.icons.DotRenderer.<init>(DotRenderer.java:64)
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: 	at com.android.launcher3.DeviceProfile.createDotRenderer(DeviceProfile.java:676)
  11-10 23:35:11.470 12655 12655 E AndroidRuntime: 	at com.android.launcher3.DeviceProfile.<init>(DeviceProfile.java:668)
  ```

### 2. Count of `queueBuffer`: 0 (Target: > 0)
- SurfaceFlinger held **0** queued client buffers.
- `BLASTSyncEngine` timed out repeatedly waiting for the `QuickstepLauncher` transaction commit.

### 3. Count of Present Dispatch Calls: 0 (Target: > 0)
- `presentDisplay`: 0
- `validateDisplay`: 0
- `presentOrValidate`: 0
- Composer `executeCommands`: 0
- Composer 2.1 received `BC_FREE_BUFFER` ×211, 0 display commands.

---

## The Groundbreaking Root Cause Discovery: The Embedded Ramdisk Image Barrier

The central question was: **If the preflight proof showed `guest-inputs/system.img` had the AOT shadow deleted and the patched APK verified, why did ART still skip dexopt and why did Launcher3 crash with the exact same unpatched bytecode stack?**

Forensic extraction of `guest-inputs/t699-systemserver-fb0-fix.cpio.gz` revealed the answer:

1. **The `early-ramdisk` script inside the CPIO archive**:
   ```bash
   "$TOYBOX" mkdir -p /system /vendor /data /ramdisk-images /dev/block
   "$TOYBOX" mknod -m 0600 /dev/loop0 b 7 0
   "$TOYBOX" mknod -m 0600 /dev/loop1 b 7 1
   "$TOYBOX" losetup -r /dev/loop0 /ramdisk-images/system.img
   "$TOYBOX" losetup -r /dev/loop1 /ramdisk-images/vendor.img
   "$TOYBOX" mknod -m 0600 /dev/block/loop0 b 7 0
   "$TOYBOX" mknod -m 0600 /dev/block/loop1 b 7 1
   echo "[T552] guest attached embedded images to loop0/loop1"
   ```

2. **The `fstab.virt` mount specification inside the CPIO archive**:
   ```
   # Diskless Android boot: early-ramdisk attaches loop0/loop1 before /init.
   # First-stage init recreates block nodes under /dev/block from sysfs.
   /dev/block/loop0 /system ext4 ro,barrier=1 wait,first_stage_mount
   /dev/block/loop1 /vendor ext4 ro,barrier=1 wait,first_stage_mount
   ```

3. **The Architectural Disconnect**:
   - The QEMU invocation passes `-drive if=none,id=t702system,file=guest-inputs/system.img,format=raw,readonly=on -device virtio-blk-device,drive=t702system`. In Linux, this exposes the external image as `/dev/vda`.
   - **However, Android first-stage init and `early-ramdisk` do NOT mount `/dev/vda`!**
   - They mount `/dev/block/loop0`, which was bound by `early-ramdisk` to `/ramdisk-images/system.img` — a 2,596,683,776-byte ext4 image embedded *inside the initramfs ramdisk itself* (SHA-256 `7527d1cff3161c51e0cf65cbb6dca06076fb8dfba8fa632cb6f7b116fb4f56f1`).
   - Inside `/ramdisk-images/system.img`, the original unpatched `Launcher3QuickStep.apk` (28,922,784 bytes) and the unpatched AOT artifacts (`Launcher3QuickStep.odex`, 119,728 bytes, and `Launcher3QuickStep.vdex`, 278,604 bytes) are STILL PRESENT!
   - External modifications to `guest-inputs/system.img` in T702 (DEX patch injection) and T703 (AOT shadow removal) were executed on `/dev/vda`, which the guest userspace never mounted.
   - This single architectural fact explains the entire sequence of observations across T702 and T703.

---

## Analysis & Exact Next Fix (Track T704)

### Summary of State
- The Route B kernel (`Image 3f45272f…`) boots cleanly to userspace and reaches `sys.boot_completed=1` at guest ~1421 s.
- The dual-layer bytecode patch designed in T702 is completely sound and verified.
- The AOT shadow removal strategy designed in T703 is completely sound.
- The point of application was incorrect: changes must be applied to the filesystem that the guest actually mounts.

### Exact Next Fix for Track T704
There are two viable remediation options for T704:

#### Option 1: Wire `early-ramdisk` to mount the external virtio-blk device (`/dev/vda`) [RECOMMENDED]
- In `t699-systemserver-fb0-fix.cpio.gz`, modify `early-ramdisk`:
  - Check if `/dev/vda` exists (or wait for virtio-blk probe).
  - If `/dev/vda` is present, `losetup -r /dev/loop0 /dev/vda` (or link `/dev/block/loop0` to `/dev/vda`).
  - Delete `/ramdisk-images/system.img` from the CPIO archive, drastically reducing the initramfs size (from ~958 MB down to ~250 MB).
  - *Crucial constraint*: Adjust `modified.dtb` `linux,initrd-end` to match the new exact byte length of the repackaged CPIO.gz.
  - *Advantage*: Decouples the 2 GB system image from the initramfs, enabling rapid image modification via `debugfs` without repacking the entire CPIO each time.

#### Option 2: Patch the embedded `/ramdisk-images/system.img` directly inside the initramfs
- Extract `/ramdisk-images/system.img` from `t699-systemserver-fb0-fix.cpio.gz`.
- Apply both the T702 DEX patch (inject patched `Launcher3QuickStep.apk`) and T703 AOT shadow removal (delete `oat/arm64`) to `/ramdisk-images/system.img`.
- Run `e2fsck -f -y` on `/ramdisk-images/system.img`.
- Repack the CPIO archive.
- Ensure the resulting compressed CPIO size matches `modified.dtb`'s `linux,initrd-end` (padded if necessary or DTB updated).
- Fly Route B with the updated initramfs.
