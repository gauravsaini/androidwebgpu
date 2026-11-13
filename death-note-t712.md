# DEATH NOTE — T712

Lane: Track T712 — hals.conf fix for the SensorService.onBootPhase block + fly for the first queued buffer + first present dispatch (Mac agyx)  
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 14.0 GiB available [>= 8 GiB threshold]; Step 1 verified all 12 staged inputs byte-for-byte without network fetch; Step 2 implemented Option B fix on Linux box via loopback mount & e2fsprogs debugfs + e2fsck -f -n [clean exit 0]: freed 2 inodes from vendor.img capacity 736 by removing dummy `/odm/etc/group` and `/odm/etc/passwd`, created `/odm/etc/sensors` [0755, 0:2000, `u:object_r:vendor_configs_file:s0`] and `/odm/etc/sensors/hals.conf` [0644, 0:2000, `u:object_r:vendor_configs_file:s0`] referencing `android.hardware.sensors@2.1-impl.ranchu.so`, updated `/etc/sensors/hals.conf`, binary-patched `android.hardware.sensors@2.1-impl.ranchu.so` at offset `0x10270` [`MultihalSensorsC2`] and offset `0x10ed4` [`MultihalSensorsC1`] with `0x14000097` [`b +0x97`] skipping missing goldfish pipe assert, repacked `t712-vendor.img` 134,217,728 B, SHA `095386ab…`; repacked CPIO initramfs `t712-fix.cpio.gz` 53,502,698 B, SHA `297d0a44…`; realigned DTB `t712-modified.dtb` 2,049 B, SHA `58f5acb3…` to `0x1133062ea`; Step 3a smoke flight on Mac QEMU 11.1.2 [host 125.19s, guest 124.71s, exit 0, 10,292 lines, SHA `51b07421…`] proved `SensorsMultiHal` loaded `hals.conf` and registered `android.hardware.sensors.ISensors/default` with 0 SIGABRT, zero `Failed to load subHal config file` errors, 0 avc denials for sensorservice; Step 3b flight-ab canonical control on `05c43917…` [host 25.04s, guest 19.96s, exit 0, 573 lines, log SHA `fc7f2a2c…`] re-proved missing `vda` polling timeout and `InitFatalReboot` due to lack of `CONFIG_VIRTIO_BLK=y`; Step 3b flight-01 ran full 2400s guest window [host 2402.81s, guest 2401.98s, exit 0, 51,719 lines, log SHA `140f36f6…`]; guest UART proved `SensorsMultiHal` successfully published `ISensors/default` in device VINTF manifest without crashes or config errors; however, in `system_server`, `startSensorServiceNative()` called during boot phase 200 [`PHASE_WAIT_FOR_DEFAULT_DISPLAY`] attempted native initialization and parked the main thread in `LockSupport.park` at `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()`; `Watchdog` killed `system_server` with SIGKILL across 10 instances after 82-83s each [PIDs 2496, 3225, 3876, 4525, 5162, 5808, 6455, 7110, 7759, 8416]; queueBuffer 0, presentDisplay 0)  
UTC close: 2026-11-13T07:28:00Z  
Worktree: /Users/Shared/wt-track712-hals-conf-fix  
Branch: t712  

## What ended

Track T712 was opened to ship a valid `/odm/etc/sensors/hals.conf` (+ stub/mock subHal) so `SensorsMultiHal` initializes, `startSensorServiceNative()` returns, `SensorService.onBootPhase` unblocks, and fly the full 2400 s guest window for the first `queueBuffer` and first present dispatch on the T711 stack on Mac QEMU 11.1.2.

1. **Step 0 — Mac Disk Gate: PASS**:
   - 14.0 GiB available on `/System/Volumes/Data` at launch; verified >= 8 GiB threshold. No WAIT-DISK.
2. **Step 1 — Verify Inputs: PASS**:
   - All 12 staged inputs verified byte-for-byte and SHA-256 against specifications with zero network fetch. Authoritative copies in box vault verified.
