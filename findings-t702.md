# Findings — Track T702: Launcher3 crash-loop cure (Mac agyx, cure the DotRenderer NPE so Launcher3 completes onCreate and queues the first buffer)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track702-launcher3-crashloop-cure`
Branch: `t702`, from `5d4b305` (the T701 commit)
Predecessor: T701 driver-certified PASS (present-path diagnosis isolated root cause to Launcher3 DotRenderer NPE crash loop, queueBuffer 0, 0 present).

## Verdict

Verdict: FAIL

(Honest FAIL: Launcher3 crash loop persisted across 90 instances because ART executed pre-compiled AOT `Launcher3QuickStep.odex` and `Launcher3QuickStep.vdex` from `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/` instead of the repacked APK's patched `classes7.dex`; SurfaceFlinger held 0 queued buffers, 0 present dispatch).

## Required markers

sys.boot_completed: observed (1) — `sys.boot_completed=1` was observed and processed by init in flight `flight-01` (`flights/flight-01/uart.log`). First wave was processed at guest 1405.952102 s (line 69387: `[ 1405.952102] init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1227)`), triggering per-boot cleanup (line 69441), bootstat actions (line 69888), and wifi tracing (lines 70407, 70440). A second wave occurred at guest 1759.748306 s (line 86796) after a system_server restart, triggering dmesgd (lines 86951, 86957) and logd actions. In control flight `flight-ab`, `boot_completed=` was empty throughout the 240 s window.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace boot, ART execution, and display pipeline paths are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 24.213859 s (line 185: `[   24.213859] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 24.580750 s (line 186: `[   24.580750] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `flight-01` (Route B kernel `3f45272f…`): observed at guest 27.719336 s (line 595: `[   27.719336] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 28.221222 s (line 600: `[   28.221222] init: init first stage started!`).

Launcher3 crashes / queueBuffer:
- Exactly **90 distinct crash blocks** of `Process: com.android.launcher3` in `flights/flight-01/uart.log` (lines 74202 to 120574).
- First crash: line 74202 (PID 12244) at guest 11-10 23:34:43.341 (`Caused by: java.lang.NullPointerException: Attempt to invoke virtual method 'int android.graphics.Bitmap.getHeight()' on a null object reference at com.android.launcher3.icons.DotRenderer.<init>(DotRenderer.java:64)`).
- Last crash: line 120574 (PID 18676) at guest 11-10 23:50:22.891.
- `queueBuffer` count: **0** (SurfaceFlinger held 0 queued client buffers).
- Root cause of patch bypass: Pre-compiled AOT directory `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/` was present on `system.img` with `Launcher3QuickStep.odex` (ELF 64-bit ARM aarch64 shared object, 119,728 B) and `Launcher3QuickStep.vdex` (278,604 B). ART executed the native machine code from the unpatched `.odex` file, skipping dexopt on the repacked APK (line 32174: `ArtService: Dexopt result: ... status=SKIPPED ...`).

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- Composer `executeCommands`: **0**
- Composer 2.1 received `BC_FREE_BUFFER` ×75, 0 display commands.

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, failed mount_all at guest ~19 s / 43.76 s (lines 1357–1358: `mkdir() failed on /cache/recovery: Read-only file system`), and continuously looped on missing `/system/bin/toybox` (lines 1359–13294) until the 240 s guest cutoff (guest 243.283 s, host 245.59 s). Mount_all wall is completely unchanged.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T702 set out to cure the Launcher3 DotRenderer `NullPointerException` crash loop identified by T701, aiming to achieve the first queued buffer (`queueBuffer > 0`) and first SurfaceFlinger present dispatch.

### What Was Done
1. **Fix Design**: Disassembled and audited `Launcher3QuickStep.apk` (`classes7.dex`). Evaluated all three proposed routes:
   - *Option B (notification-dots bypass)*: Refuted by bytecode disassembly of `com.android.launcher3.DeviceProfile.<init>` (offsets `0x14904e` and `0x14905e`), which proved `createDotRenderer()` is called unconditionally during InvariantDeviceProfile grid construction with zero settings or property checks.
   - *Option C (FallbackHome override)*: Refuted because `FallbackHome` is a transient Direct-Boot placeholder that automatically calls `finish()` as soon as the user credential storage unlocks and a real home app exists.
   - *Option A (DotRenderer null-bitmap guard + BitmapRenderer fallback)*: Selected. Designed and applied a dual-layer bytecode patch to `classes7.dex`:
     - Offset `0x199a90`: `USE_HARDWARE_BITMAP = false`
     - Offset `0x199a10`: `if-nez` bypassed with `nop`s, falling back to software `Bitmap.Config.ARGB_8888`
     - Offset `0x19b112`: `invoke-virtual getHeight` replaced with `move v2, v0` null-guard (height = pill size `v0`, zeroing any dereference of null `Bitmap v2`).
2. **Repack & Signing**: Aligned `Launcher3QuickStep.apk` (4-byte zipalign), signed with `apksigner` (v1+v2+v3, SHA `8a28e1db…`), injected into `guest-inputs/system.img` with `debugfs` (preserving mode 0100644, uid 0, gid 0, SELinux context `u:object_r:system_file:s0`), and validated clean with `e2fsck`. Repacked `system.img` SHA: `8cab6e8987615ad2b51fc659418de0fa34c0d1706b16022175aefb5960e7ee39`.
3. **Flight-01 (2400 s target)**: Successfully flew for 2404.78 host seconds (2402.63 guest seconds). Init processed `sys.boot_completed=1` at guest 1405.95 s.
4. **Diagnostic Breakthrough**: The flight revealed why the repacked APK did not cure the crash loop: `system/system_ext/priv-app/Launcher3QuickStep/` contains a pre-compiled AOT directory `oat/arm64/` with `Launcher3QuickStep.odex` (119,728 B) and `Launcher3QuickStep.vdex` (278,604 B). Because the `.odex` file was present on disk, ART's `ArtService` logged `status=SKIPPED` and executed native machine code directly from the unpatched `.odex` file, completely ignoring the repacked APK's patched DEX bytecode. Consequently, `com.android.launcher3` crashed 90 times across the flight with the identical DotRenderer NPE.
5. **A/B Canonical Control (`flight-ab`)**: Flown on canonical kernel `05c43917…` for 240 s guest target (host 245.59 s). Replicated the canonical mount_all wall at guest ~19 s / 43.76 s, confirming no regression in system image boot mounting.

---

## Input Verification Table

All inputs verified in `/Users/Shared/wt-track702-launcher3-crashloop-cure` prior to execution:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t701.md` | `b247c8d142bdaf7c6aa863879dd0c6882a1ae1380631b24b8f19ef5e15f7e15f` | `b247c8d142bdaf7c6aa863879dd0c6882a1ae1380631b24b8f19ef5e15f7e15f` | 26,318 B | MATCH (PASS) |
| `findings-t700.md` | `d92e2c08b85e56af6560a9ee6cfd8d1f96f64ecb3597e5e55cadb03e6f95585e` | `d92e2c08b85e56af6560a9ee6cfd8d1f96f64ecb3597e5e55cadb03e6f95585e` | 17,748 B | MATCH (PASS) |
| `findings-t699.md` | `c84167c485ee13ffb7c00f0aafdcaa3d4953585f9bc53333b8b423cdafaf1926` | `c84167c485ee13ffb7c00f0aafdcaa3d4953585f9bc53333b8b423cdafaf1926` | 17,329 B | MATCH (PASS) |
| `evidence/t700-flight-01-uart.log` | `7ef491cc1b6740393e6ba2f08319f53b5b7154ac73a138d437acb3eb2d37f6b5` | `7ef491cc1b6740393e6ba2f08319f53b5b7154ac73a138d437acb3eb2d37f6b5` | 15,936,275 B | MATCH (PASS) |
| `evidence/t700-flight-ab-uart.log` | `8ac276e1a895fd0a6dc2cecf3b99414ea49911143313be7a2811f95929fed738` | `8ac276e1a895fd0a6dc2cecf3b99414ea49911143313be7a2811f95929fed738` | 1,946,273 B | MATCH (PASS) |
| `guest-inputs/routeB-Image` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | 20,126,208 B | MATCH (PASS) |
| `guest-inputs/t699-systemserver-fb0-fix.cpio.gz` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | 958,780,579 B | MATCH (PASS) |
| `guest-inputs/modified.dtb` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B | MATCH (PASS) |
| `guest-inputs/canonical-Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B | MATCH (PASS) |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| Original `system.img` (pre-fix) | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B | MATCH (PASS) |
| Original `Launcher3QuickStep.apk` | `7626f26b8fc7b905d551edff3058335a67a5b8779385a90e447ce5ef349f9563` | `7626f26b8fc7b905d551edff3058335a67a5b8779385a90e447ce5ef349f9563` | 28,922,784 B | MATCH (PASS) |

---

## Fix Design: Evaluation of Options

### 1. Option B: Notification-Dots Bypass (Refuted)
- **Hypothesis**: Disable notification dots via `Settings.Secure.NOTIFICATION_BADGING = 0` or overlay configuration `config_useNotificationDots = false` to bypass `DotRenderer` instantiation.
- **Bytecode Reality**: Audit of `classes7.dex` in `Launcher3QuickStep.apk` revealed that `com.android.launcher3.DeviceProfile.<init>` directly calls `createDotRenderer()`:
  ```
  14904e: invoke-static {v5, v6, v7}, DeviceProfile;.createDotRenderer:(Context;ILandroid/util/SparseArray;)DotRenderer;
  149056: iput-object v6, v0, DeviceProfile;.mDotRendererWorkSpace:DotRenderer;
  14905e: invoke-static {v5, v6, v7}, DeviceProfile;.createDotRenderer:(Context;ILandroid/util/SparseArray;)DotRenderer;
  149066: iput-object v6, v0, DeviceProfile;.mDotRendererAllApps:DotRenderer;
  ```
  This occurs unconditionally inside the constructor of `DeviceProfile` during `InvariantDeviceProfile.initGrid()` (lines 668 and 676 of `DeviceProfile.java`). It does NOT check any settings or feature flags. Option B cannot prevent `DotRenderer.<init>` from being executed.

### 2. Option C: FallbackHome Default-Home Override (Refuted)
- **Hypothesis**: Set default home intent to `com.android.settings/.FallbackHome`.
- **System Reality**: `FallbackHome` in AOSP is designed strictly as a temporary direct-boot placeholder before user storage decryption. When the user storage unlocks (observed in UART at line 95794: `FallbackHome: User unlocked and real home found; let's go!`), `FallbackHome` calls `finish()` and unregisters itself, deferring to the real home app (`Launcher3QuickStep`). It does not provide an interactive workspace or submit regular display buffers.

### 3. Option A: DotRenderer Null-Bitmap Guard + Hardware Bitmap Fallback (Selected)
- **Disassembly Analysis**:
  - `DotRenderer.<init>` at offset `0x19b0fe` calls `ShadowGenerator$Builder.createPill:(II)Landroid/graphics/Bitmap;` (pill size `v0`).
  - `createPill` delegates to `BitmapRenderer.createHardwareBitmap:(IILBitmapRenderer;)Bitmap;` (offset `0x199a00`).
  - At offset `0x199a10`, `createHardwareBitmap` checks `USE_HARDWARE_BITMAP` (static boolean at offset `0x199a90`). If true, it records into an Android `Picture` and calls `Bitmap.createBitmap(picture)`.
  - Under headless QEMU TCG SwiftShader, `ThreadedRenderer` fails to read the picture with error `-30001` (UART line 96219: `W ThreadedRenderer: Error reading image: -30001!`) and returns `null`.
  - At offset `0x19b112`, `DotRenderer.<init>` unconditionally executes:
    `6e10 1503 0200` : `invoke-virtual {v2}, Bitmap.getHeight:()I`
    `0a02`           : `move-result v2`
    Because `v2` is `null`, Dalvik/ART throws `NullPointerException`.

---

## Fix Implementation

A dual-layer bytecode patch was applied to `classes7.dex`:

1. **Hardware Bitmap Bypass**:
   - `0x199a90`: Changed `1210` (`const/4 v0, #int 1`) to `1200` (`const/4 v0, #int 0`), forcing `USE_HARDWARE_BITMAP = false`.
   - `0x199a10`: Changed `3900 0700` (`if-nez v0, +0007`) to `0000 0000` (`nop; nop`), forcing fallthrough to `createSoftwareBitmap` (standard `Bitmap.Config.ARGB_8888` allocation).
2. **Null-Bitmap Guard in DotRenderer**:
   - `0x19b112`: Replaced `6e10 1503 0200 0a02` (`invoke-virtual getHeight; move-result v2`) with `0102 0000 0000 0000` (`move v2, v0; nop; nop; nop`). This sets `v2 = v0` (the pill size integer already in register `v0`), completely eliminating the virtual method invocation on the bitmap.

### Repack & Preflight Verification
- `classes7.dex` SHA-1 and Adler-32 checksums recomputed and verified clean with `dexdump -c`.
- Repacked APK aligned to 4-byte boundaries with `zipalign -f -p 4`.
- Repacked APK signed with `apksigner` (v1+v2+v3 enabled).
  - New APK SHA-256: `8a28e1dbb416f1f6a3279082279dd9441db53cad10353892f8a32c08a85cd97c` (25,654,202 bytes).
- Injected into `guest-inputs/system.img` using `debugfs`:
  - Mode: `0100644`
  - UID: `0`, GID: `0`
  - SELinux security context: `u:object_r:system_file:s0`
- Ext4 filesystem verified clean with `e2fsck -f -y`.
- Repacked `system.img` SHA-256: `8cab6e8987615ad2b51fc659418de0fa34c0d1706b16022175aefb5960e7ee39` (2,059,816,960 bytes).
- **Preflight Proof**: Extracted the APK back from `system.img` using `debugfs`, extracted `classes7.dex`, verified `dexdump -c` passed, and verified the patched opcodes at `0x199a10`, `0x199a90`, and `0x19b112` matched exactly.

---

## Flight-01 Execution

- **Command**:
  ```bash
  /opt/homebrew/bin/qemu-system-aarch64 \
    -machine virt,gic-version=2,virtio-mmio-transports=1 -cpu cortex-a57 -m 8192 -smp 4 \
    -kernel guest-inputs/routeB-Image \
    -dtb guest-inputs/modified.dtb \
    -device loader,file=guest-inputs/t699-systemserver-fb0-fix.cpio.gz,addr=0x110000000,force-raw=on \
    -drive if=none,id=t702system,file=guest-inputs/system.img,format=raw,readonly=on \
    -device virtio-blk-device,drive=t702system \
    -append "console=ttyAMA0,115200 earlycon loglevel=8 debug rdinit=/early-ramdisk androidboot.hardware=virt androidboot.selinux=permissive androidboot.veritymode=disabled enforcing=0 printk.ratelimit=0 brd.rd_nr=1 brd.rd_size=524288 androidboot.t702_fix_initramfs_sha256=c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b androidboot.t702_flight_target=2400 androidboot.t702_dtb_sha256=e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389 androidboot.t702_system_sha256=8cab6e8987615ad2b51fc659418de0fa34c0d1706b16022175aefb5960e7ee39 androidboot.t702_kernel_sha256=3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3" \
    -gdb tcp:127.0.0.1:1245 -display none -serial file:flights/flight-01/uart.log -monitor none -no-reboot
  ```
- **Timing**:
  - Started: `2026-11-08T06:43:03Z`
  - Ended: `2026-11-08T07:23:08Z`
  - Host elapsed: 2404.78 s
  - Guest elapsed: 2402.627 s (reached target 2400 s cutoff cleanly)
- **Log metrics**: 120,804 lines, 17,635,126 bytes, SHA-256 `8c9895c97351783e0fffcaafccd70fdb25efa48016d89d5578615bf281381a26`.

---

## Proving Observations & The ART Precompiled ODEX Wall

### 1. Count of FATAL EXCEPTION for `com.android.launcher3`: 90 (Target: 0)
- Observed **90 distinct crash instances** across the 2400 s flight (lines 74202 to 120574).
- PIDs included: 12244, 13597, 14101, 14262, 14353, 14472, 14535, 14577, 14631, 14669, 14700, 14738, 14840, 14906, 15030, 15108, 15157, 15302, 15364, 15436, 15519, 15594, 15670, 15755, 15859, 15966, 16026, 16082, 16132, 16171, 16238, 16278, 16348, 16410, 16485, 16558, 16600, 16636, 16670, 16703, 16739, 16774, 16820, 16864, 16910, 16956, 17035, 17080, 17113, 17144, 17176, 17210, 17243, 17275, 17325, 17372, 17410, 17460, 17495, 17528, 17567, 17603, 17648, 17680, 17716, 17751, 17784, 17817, 17850, 17894, 17940, 17978, 18015, 18052, 18082, 18127, 18164, 18207, 18245, 18277, 18322, 18358, 18391, 18446, 18480, 18518, 18560, 18597, 18636, 18676.
- Every single crash exhibited the identical stack trace:
  ```
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: FATAL EXCEPTION: main
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: Process: com.android.launcher3, PID: 12244
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: java.lang.RuntimeException: Unable to start activity ComponentInfo{com.android.launcher3/com.android.launcher3.uioverrides.QuickstepLauncher}: java.lang.NullPointerException: Attempt to invoke virtual method 'int android.graphics.Bitmap.getHeight()' on a null object reference
  ...
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: Caused by: java.lang.NullPointerException: Attempt to invoke virtual method 'int android.graphics.Bitmap.getHeight()' on a null object reference
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: 	at com.android.launcher3.icons.DotRenderer.<init>(DotRenderer.java:64)
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: 	at com.android.launcher3.DeviceProfile.createDotRenderer(DeviceProfile.java:676)
  11-10 23:34:43.341 12244 12244 E AndroidRuntime: 	at com.android.launcher3.DeviceProfile.<init>(DeviceProfile.java:668)
  ```

### 2. Root Cause of Why the Fix Did Not Land in Runtime
Although `Launcher3QuickStep.apk` was modified and cleanly signed, the Android system image `/system/system_ext/priv-app/Launcher3QuickStep/` contains a pre-compiled AOT directory:
- `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.odex` (119,728 bytes, ELF 64-bit ARM aarch64 shared object)
- `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.vdex` (278,604 bytes, Android vdex version 027)

During package scanning in `system_server`, `ArtService` logged:
```
line 32174: 11-10 23:19:54.028  7769  8207 I ArtService: Dexopt result: [packageName = com.android.launcher3] DexContainerFileDexoptResult{dexContainerFile=/system/system_ext/priv-app/Launcher3QuickStep/Launcher3QuickStep.apk, primaryAbi=true, abi=arm64-v8a, actualCompilerFilter=verify, status=SKIPPED, dex2oatWallTimeMillis=0, dex2oatCpuTimeMillis=0, sizeBytes=0, sizeBeforeBytes=0}
```
Because the pre-compiled `.odex` file was already present on disk, ART's class loader prioritized and loaded the native machine code from the unpatched `Launcher3QuickStep.odex`, completely ignoring the patched `classes7.dex` inside `Launcher3QuickStep.apk`.

### 3. Count of `queueBuffer`: 0 (Target: > 0)
Because Launcher3 crashed before completing `onCreate()`, it never created its window decor view, never attached to `ViewRootImpl`, and never initialized a `BLASTBufferQueue`. SurfaceFlinger received 0 client buffers.

### 4. Count of Present Dispatch Calls: 0 (Target: > 0)
- `presentDisplay`: 0
- `validateDisplay`: 0
- `presentOrValidate`: 0
- Composer `executeCommands`: 0
- SurfaceFlinger's `BLASTSyncEngine` timed out waiting for the `QuickstepLauncher` container (lines 85233, 90654, 91324–91326).

---

## Canonical Control Flight-AB

- **Kernel**: `guest-inputs/canonical-Image` (SHA-256 `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04`, 23,073,280 bytes).
- **Target**: 240 s guest target.
- **Timing**:
  - Started: `2026-11-08T07:24:11Z`
  - Ended: `2026-11-08T07:28:16Z`
  - Host elapsed: 245.59 s
  - Guest elapsed: 243.283 s
- **Log metrics**: 13,294 lines, 2,242,296 bytes, SHA-256 `75feb0116f4e4be366d4120fada697a7792de187e8fc8c9a3cc0bc3044f8f7a7`.
- **Observations**:
  - Line 186: `[   24.580750] Freeing unused kernel memory: 1024K`
  - Line 600: `[   28.221222] init: init first stage started!`
  - Lines 1357–1358: `[   43.767178] init: Command 'mkdir /cache/recovery 0770 system cache' action=post-fs (/system/etc/init/hw/init.rc:588) took 0ms and failed: mkdir() failed on /cache/recovery: Read-only file system`
  - Lines 1359+: `/early-ramdisk[216]: /system/bin/toybox: inaccessible or not found`
  - Replicated the canonical mount_all failure, confirming that the repacked system image did not break the canonical kernel baseline.

---

## Analysis & Exact Next Fix (Track T703)

### The Barrier
The bytecode patch in `classes7.dex` was sound and verified, but it was shadowed by pre-compiled AOT artifacts (`Launcher3QuickStep.odex` and `Launcher3QuickStep.vdex`) residing in `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/`.

### Exact Next Fix for Track T703
In `guest-inputs/system.img`:
1. Delete the pre-compiled AOT files:
   - `debugfs -w -R "rm system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.odex" guest-inputs/system.img`
   - `debugfs -w -R "rm system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/Launcher3QuickStep.vdex" guest-inputs/system.img`
   - `debugfs -w -R "rmdir system/system_ext/priv-app/Launcher3QuickStep/oat/arm64" guest-inputs/system.img`
   - `debugfs -w -R "rmdir system/system_ext/priv-app/Launcher3QuickStep/oat" guest-inputs/system.img`
2. Run `e2fsck -f -y guest-inputs/system.img`.
3. When `system_server` boots, ART will find no pre-compiled `.odex` file, forcing `ArtService` to run dex2oat or JIT directly on `Launcher3QuickStep.apk`'s patched `classes7.dex`.
4. This will allow `Launcher3` to instantiate `DotRenderer` safely without NPE, complete `onCreate()`, create its `BLASTBufferQueue`, and submit the first frame buffer.
