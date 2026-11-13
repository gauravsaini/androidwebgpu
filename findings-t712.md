# Findings — Track T712: hals.conf fix for the SensorService.onBootPhase block + fly for the first queued buffer + first present dispatch (Mac agyx)

Date: 2026-11-13  
Worktree: `/Users/Shared/wt-track712-hals-conf-fix`  
Branch: `t712`  
Predecessor: Track T711 driver-certified honest FAIL (commit `e5c98a4`: SELinux-domain fix built and flown; dummy `sensorservice` registered on attempt 0 under `u:r:system_server:s0` with ZERO avc denials; `DeviceStateProviderImpl.setStateConditions` unblocked, reaching boot phase 200; `SensorsMultiHal` failed to load `/odm/etc/sensors/hals.conf`; `SensorService.onBootPhase` blocked on `FutureTask.get()`; Watchdog killed `system_server` across 9 instances; queueBuffer 0, presentDisplay 0).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 14.0 GiB available on `/System/Volumes/Data` [>= 8 GiB threshold]; Step 1 verified all 12 staged inputs byte-for-byte against specifications with zero network fetch; Step 2 implemented Option B fix on the Linux box via loopback mount & e2fsprogs debugfs + e2fsck -f -n [clean exit 0]: freed 2 inodes from vendor.img superblock capacity 736 by removing 0-byte dummy `/odm/etc/group` and `/odm/etc/passwd`, created `/odm/etc/sensors` [0755, 0:2000, `u:object_r:vendor_configs_file:s0`] and `/odm/etc/sensors/hals.conf` [0644, 0:2000, `u:object_r:vendor_configs_file:s0`] referencing `android.hardware.sensors@2.1-impl.ranchu.so`, updated `/etc/sensors/hals.conf`, and binary-patched `android.hardware.sensors@2.1-impl.ranchu.so` at offset `0x10270` [`MultihalSensorsC2`] and offset `0x10ed4` [`MultihalSensorsC1`] with `0x14000097` [`b +0x97`] to cleanly skip missing goldfish pipe assert and thread loop, repacked `t712-vendor.img` 134,217,728 B, SHA `095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf`; repacked CPIO initramfs `t712-fix.cpio.gz` 53,502,698 B, SHA `297d0a447cd56cbc8b985b11956e95563d5965640abae32985ae1ee26f559155`; realigned DTB `t712-modified.dtb` 2,049 B, SHA `58f5acb3b250f64fb7426cb090cce1728a153281dfdb40d8a9b6863a949e41d8` to `0x1133062ea`; Step 3a smoke flight on Mac QEMU 11.1.2 [host 125.19s, guest 124.71s, exit 0, 10,292 lines, SHA `51b07421…`] proved `SensorsMultiHal` loaded `hals.conf` and registered `android.hardware.sensors.ISensors/default` with servicemanager with 0 SIGABRT, zero `Failed to load subHal config file` errors, 0 avc denials for sensorservice; Step 3b flight-ab canonical control on `05c43917…` [host 25.04s, guest 19.96s, exit 0, 573 lines, SHA `fc7f2a2cdae4e86d046daf90053275b613be7373ac3069fb6b63191ad327a45e`] re-proved missing `vda` polling timeout and `InitFatalReboot` due to lack of `CONFIG_VIRTIO_BLK=y`; Step 3b flight-01 ran full 2400s guest window [host 2402.81s, guest 2401.98s, exit 0, 51,719 lines, log SHA `140f36f69eab75f48a3391e6f6beb37fb14f182163adb342e7902308b7c31125`]; guest UART proved `SensorsMultiHal` successfully published `ISensors/default` in device VINTF manifest without crashes or config errors; however, in `system_server`, `startSensorServiceNative()` called during boot phase 200 [`PHASE_WAIT_FOR_DEFAULT_DISPLAY`] attempted native initialization and parked the main thread in `LockSupport.park` at `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()`; `Watchdog` killed `system_server` with SIGKILL across 10 instances after 82-83s each [PIDs 2496, 3225, 3876, 4525, 5162, 5808, 6455, 7110, 7759, 8416]; queueBuffer 0, presentDisplay 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, the primeCache abort from T707 remained completely cured (0 occurrences of `output buffer not gpu writeable`, 0 `failed to create composer client`, 0 `Abort message:`). The T708/T709 database pre-creation fix held throughout the flight (0 `SQLiteCantOpenDatabaseException`). The T711 SELinux domain fix held completely: `dummy_sensorservice` registered `sensorservice` with `servicemanager` on attempt 0 under domain `u:r:system_server:s0` with 0 avc denials. The T712 Option B fix cured the `SensorsMultiHal` configuration failure: `/odm/etc/sensors/hals.conf` was loaded without error (0 occurrences of `Failed to load subHal config file`), `android.hardware.sensors.ISensors/default` registered in VINTF with 0 SIGABRT (down from subHal crash). However, when `system_server` entered `startOtherServices()` at boot phase 200 (`PHASE_WAIT_FOR_DEFAULT_DISPLAY`), Java `SensorService.onBootPhase(SensorService.java:89)` blocked on `FutureTask.get()` waiting for native `startSensorServiceNative()`. The native sensor initialization halted/hung in native binder/device connect, leaving the main thread parked in `LockSupport.park` for 82-83s and triggering `Watchdog` SIGKILL across 10 consecutive instances (PIDs 2496, 3225, 3876, 4525, 5162, 5808, 6455, 7110, 7759, 8416). Thus `sys.boot_completed` remained unset. In canonical control flight `flight-ab`, `boot_completed` remained unset until clean reboot on vda polling timeout at guest 19.96s.