3. **Step 2 — Option B Fix Implemented on Linux Box & Repacked: PASS**:
   - Superblock inode exhaustion resolved: deleted dummy 0-byte files `/odm/etc/group` and `/odm/etc/passwd` using `debugfs -w` on the Linux box.
   - Created `/odm/etc/sensors` (0755, 0:2000, `u:object_r:vendor_configs_file:s0`) and `/odm/etc/sensors/hals.conf` (0644, 0:2000, `u:object_r:vendor_configs_file:s0`).
   - Populated `/etc/sensors/hals.conf` with `android.hardware.sensors@2.1-impl.ranchu.so`.
   - Binary patched `android.hardware.sensors@2.1-impl.ranchu.so` at offset `0x10270` (`MultihalSensorsC2`) and offset `0x10ed4` (`MultihalSensorsC1`) with `0x14000097` (`b +0x97`), cleanly skipping goldfish pipe transport assert. Verified clean with `e2fsck -f -n`.
   - Repacked `t712-vendor.img` (134,217,728 B, SHA `095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf`).
   - Repacked CPIO initramfs `t712-fix.cpio.gz` (53,502,698 B, SHA `297d0a447cd56cbc8b985b11956e95563d5965640abae32985ae1ee26f559155`).
   - Re-aligned DTB `t712-modified.dtb` (2,049 B, SHA `58f5acb3b250f64fb7426cb090cce1728a153281dfdb40d8a9b6863a949e41d8`) to `linux,initrd-end = <0x01 0x133062ea>`.
   - Synchronized all images to box vault `/mnt/sdb1/kernel-critical/lane-images/`.
4. **Step 3a — Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 125.19 s, guest 124.71 s, exit 0, 10,292 lines, SHA `51b074214542e40df338745b0289ef2a7a8ac07c90a62f874236f87770f94964`.
   - External `/dev/block/vda` mounted as `/system` Success at guest 9.51 s (line 529).
   - `fb0: simplefb registered!` at guest 0.32 s.
   - `SensorsMultiHal` loaded `hals.conf` and registered `android.hardware.sensors.ISensors/default` with 0 SIGABRT, zero `Failed to load subHal config file` errors, 0 avc denials.
5. **Step 3b — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 25.04 s, guest 19.96 s, exit 0, 573 lines, SHA `fc7f2a2cdae4e86d046daf90053275b613be7373ac3069fb6b63191ad327a45e`.
   - Re-proved `CONFIG_VIRTIO_BLK=y` requirement: partition polling timed out and triggered `InitFatalReboot` at guest 19.84 s (line 563).
6. **Step 3b — Flight-01 (Full 2400 s Window): PASS Execution**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2402.81 s, guest 2401.98 s, exit 0, 51,719 lines, SHA `140f36f69eab75f48a3391e6f6beb37fb14f182163adb342e7902308b7c31125`.
   - Re-verified guest boot pipeline: Freeing kernel memory @3.44s (line 188), Init first stage @9.10s (line 520), VDA mount @9.50s (line 529).
   - Zero primeCache aborts, zero composer failures, zero database crashes.

## Why the verdict is FAIL

Under the proving criteria of Track T712:
1. **`sys.boot_completed=1` Not Observed (0)**:
   `sys.boot_completed` remained unset throughout the 2400 s window.
2. **Watchdog Killing system_server on Native SensorService Hang**:
   While `SensorsMultiHal` successfully loaded `hals.conf` and registered `android.hardware.sensors.ISensors/default` without error, the in-process native `startSensorServiceNative()` invoked by `SensorService.java` at boot phase 200 (`PHASE_WAIT_FOR_DEFAULT_DISPLAY`) attempted native sensor subsystem initialization (governed by property `system_init.startsensorservice=true`).
   This native call did not complete in-process, leaving the main thread parked at `jdk.internal.misc.Unsafe.park` in `FutureTask.awaitDone` on `waitForFutureNoInterrupt`.
   `Watchdog` killed `system_server` with SIGKILL across 10 instances after 82-83 seconds each (PIDs 2496, 3225, 3876, 4525, 5162, 5808, 6455, 7110, 7759, 8416).
3. **QueueBuffer & Present Dispatch Not Reached (0)**:
   `queueBuffer`: 0, `presentDisplay`: 0, `validateDisplay`: 0.

## Exact First Failing Link & Exact Next Fix (Track T713)

1. **First Failing Link**:
   `system_server` main thread hangs in `com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` waiting for `startSensorServiceNative()`. In `libandroid_servers.so`, `NativeSensorService` checks `system_init.startsensorservice` (defaults to `true`) and initiates native sensor daemon connection, which halts or hangs in-process. `Watchdog` kills `system_server` every ~82-83 seconds.

2. **Exact Next Fix (Track T713)**:
   - **Option C (System property bypass for startSensorServiceNative)**:
     Set `system_init.startsensorservice=0` (or `false`) via `/vendor/build.prop`, `/default.prop`, or early-ramdisk `/prop.default`. In `libandroid_servers.so` at offset `0xaaee8`, `tbz w20, #0, aafec` tests this boolean property: when `0`/`false`, it immediately branches to the epilogue `0xaafec` and returns cleanly without starting the native daemon or blocking on sensor HAL connection.
   - This will allow `startSensorServiceNative()` to return immediately, unblocking `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` and permitting `startOtherServices()` to advance directly to WindowManager, ActivityManager, display initialization, and Launcher3 execution.
