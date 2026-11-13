# Findings — Track T711: SELinux-domain fix for the Priority 1 sensorservice cure + fly for the first queued buffer + first present dispatch (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track711-selinux-domain-fix`  
Branch: `t711`  
Predecessor: T710 driver-certified honest FAIL (commit a8d9901: Priority 1 mock sensorservice binder daemon built and flown; servicemanager SELinux-denied registration 461 times due to scontext=u:r:kernel:s0 with 4,597 avc denials; sensorservice never published on /dev/binder; SystemSensorManager.nativeCreate blocked main thread in DeviceStateProviderImpl.setStateConditions; Watchdog killed system_server across 10 instances; queueBuffer 0, presentDisplay 0).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 9.4 GiB available on /System/Volumes/Data [>= 8 GiB threshold]; Step 1 verified all 12 staged inputs byte-for-byte against specifications with zero network fetch; Step 2 implemented the SELinux domain fix [statically cross-compiled aarch64 `t711_logcat_streamer` 780,496 B, SHA `be5574327a717ee67f3a14424f333eff099172ca2d8636ec1a9157d445f6c27d` transitioning to `u:r:system_server:s0`; dynamically cross-compiled aarch64 `t711_dummy_sensorservice` 11,040 B, SHA `417d73b71b9a62886fdace95af4040ccb7a852e78c2012876dce85eabb41243e` self-transitioning to `u:r:system_server:s0`; early-ramdisk file labeling patched to `u:object_r:system_file:s0`; repacked CPIO initramfs `t711-fix.cpio.gz` 53,502,663 B, SHA `025cac1c987aaf57bfc3356062e3245de49c550b549a24cc73ff2754f040321d`; realigned DTB `t711-modified.dtb` 2,049 B, SHA `2d6d0c11ef22cace73e0f95b5eb8552727c57c614cc07d0fc16ece4613eeb77b` to `0x1133062c7`]; Step 3a smoke flight on Mac QEMU 11.1.2 [host 165.32s, guest 163.35s, exit 0, 11,051 lines, SHA `d1bfd4f6…`] proved external `/dev/block/vda` mounted as `/system` at guest 9.64s, simplefb registered, primeCache skipped with 0 aborts, 0 composer client failures, and proved `dummy_sensorservice` successfully registered `sensorservice` with `servicemanager` on attempt 0 with ZERO sensorservice avc denials [Hard Gate 2 satisfied]; Step 3b flight-ab canonical control on `05c43917…` [host 20.03s, guest 19.19s, exit 0, 574 lines, log SHA `29154ab0…`] re-proved missing `vda` polling timeout and `InitFatalReboot` due to lack of `CONFIG_VIRTIO_BLK=y`; Step 3b flight-01 ran full 2400s guest window [host 2402.91s, guest 2401.41s, exit 0, 51,931 lines, log SHA `6fc97ccc88c4991928934f38d94e0a5831044e79629e1a395327ba0650a5b4ad`]; guest UART proved `dummy_sensorservice` registered on `/dev/binder` under `u:r:system_server:s0` with zero SELinux denials and handled 10 transactions, completely unblocking `DeviceStateProviderImpl.setStateConditions`; `system_server` advanced past bootstrap and core services into `startOtherServices()`, reaching boot phase 200 (`PHASE_WAIT_FOR_DEFAULT_DISPLAY`); at phase 200, `com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)` blocked on `FutureTask.get()` waiting for in-process `startSensorServiceNative()`, which could not complete due to missing `/odm/etc/sensors/hals.conf` subHal configuration (`SensorsMultiHal` failure); `Watchdog` killed `system_server` across 9 instances after 83-86s each [PIDs 2592, 3218, 3890, 4545, 5199, 5879, 6533, 7186, 7847], with the 10th instance [PID 8505] active at cutoff; queueBuffer 0, presentDisplay 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, the primeCache abort from T707 remained completely cured (0 occurrences of `output buffer not gpu writeable`, 0 `failed to create composer client`, 0 `Abort message:`). The T708/T709 database pre-creation fix held throughout the flight (0 `SQLiteCantOpenDatabaseException`). The T711 SELinux domain fix succeeded completely: `dummy_sensorservice` registered `sensorservice` with `servicemanager` on attempt 0 under domain `u:r:system_server:s0` with 0 avc denials (down from 4,597 avc denials in T710), unblocking `SystemSensorManager.nativeCreate` in `DeviceStateProviderImpl.setStateConditions`. However, when `system_server` entered `startOtherServices()` at boot phase 200 (`PHASE_WAIT_FOR_DEFAULT_DISPLAY`), Java `SensorService.onBootPhase(SensorService.java:89)` blocked on `FutureTask.get()` waiting for native `startSensorServiceNative()`. The native sensor subsystem halted because `SensorsMultiHal` failed to load `/odm/etc/sensors/hals.conf` (`Failed to load subHal config file: /odm/etc/sensors/hals.conf`). Consequently, the main thread was parked in `LockSupport.park` for 83s, triggering `Watchdog` SIGKILL across 9 distinct instances (PIDs 2592, 3218, 3890, 4545, 5199, 5879, 6533, 7186, 7847), with the 10th instance (PID 8505) alive at guest cutoff (2401.41s). Thus `sys.boot_completed` remained unset. In canonical control flight `flight-ab`, `boot_completed` remained unset until clean reboot on vda polling timeout at guest 19.10s.