WALL-TYPE-TAG: guest type-1 on FAIL  
Rust par bhi lagu: haan — the Android userspace binder service manager SELinux permissions, `SensorService` in-process JNI lifecycle, `SensorsMultiHal` HAL requirements, and Zygote lifecycle are identical regardless of whether QEMU or a Rust emulator runs the guest.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.435060 s (line 188: `[    3.435060] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.504400 s (line 186: `[    3.504400] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.436176 s (line 188: `[    3.436176] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 9.097521 s (line 520: `[    9.097521] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 9.314894 s (line 521: `[    9.314894] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 9.113759 s (line 520: `[    9.113759] init: init first stage started!`).

system_server:
- In flight `flight-01`, a total of 10 `system_server` instances were spawned by Zygote:
  - Instance 1 (PID 2496): start line 10275 (`Entered the Android system server!`) -> Watchdog killed line 13945 after 82s main thread hang.
  - Instance 2 (PID 3225): start line 14773 (`Entered the Android system server!`) -> Watchdog killed line 17721 after 83s main thread hang.
  - Instance 3 (PID 3876): start line 18805 (`Entered the Android system server!`) -> Watchdog killed line 21714 after 83s main thread hang.
  - Instance 4 (PID 4525): start line 22869 (`Entered the Android system server!`) -> Watchdog killed line 25749 after 83s main thread hang.
  - Instance 5 (PID 5162): start line 26876 (`Entered the Android system server!`) -> Watchdog killed line 29785 after 83s main thread hang.
  - Instance 6 (PID 5808): start line 31451 (`Entered the Android system server!`) -> Watchdog killed line 34350 after 83s main thread hang.
  - Instance 7 (PID 6455): start line 35713 (`Entered the Android system server!`) -> Watchdog killed line 38661 after 83s main thread hang.
  - Instance 8 (PID 7110): start line 40049 (`Entered the Android system server!`) -> Watchdog killed line 42931 after 83s main thread hang.
  - Instance 9 (PID 7759): start line 44265 (`Entered the Android system server!`) -> Watchdog killed line 47162 after 83s main thread hang.
  - Instance 10 (PID 8416): start line 48843 (`Entered the Android system server!`) -> Watchdog killed line 51719 after 82s main thread hang (at flight cutoff).
- In control flight `flight-ab`, `system_server` did not fork (0) because the canonical kernel failed to mount `/system` from `/dev/block/vda` due to lack of `CONFIG_VIRTIO_BLK=y`, triggering `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- `queueBuffer`: **0**

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block nodes, but failed to detect `/dev/block/vda` due to absence of `CONFIG_VIRTIO_BLK=y`. At guest 19.84 s, Android init timed out polling vda (line 563: `partition(s) not found after polling timeout: vda`) and triggered `InitFatalReboot` (signal 6, line 565). QEMU cleanly terminated under `-no-reboot` at guest 19.96 s (host 25.04 s, exit code 0, 573 lines, log SHA-256 `fc7f2a2cdae4e86d046daf90053275b613be7373ac3069fb6b63191ad327a45e`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` repeatedly terminated during `startOtherServices` before reaching Home activity launch.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T712 executed the Option B fix for the `SensorsMultiHal` `/odm/etc/sensors/hals.conf` missing file wall diagnosed in Track T711. In T711, `SensorsMultiHal` failed with `Failed to load subHal config file: /odm/etc/sensors/hals.conf`, preventing `startSensorServiceNative()` from returning, parking `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` on `FutureTask.get()`, and causing 9 Watchdog kills.

### Key Accomplishments & Evidence
1. **Option B Fix Implemented on Linux Box (Step 2 PASS)**:
   - On the Linux box, mounted `t708-vendor.img` via loopback and inspected filesystem structure. Superblock had all 736 inodes allocated.
   - Deleted unused 0-byte dummy files `/odm/etc/group` and `/odm/etc/passwd` using `/usr/sbin/debugfs -w` to free 2 inodes.
   - Created `/odm/etc/sensors` (dir mode 0755, uid 0, gid 2000, context `u:object_r:vendor_configs_file:s0`) and `/odm/etc/sensors/hals.conf` (file mode 0644, uid 0, gid 2000, context `u:object_r:vendor_configs_file:s0`) containing `android.hardware.sensors@2.1-impl.ranchu.so`.
   - Populated `/etc/sensors/hals.conf` with `android.hardware.sensors@2.1-impl.ranchu.so`.
   - Identified goldfish pipe transport dependency in ranchu subHal: QEMU virt-machine lacks goldfish pipe device `/dev/goldfish_pipe`, which caused line-53 assert and SIGABRT in initial smoke test.
   - Disassembled `android.hardware.sensors@2.1-impl.ranchu.so` and binary-patched both constructor entry points:
     - `MultihalSensorsC2` at file offset `0x10270`: patched with `0x14000097` (`b +0x97`), jumping directly to clean epilogue and skipping pipe connect assert.
     - `MultihalSensorsC1` at file offset `0x10ed4`: patched with `0x14000097` (`b +0x97`), jumping directly to clean epilogue.
   - Verified filesystem integrity with `e2fsck -f -n` (clean exit code 0, 736/736 files, 29165/32768 blocks).
   - Repacked `t712-vendor.img` (134,217,728 B, SHA `095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf`).
   - Repacked CPIO initramfs `t712-fix.cpio.gz` (53,502,698 B, SHA `297d0a447cd56cbc8b985b11956e95563d5965640abae32985ae1ee26f559155`).
   - Re-aligned DTB `t712-modified.dtb` (2,049 B, SHA `58f5acb3b250f64fb7426cb090cce1728a153281dfdb40d8a9b6863a949e41d8`) to `linux,initrd-end = <0x01 0x133062ea>`.
   - Synchronized all images to box vault `/mnt/sdb1/kernel-critical/lane-images/`.

2. **Smoke Flight (Step 3a PASS)**:
   - Flown on Mac QEMU 11.1.2: host 125.19s, guest 124.71s, exit 0, 10,292 lines, SHA `51b074214542e40df338745b0289ef2a7a8ac07c90a62f874236f87770f94964`.
   - External `/dev/block/vda` mounted as `/system` Success at guest 9.51s (line 529).
   - `fb0: simplefb registered!` at guest 0.32s.
   - `SensorsMultiHal` started cleanly:
     - `0 occurrences` of `Failed to load subHal config file: /odm/etc/sensors/hals.conf` (cured!).
     - Line 9739: `[  102.684771] servicemanager: Found android.hardware.sensors.ISensors/default in device VINTF manifest.`
     - Zero SIGABRT, zero crash in `sensors-service.multihal`.
     - `dummy_sensorservice` registered with servicemanager on attempt 0 under `u:r:system_server:s0` with ZERO avc denials.

3. **Canonical Control Flight-ab (Step 3b PASS)**:
   - Flown on canonical kernel `05c43917…`: host 25.04s, guest 19.96s, exit 0, 573 lines, SHA `fc7f2a2cdae4e86d046daf90053275b613be7373ac3069fb6b63191ad327a45e`.
   - Re-proved `CONFIG_VIRTIO_BLK=y` requirement: partition polling timed out on vda at guest 19.84s (line 563) and triggered `InitFatalReboot` (signal 6, line 565).

4. **Full 2400 s Flight-01 (Step 3b PASS Execution)**:
   - Flown on rebuilt kernel `53d9563e…`: host 2402.81s, guest 2401.98s, exit 0, 51,719 lines, SHA `140f36f69eab75f48a3391e6f6beb37fb14f182163adb342e7902308b7c31125`.
   - Freeing kernel memory @3.44s (line 188), Init first stage @9.10s (line 520), VDA mount @9.50s (line 529).
   - Zero primeCache aborts, zero composer client failures, zero SQLite database crashes.
   - `hals.conf` loaded without error; `ISensors/default` registered in VINTF manifest.

5. **Exact Mechanism of Failure Uncovered**:
   - Despite `SensorsMultiHal` successfully loading `hals.conf` and publishing `ISensors/default`, in `system_server`, `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` remained blocked on `FutureTask.get()`.
   - In `libandroid_servers.so`:
     `startSensorServiceNative()` calls `new NativeSensorService(env, thiz)`.
     At `0xaae74`, `NativeSensorService` checks the property:
     `bool start = android::base::GetBoolProperty("system_init.startsensorservice", true);`
     Because `system_init.startsensorservice` defaults to `true`, it calls native `SensorService` in `libsensorservice.so`, which attempts to connect to the sensor HAL via `SensorDevice::getInstance()`.
   - In native `SensorService`, this call parks or hangs, preventing `startSensorServiceNative()` from returning.
   - Consequently, the main thread in `system_server` remained parked at `jdk.internal.misc.Unsafe.park` in `FutureTask.awaitDone` for 82-83 seconds, triggering `Watchdog` SIGKILL across 10 consecutive instances (PIDs 2496, 3225, 3876, 4525, 5162, 5808, 6455, 7110, 7759, 8416).

---

## Exact First Failing Link & Exact Next Fix (Track T713)

1. **First Failing Link**:
   `system_server` main thread hangs in `com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` waiting for `startSensorServiceNative()`. In `libandroid_servers.so`, `NativeSensorService` reads `system_init.startsensorservice` (which defaults to `true`) and initiates native sensor subsystem connection. Even with `SensorsMultiHal` loaded and published, the native sensor service fails to complete initialization in-process, leaving `mSensorStartDone` uncompleted. `Watchdog` detects main thread blocked for >60s and terminates `system_server` with SIGKILL every ~82-83s.

2. **Exact Next Fix (Track T713 — Priority C Bypass)**:
   - **Option C (System property bypass for startSensorServiceNative)**:
     Set `system_init.startsensorservice=0` (or `false`) via `/vendor/build.prop`, `/default.prop`, or early-ramdisk `/prop.default`. In `libandroid_servers.so` at offset `0xaaee8`, `tbz w20, #0, aafec` tests this boolean property: when `0`/`false`, it immediately branches to the epilogue `0xaafec` and returns cleanly without starting the native daemon or blocking on sensor HAL connection.
   - This will allow `startSensorServiceNative()` to return immediately, unblocking `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` and permitting `startOtherServices()` to advance directly to WindowManager, ActivityManager, display initialization, and Launcher3 execution.
