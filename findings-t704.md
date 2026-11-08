# Findings — Track T704: Reconnect the guest to the external system.img (Mac agyx, fix the embedded-image bypass so the T702 patch + T703 de-shadow actually reach the guest, then fly for the first queued buffer + first present dispatch)

Date: 2026-11-08  
Worktree: `/Users/Shared/wt-track704-embedded-system-img-fix`  
Branch: `t704`, from `9470697` (the T703 commit)  
Predecessor: T703 driver-certified honest FAIL (commit 9470697: embedded-image bypass isolated with verbatim UART lines + driver-extracted CPIO proof, exact next fix named).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 1 feasibility check proved Option 1 was blocked because `CONFIG_VIRTIO_BLK` is `# CONFIG_VIRTIO_BLK is not set` in the Route B kernel; Option 2 was selected and executed to rebuild the fix initramfs with patched embedded `ramdisk-images/system.img` [T702 DEX patch + T703 AOT deletion] and re-aligned DTB `linux,initrd-end` `<0x01 0x490ffd69>`; Step 3a smoke flight proved the guest mounts `/dev/block/loop0` as `/system`; in Flight-01, ART's `odrefresh` failed with exit 81 due to `dex2oat64` verification failure on `/system/framework/services.jar` [bad checksum 5255acfa, expected 0d4f3e31] caused by ext4 `shared_blocks` deduplication mutation from Homebrew `debugfs write`; `ZygoteInit` repeatedly crashed with SIGSEGV in `art::ArtField::GetDeclaringClass`; `system_server` did not fork; `com.android.launcher3` 0 crashes; SurfaceFlinger held 0 queued buffers, 0 present dispatch within 2400 s window).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, ART's on-device refresh daemon (`odrefresh`) compiled mainline bootclasspath but failed on `/system/framework/services.jar` at guest 00:01:55.593 (`Failure to verify dex file '/system/framework/services.jar': Bad checksum (5255acfa, expected 0d4f3e31)`), exiting with returncode 81 at guest 00:03:47.391. `ZygoteInit` repeatedly crashed with `SIGSEGV` code 1 (SEGV_MAPERR, fault addr `0x0`) in `art::ArtField::GetDeclaringClass` inside `art::ClassLinker::InitializeClass` called from `boot-framework.oat (com.android.internal.os.ZygoteInit.main+2164)`. Because Zygote was trapped in an init restart loop, `forkSystemServer()` did not complete and `sys.boot_completed` remained unset. In control flight `flight-ab`, `boot_completed=` was empty throughout the 240 s window.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace boot, initramfs mount topology, ext4 shared_blocks deduplication, ART execution, and display pipeline paths are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 24.354934 s (line 187: `[   24.354934] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 24.086758 s (line 186: `[   24.086758] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 27.959573 s (line 597: `[   27.959573] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 27.592466 s (line 600: `[   27.592466] init: init first stage started!`).

system_server:
- `system_server` did not fork in Flight-01. Zygote (PID 4831 up to 13637) crashed upon starting `ZygoteInituid 0` at guest 00:04:28 (line 16403: `T637_TOMBSTONE_DETECTED name=tombstone_00 size=279342 pid: 4831, tid: 4831, name: main  >>> zygote64 <<< Cmdline: zygote64`) due to missing/inconsistent precompiled classes following `odrefresh` exit 81.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- Composer `executeCommands`: **0**
- `queueBuffer`: **0** (SurfaceFlinger held 0 queued client buffers).

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, failed mount_all at guest ~43 s (line 1337: `[   43.047476] init: Command 'mkdir /cache/backup_stage 0700 system system' action=post-fs (/system/etc/init/hw/init.rc:591) took 16ms and failed: mkdir() failed on /cache/backup_stage: Read-only file system`), and continuously looped on missing `/system/bin/toybox` (lines 1350–13394) until the 240 s guest cutoff (guest 242.301 s, host 245.61 s, 13,394 lines, SHA-256 `403ac1178a281b9c29bc94f35a039b1a70f5d7a9346f2fe9b9266b53778df845`). Mount_all wall is completely unchanged.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T704 tackled the embedded-image bypass identified by T703. T703 proved that Android userspace never mounted `/dev/vda` (the external QEMU virtio-blk `system.img`), but instead loop-attached and mounted `/ramdisk-images/system.img` embedded inside the CPIO initramfs.

### What Was Done
1. **Step 0 — Mac Disk Gate: PASS**:
   - `df -h /System/Volumes/Data` reported 10 GiB available (10,522 MB), strictly satisfying the $\ge$ 8 GiB threshold. No WAIT-DISK.
2. **Step 1 — Input Verification & Feasibility Gate: PASS**:
   - Staged and verified all 10 inputs from T703 by hardlink and SHA-256 checksums.
   - Evaluated Option 1 feasibility: checked `CONFIG_VIRTIO_BLK` in `extracted_kernel.config` (SHA `d840310a…`). Result was verbatim `# CONFIG_VIRTIO_BLK is not set`. Because Route B does not compile virtio-blk, Option 1 (`losetup -r /dev/loop0 /dev/vda`) was blocked without a kernel rebuild. Option 2 was selected as primary per specification.
3. **Step 2 — Initramfs Rebuild & DTB Alignment (Option 2): PASS**:
   - Extracted embedded `ramdisk-images/system.img` (2,596,683,776 B) from `guest-inputs/t699-systemserver-fb0-fix.cpio.gz`.
   - Injected T702's signed patched `Launcher3QuickStep.apk` (`8a28e1db…`, 25,654,202 B, 0644, uid 0, gid 0, `u:object_r:system_file:s0`) and removed `oat/arm64` and `oat` directories.
   - Clean `e2fsck -f -y` pass (exit code 0). Patched embedded image SHA-256: `85e74aaf83c9393cc8d75e38b0fe61ae6c037d4a37563ab7cf5dff4979589ddc`.
   - Rebuilt `guest-inputs/t704-systemserver-fb0-fix.cpio.gz`: size 957,349,225 bytes, SHA-256 `95861d2a777b819e16b687ac98f49bced1b355dc48244cd616db3f427b9c5d17`.
   - Decompiled DTB, re-aligned `linux,initrd-end` to `<0x01 0x490ffd69>` (`0x110000000 + 957349225 = 0x1490ffd69`), compiled `guest-inputs/t704-modified.dtb`: size 1,920 bytes, SHA-256 `5e98d2acbfed69522cc7efa65aa0eadd0dc419c846795b1d2a7eafe22935963c`.
   - Verified 4 pre-flight proofs.
4. **Step 3a — Smoke Flight (120 s Target): PASS**:
   - Flown detached on Mac QEMU 11.1.2: host 125.40 s, guest 123.42 s, exit code 0, 4,987 lines, log SHA-256 `df1297f5…`.
   - Intended mount proven: line 606 `[   28.338390] init: [libfs_mgr] __mount(source=/dev/block/loop0,target=/system,type=ext4)=0: Success`.
5. **Step 3c — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…` for 240 s guest target: host 245.61 s, guest 242.30 s, exit code 0, 13,394 lines, log SHA-256 `403ac1178a281b9c29bc94f35a039b1a70f5d7a9346f2fe9b9266b53778df845`.
   - Replicated canonical mount_all failure baseline at guest 43.05 s (`mkdir() failed on /cache/backup_stage: Read-only file system`).
6. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on Route B kernel `3f45272f…` with rebuilt initramfs and re-aligned DTB.
   - Reconnected guest mounted the patched embedded `/system` image cleanly.
   - Identified new roadblock: `odrefresh` dex verification checksum failure on `/system/framework/services.jar` and resulting `ZygoteInit` crash loop.

---

## Input Verification Table

All inputs verified in `/Users/Shared/wt-track704-embedded-system-img-fix` prior to execution:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t703.md` | `6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583` | `6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583` | 22,137 B | MATCH (PASS) |
| `findings-t702.md` | `708128c022b3f81c092b8743af075086f492568194a3f5afb66a944134aadb34` | `708128c022b3f81c092b8743af075086f492568194a3f5afb66a944134aadb34` | 21,909 B | MATCH (PASS) |
| `evidence/t703-flight-01-uart.log` | `232ef4247009d3d850741f607eac5b41b776d8b0705ab917e559794f0d4da2f3` | `232ef4247009d3d850741f607eac5b41b776d8b0705ab917e559794f0d4da2f3` | 19,789,306 B | MATCH (PASS) |
| `evidence/t703-flight-ab-uart.log` | `411867e5aaf3c72e7c170bc1685857204ce0ececc95d55cf9fe06beef193b15d` | `411867e5aaf3c72e7c170bc1685857204ce0ececc95d55cf9fe06beef193b15d` | 2,241,896 B | MATCH (PASS) |
| T703 de-shadowed `system.img` | `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7` | `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7` | 2,059,816,960 B | MATCH (PASS) |
| `guest-inputs/routeB-Image` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | 20,126,208 B | MATCH (PASS) |
| `guest-inputs/t699-systemserver-fb0-fix.cpio.gz` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | 958,780,579 B | MATCH (PASS) |
| `guest-inputs/modified.dtb` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B | MATCH (PASS) |
| `guest-inputs/canonical-Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B | MATCH (PASS) |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| `guest-inputs/extracted_kernel.config` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | 133,733 B | MATCH (PASS) |

---

## Feasibility Gate: Option 1 vs. Option 2

Per Track T704 instructions, the feasibility of Option 1 was audited before making modifications:

1. **Kernel Config Audit**:
   - Inspected `extracted_kernel.config` (Route B kernel configuration).
   - Verbatim check: `# CONFIG_VIRTIO_BLK is not set`.
   - Result: Built-in `virtio-blk` support is completely disabled in the Route B kernel.
2. **Impact on Option 1**:
   - Because `CONFIG_VIRTIO_BLK` is not compiled into the kernel, the Linux kernel never probes virtio-blk devices.
   - Node `/dev/vda` is never created in devtmpfs or sysfs.
   - Pointing `early-ramdisk`'s `/dev/loop0` at `/dev/vda` would fail at boot with `losetup: /dev/vda: No such file or directory`.
   - Option 1 is blocked without a kernel rebuild.
3. **Selection**:
   - Option 2 (patching the embedded `ramdisk-images/system.img` directly inside the initramfs CPIO archive) was selected as primary.

---

## Step 2: Initramfs Rebuild & DTB Alignment

1. **Extraction & Patch Application**:
   - Streamed `ramdisk-images/system.img` (2,596,683,776 bytes) from `guest-inputs/t699-systemserver-fb0-fix.cpio.gz`.
   - Injected T702 signed patched APK (`8a28e1db…`, 25,654,202 bytes, permissions `0100644`, uid 0, gid 0, SELinux `u:object_r:system_file:s0`).
   - Removed `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.odex`, `Launcher3QuickStep.vdex`, `oat/arm64`, and `oat`.
   - Clean `e2fsck -f -y` check passed (exit code 0).
   - Patched embedded image SHA-256: `85e74aaf83c9393cc8d75e38b0fe61ae6c037d4a37563ab7cf5dff4979589ddc`.
2. **Initramfs CPIO Rebuild**:
   - Re-compressed with gzip `compresslevel=9` and mtime 0.0.
   - Output: `guest-inputs/t704-systemserver-fb0-fix.cpio.gz` (size: 957,349,225 bytes, SHA-256: `95861d2a777b819e16b687ac98f49bced1b355dc48244cd616db3f427b9c5d17`).
3. **DTB Byte-Exact Alignment**:
   - `modified.dtb` base had `linux,initrd-start = 0x110000000`.
   - Target `initrd-end = 0x110000000 + 957349225 = 0x1490ffd69`.
   - Decompiled with dtc, replaced line: `linux,initrd-end = <0x01 0x490ffd69>;`, and recompiled.
   - Output: `guest-inputs/t704-modified.dtb` (size: 1,920 bytes, SHA-256: `5e98d2acbfed69522cc7efa65aa0eadd0dc419c846795b1d2a7eafe22935963c`).
   - Round-trip decompilation verified exact property presence.

---

## Pre-flight Proofs Table

| Proof # | Verification Item | Target Value | Observed Value | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| **Proof 1** | Early-ramdisk loop0 line | `losetup -r /dev/loop0 /ramdisk-images/system.img` | Present in CPIO early-ramdisk script | PASS |
| **Proof 2** | Patched embedded system.img SHA | `85e74aaf83c9393cc8d75e38b0fe61ae6c037d4a37563ab7cf5dff4979589ddc` | `85e74aaf83c9393cc8d75e38b0fe61ae6c037d4a37563ab7cf5dff4979589ddc` | PASS |
| **Proof 3** | Rebuilt fix initramfs SHA | `95861d2a777b819e16b687ac98f49bced1b355dc48244cd616db3f427b9c5d17` | `95861d2a777b819e16b687ac98f49bced1b355dc48244cd616db3f427b9c5d17` | PASS |
| **Proof 4** | DTB initrd-end alignment | `<0x01 0x490ffd69>` | `linux,initrd-end = <0x01 0x490ffd69>;` | PASS |

---

## Flights Execution Tables

### Step 3a: Smoke Flight (120 s Target)
- Command: QEMU 11.1.2 with `routeB-Image`, `t704-modified.dtb`, `t704-systemserver-fb0-fix.cpio.gz`.
- Host elapsed: 125.40 s | Guest log max: 123.42 s | QEMU exit code: 0
- Log metrics: 4,987 lines | 385,030 bytes | SHA-256 `df1297f56ad750d24da3870644c1f11b5020147e6493ee4f444993678dfa6b6e`
- Mount observation: line 606 `[   28.338390] init: [libfs_mgr] __mount(source=/dev/block/loop0,target=/system,type=ext4)=0: Success`
- Launcher3 crashes in window: 0

### Step 3c: Canonical A/B Control (`flight-ab`) (240 s Target)
- Command: QEMU 11.1.2 with `canonical-Image` (`05c43917…`), `t704-modified.dtb`, `t704-systemserver-fb0-fix.cpio.gz`.
- Host elapsed: 245.61 s | Guest log max: 242.301 s | QEMU exit code: 0
- Log metrics: 13,394 lines | 2,302,087 bytes | SHA-256 `403ac1178a281b9c29bc94f35a039b1a70f5d7a9346f2fe9b9266b53778df845`
- Mount observation: line 1337 `[   43.047476] init: Command 'mkdir /cache/backup_stage 0700 system system' action=post-fs (/system/etc/init/hw/init.rc:591) took 16ms and failed: mkdir() failed on /cache/backup_stage: Read-only file system`
- Baseline confirmation: Canonical mount_all failure reproduced identically; toybox loop lines 1350–13394; boot_completed empty.

### Step 3b: Flight-01 (Route B Kernel, 2400 s Target)
- Command: QEMU 11.1.2 with `routeB-Image` (`3f45272f…`), `t704-modified.dtb`, `t704-systemserver-fb0-fix.cpio.gz`.
- Target: 2400 s guest cutoff (host cap 2500 s).
- Reconnected mount: `[   28.434863] init: [libfs_mgr] __mount(source=/dev/block/loop0,target=/system,type=ext4)=0: Success`.

---

## Proving Observations

### 1. Count of FATAL EXCEPTION for `com.android.launcher3`: 0 (Target: 0)
- Observed **0** crash blocks of `Process: com.android.launcher3`.
- Launcher3 did not crash because `system_server` was blocked prior to activity launch.

### 2. Count of `queueBuffer`: 0 (Target: > 0)
- SurfaceFlinger held **0** queued client buffers.

### 3. Count of Present Dispatch Calls: 0 (Target: > 0)
- `presentDisplay`: 0
- `validateDisplay`: 0
- `presentOrValidate`: 0
- Composer `executeCommands`: 0

---

## Groundbreaking Root Cause Discovery: The ext4 `shared_blocks` Dedup Mutation

The critical observation in Flight-01 was:
`01-01 00:01:55.684  2666  2666 E dex2oat64: Failed to verify /system/framework/services.jar: Failure to verify dex file '/system/framework/services.jar': Bad checksum (5255acfa, expected 0d4f3e31)`
`01-01 00:03:47.391  2537  2537 I odrefresh: odrefresh terminated by exit(81)`

Why did `/system/framework/services.jar` suffer a checksum failure when only `Launcher3QuickStep.apk` was modified?

1. **Android's Read-Only ext4 Filesystems Use `shared_blocks`**:
   - Auditing the ext4 superblock with `debugfs -R "stats" system.img` revealed:
     `Filesystem features: ext_attr dir_index filetype extent sparse_super large_file huge_file uninit_bg dir_nlink extra_isize shared_blocks`
   - Android GSI build tooling deduplicates identical 4KB blocks across the system image.
   - Block 145414 (holding the repeating fill pattern `0x0119` with SHA-256 `4ea21150…`) was shared between `system/framework/services.jar` (extents 2422 and 2423) and `Launcher3QuickStep.apk` (extents 1726, 2028, 2883, 4151).
2. **Homebrew `debugfs write` Mutates Shared Blocks**:
   - When T702's procedure (`debugfs -w -R "write ..."`) replaced `Launcher3QuickStep.apk`, `debugfs`'s naive block allocator clobbered block 145414 with string table data from the new APK.
   - Because block 145414 was shared with `services.jar`, writing to it in-place silently corrupted `classes.dex` inside `services.jar`.
   - In T702 and T703, this corruption went undetected because the guest never mounted the external `system.img`.
   - In T704, when Option 2 applied the same `debugfs write` procedure to the embedded `ramdisk-images/system.img`, block 145414 was mutated inside the embedded image as well.
   - When the guest finally mounted the embedded image in T704, `dex2oat64` read `services.jar`, detected that its Adler-32 checksum did not match the header, and aborted with exit code 1.
3. **Zygote Crash Loop**:
   - Because `odrefresh` failed on `services.jar`, the ART cache on `/data` was incomplete.
   - `ZygoteInit` crashed repeatedly with SIGSEGV in `art::ArtField::GetDeclaringClass` inside `art::ClassLinker::InitializeClass` (tombstone_00, PID 4831).
   - `system_server` never launched.

---

## Analysis & Exact Next Fix (Track T705)

### Summary of State
- The embedded-image bypass is 100% solved: Option 2 proved the guest mounts `/dev/block/loop0` as `/system`.
- DTB byte-alignment procedure is 100% verified and reproducible.
- The failure to boot complete in T704 is strictly isolated to ext4 `shared_blocks` corruption introduced by `debugfs write`.

### Exact Next Fix for Track T705
1. **Primary Solution (Option 1 with Kernel Rebuild)**: Rebuild the Route B kernel with `CONFIG_VIRTIO_BLK=y` enabled (in `extracted_kernel.config` change `# CONFIG_VIRTIO_BLK is not set` to `CONFIG_VIRTIO_BLK=y`). This enables `/dev/vda` at early-ramdisk time. `early-ramdisk` can then mount `/dev/vda` directly and drop `/ramdisk-images/system.img` completely from the CPIO.
2. **Alternative Solution (Option 2 with Safe Linux Ext4 Loop)**: If modifying the image, do NOT use macOS Homebrew `debugfs write`. Instead, mount the ext4 image via a true Linux loop device on the box via `boxssh` where the Linux ext4 kernel driver properly unshares deduplicated `shared_blocks` on copy-on-write, preserving `services.jar` checksum integrity.