WALL-TYPE-TAG: guest type-1 on FAIL  
Rust par bhi lagu: haan — the Android userspace binder service manager SELinux permissions, `SensorService` in-process JNI lifecycle, `SensorsMultiHal` HAL requirements, and Zygote lifecycle are identical regardless of whether QEMU or a Rust emulator runs the guest.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.462612 s (line 188: `[    3.462612] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.497554 s (line 186: `[    3.497554] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.462612 s (line 188: `[    3.462612] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 9.236034 s (line 520: `[    9.236034] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 9.227220 s (line 521: `[    9.227220] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 9.236034 s (line 520: `[    9.236034] init: init first stage started!`).

system_server:
- In flight `flight-01`, a total of 10 `system_server` instances were spawned by Zygote:
  - Instance 1 (PID 2592): line 10897 (`I/system_server_start( 2592): [1,157425,157425]`) -> Watchdog killed line 14240 after 83s main thread hang.
  - Instance 2 (PID 3218): line 14976 (`I/system_server_start( 3218): [2,394112,394112]`) -> Watchdog killed line 18025 after 83s main thread hang.
  - Instance 3 (PID 3890): line 19164 (`I/system_server_start( 3890): [3,636280,636280]`) -> Watchdog killed line 22147 after 86s main thread hang.
  - Instance 4 (PID 4545): line 23283 (`I/system_server_start( 4545): [4,872431,872431]`) -> Watchdog killed line 26173 after 83s main thread hang.
  - Instance 5 (PID 5199): line 27308 (`I/system_server_start( 5199): [5,1111000,1111000]`) -> Watchdog killed line 30211 after 83s main thread hang.
  - Instance 6 (PID 5879): line 31916 (`I/system_server_start( 5879): [6,1376565,1376565]`) -> Watchdog killed line 34845 after 83s main thread hang.
  - Instance 7 (PID 6533): line 36525 (`I/system_server_start( 6533): [7,1615030,1615030]`) -> Watchdog killed line 39460 after 83s main thread hang.
  - Instance 8 (PID 7186): line 41124 (`I/system_server_start( 7186): [8,1852274,1852274]`) -> Watchdog killed line 44033 after 83s main thread hang.
  - Instance 9 (PID 7847): line 45694 (`I/system_server_start( 7847): [9,2097704,2097704]`) -> Watchdog killed line 48601 after 83s main thread hang.
  - Instance 10 (PID 8505): line 50210 (`I/system_server_start( 8505): [10,2342649,2342649]`) -> Alive at cutoff (guest 2401.41s).
- In control flight `flight-ab`, `system_server` did not fork (0) because the canonical kernel failed to mount `/system` from `/dev/block/vda` due to lack of `CONFIG_VIRTIO_BLK=y`, triggering `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- `queueBuffer`: **0**

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block nodes, but failed to detect `/dev/block/vda` due to absence of `CONFIG_VIRTIO_BLK=y`. At guest 19.08 s, Android init timed out polling vda and triggered `InitFatalReboot` (signal 6, line 565). QEMU cleanly terminated under `-no-reboot` at guest 19.19 s (host 20.03 s, exit code 0, 574 lines, log SHA-256 `29154ab0e1c0bc0ee270e4d387eb10b7918dfb0b8298f16d6625b0ae590440be`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` repeatedly terminated during `startOtherServices` before reaching Home activity launch.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T711 executed the Priority 1 SELinux-domain fix for the `sensorservice` registration diagnosed in T710. In T710, `dummy_sensorservice` was denied the `{ add }` permission by `servicemanager` 461 times because it ran under `u:r:kernel:s0`. In T711, `dummy_sensorservice` and `t711_logcat_streamer` were updated to transition to `u:r:system_server:s0`, and the early-ramdisk file label was set to `u:object_r:system_file:s0`. The fix was packaged into `t711-fix.cpio.gz` and `t711-modified.dtb`.

### Key Accomplishments & Evidence
1. **SELinux Domain Fix Implemented & Verified (Step 2 PASS)**:
   - Statically cross-compiled `t711_logcat_streamer` (780,496 B, SHA `be557432…`), configured to set `/proc/self/attr/exec` and run `/data/dummy_sensorservice` under `u:r:system_server:s0`.
   - Dynamically cross-compiled `t711_dummy_sensorservice` (11,040 B, SHA `417d73b7…`), configured with self-transition to `u:r:system_server:s0`.
   - Early-ramdisk patched to label `/data/dummy_sensorservice` as `u:object_r:system_file:s0`.
   - Repacked CPIO initramfs `t711-fix.cpio.gz` (53,502,663 B, SHA `025cac1c987aaf57bfc3356062e3245de49c550b549a24cc73ff2754f040321d`, compresslevel=9).
   - Re-aligned DTB `t711-modified.dtb` (2,049 B, SHA `2d6d0c11ef22cace73e0f95b5eb8552727c57c614cc07d0fc16ece4613eeb77b`) byte-exactly to `linux,initrd-end = <0x01 0x133062c7>`.
2. **Smoke Flight (Step 3a PASS)**:
   - Flown on Mac QEMU 11.1.2: host 165.32 s, guest 163.35 s, exit 0, 11,051 lines, SHA `d1bfd4f681a7ca177a38d97feb46f956b16833de66cba438a1bffcb611d6d755`.
   - External `/dev/block/vda` mounted as `/system` Success at guest 9.64 s.
   - `fb0: simplefb registered!` at guest 0.32 s.
   - `primeCache` skipped (0 aborts), composer client failures: 0.
   - **Hard Gate 2 Completely Satisfied**: ZERO `avc: denied` lines for `sensorservice` (down from 4,597 in T710).
   - `dummy_sensorservice` registered on attempt 0:
     ```
     [dummy_sensorservice] Starting dummy sensorservice daemon (pid=454)...
     [dummy_sensorservice] Initial context: 'u:r:system_server:s0'
     [dummy_sensorservice] Successfully registered sensorservice with ServiceManager (attempts=0)!
     ```
3. **Canonical Control Flight-ab (Step 3b PASS)**:
   - Flown on canonical kernel `05c43917…`: host 20.03 s, guest 19.19 s, exit 0, 574 lines, SHA `29154ab0e1c0bc0ee270e4d387eb10b7918dfb0b8298f16d6625b0ae590440be`.
   - Re-proved `CONFIG_VIRTIO_BLK=y` requirement: partition polling timed out on vda and triggered `InitFatalReboot` at guest 19.10 s.
4. **Full 2400 s Flight-01 (Step 3b PASS Execution)**:
   - Flown on rebuilt kernel `53d9563e…`: host 2402.91 s, guest 2401.41 s, exit 0, 51,931 lines, SHA `6fc97ccc88c4991928934f38d94e0a5831044e79629e1a395327ba0650a5b4ad`.
   - Re-verified guest boot pipeline: Freeing kernel memory @3.46s (line 188), Init first stage @9.24s (line 520), VDA mount @9.64s (line 529).
   - Zero primeCache aborts, zero composer client failures, zero SQLite database crashes.
5. **Exact Mechanism of Failure Uncovered**:
   - `dummy_sensorservice` unblocked the T710 wall: `DeviceStateProviderImpl.setStateConditions` obtained `sensorservice` on `/dev/binder` and successfully transacted with it (10 transactions served across the flight).
   - `system_server` successfully booted through `startBootstrapServices()` and `startCoreServices()`, then entered `startOtherServices()`.
   - At boot phase 200 (`PHASE_WAIT_FOR_DEFAULT_DISPLAY`), `SystemServer` calls `com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)`.
   - `SensorService.onBootPhase()` calls `ConcurrentUtils.waitForFutureNoInterrupt(mSensorStartExecutor.submit(() -> mPtr = startSensorServiceNative()), "Start Sensor Service")`.
   - `startSensorServiceNative()` attempts to initialize the in-process native SensorService / `SensorDevice`, which depends on `android.hardware.sensors.ISensors` (`SensorsMultiHal`).
   - `SensorsMultiHal` failed during early boot:
     ```
     01-01 00:01:45.458 E/SensorsMultiHal( 2447): Failed to load subHal config file: /odm/etc/sensors/hals.conf
     ```
   - With no valid subHals loaded, `startSensorServiceNative()` hangs waiting on HAL initialization.
   - The main thread in `system_server` remained parked at `jdk.internal.misc.Unsafe.park` in `FutureTask.awaitDone` for 83-86 seconds, triggering `Watchdog` SIGKILL across 9 consecutive instances (PIDs 2592, 3218, 3890, 4545, 5199, 5879, 6533, 7186, 7847). PID 8505 was alive at cutoff.

---

## Exact First Failing Link & Exact Next Fix (Track T712)

1. **First Failing Link**:
   `system_server` main thread hangs in `com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` waiting for `startSensorServiceNative()`. In `system_server`, the in-process native `SensorService` initializes `SensorDevice`, which calls `SensorsMultiHal`. `SensorsMultiHal` cannot start because `/odm/etc/sensors/hals.conf` is missing (`Failed to load subHal config file: /odm/etc/sensors/hals.conf`), leaving the native initialization unresolved and causing `Watchdog` to kill `system_server` every ~83 seconds.

2. **Exact Next Fix (Track T712)**:
   - **Option B (Deploy dummy hals.conf)**: Repack `vendor.img` (on the Linux box via loopback mount) to supply dummy `/odm/etc/sensors/hals.conf` (or `/vendor/etc/sensors/hals.conf`) referencing a mock/stub sensors subHal, or provide a minimal mock subHal library so `SensorsMultiHal` initializes and `startSensorServiceNative()` returns.
   - **Option C (SensorService bypass during bootstrap)**: Patch framework/overlay or set system properties to bypass the native sensor service start in `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY)`, allowing `startOtherServices()` to proceed to ActivityManager / WindowManager display initialization and launcher startup.
