# DEATH NOTE: Track T714 — Direct binary patch of libandroid_servers.so & Forensic Autopsy

## Executive Summary

- **Track**: T714
- **Date**: 2026-11-13
- **Verdict**: FAIL
- **Core Objective**: Direct binary patch of `libandroid_servers.so` in `system.img` to bypass in-process native sensor initialization (`startSensorServiceNative()`), followed by a full 2400 s guest flight on the T713 stack to achieve the first `queueBuffer` and first present dispatch.
- **Immediate Cause of Death**: Despite successful byte-verification and binary patching of `libandroid_servers.so` at `0xaaee8` (`tbz w20, #0, aafec` -> `b 0xaafec`), `SensorService.onBootPhase(PHASE_WAIT_FOR_DEFAULT_DISPLAY = 200)` blocked indefinitely on `FutureTask.get()` in `ConcurrentUtils.waitForFutureNoInterrupt(mSensorServiceStart, START_NATIVE_SENSOR_SERVICE)`. Android Watchdog killed `system_server` exactly 10 consecutive times after 81–83 s each. `queueBuffer` remained 0, `presentDisplay` remained 0, `boot_completed` remained 0.
- **Root Cause & Forensic Wall**: `SensorService.java` offloads native startup via `mSensorServiceStart = SystemServerInitThreadPool.submit(...)`. However, `SystemServerInitThreadPool` is instantiated with only **1 worker thread** (`Creating instance with 1 threads`, L10918). That single worker thread was occupied by `PackageManager`'s `AppDataHelper.fixAppsDataOnBoot`, which entered a retry loop waiting for `installd` (`W/Installer: installd not found; trying again`, L12818, repeated 2,750 times across the flight). Because `installd` was never started by init (it belongs to `class main`, which was not triggered by init in this boot sequence), the single worker thread remained permanently blocked. The `mSensorServiceStart` task was queued behind it and never executed. When `startOtherServices()` invoked `onBootPhase(200)`, the main thread called `FutureTask.get()` on `mSensorServiceStart` and hung until Watchdog SIGKILL.
- **Rust par bhi lagu**: **Haan**. Yeh wall emulator/hypervisor dependent nahi hai; yeh Android OS framework ki internal concurrency architecture aur init service dependency ordering ka deadlock hai. Rust emulator par bhi `SystemServerInitThreadPool` single-threaded hi rahega aur `installd` missing hone par same starvation deadlock hoga.

---

## 1. Forensic Autopsy & Evidence

### Step 2 Binary Patch Execution & Verification
1. **Extraction**: Unpatched `libandroid_servers.so` (850,584 B, SHA-256 `2ce96adb15e4a8934d52cfaa8825c1b500b3eefa6a989e071a33963f1580539c`) was extracted from base `t706-system.img` on the Linux box.
2. **Byte Verification & Driver Variance V2 Resolution**:
   - String offset `0x2ef7c`: `system_init.startsensorservice\0` (31 bytes).
   - Driver Variance V2 resolved: instruction at `0xaaea0` is `10c206e9` (`adr x9, 2ef7c`), loading the property string PC-relatively directly from `0x2ef7c`. The T713 agent report had misprinted `adrp x0, 2ef000` and had a double-7 typo `0x2ef77c`.
   - Patch site offset `0xaaee8`: `34 08 00 36` (`0x36000834` = `tbz w20, #0, aafec`).
3. **Patching**: 4 bytes at offset `0xaaee8` replaced with `41 00 00 14` (`0x14000041` = `b 0xaafec`), targeting function epilogue. Patched `.so` SHA-256: `ea4ce478f0e72d30018bd6bdac3cabbb506fdc8d616d3b8b7e6af58036482d57`.
4. **Extent-Safe Repack**: Applied into `t714-system.img` allocating fresh inode 6980, mode 0100644, security.selinux `u:object_r:system_lib_file:s0`. Checked `e2fsck -f -n` (clean exit 0). Verified block 145414 intact; `services.jar` byte-identical (`d4108052…`). Patched `system.img` SHA-256: `ea5af412552668682f185a59dd4af39d600325c4ad1aaa62e0413602fae143ef` (2,059,816,960 B).
5. **Preserved Artifacts**: Preserved `guest-inputs/libandroid_servers.so` and `guest-inputs/libandroid_servers_patched.so` in worktree for driver verification.

### Step 3a Smoke Flight (Attempt 3)
- Host: 275.90s, Guest: 274.84s, exit code 0, 13,234 lines, SHA-256 `7b83a44a966023f97b2df6a9cd9939fc9caa81b0f446dc93a763a4a1a958d7d0`.
- `/dev/block/vda` mounted as `/system` Success at guest 8.74s.
- `simplefb` registered at guest 0.30s.
- `SensorsMultiHal` loaded `hals.conf` and published `ISensors/default` with 0 SIGABRT.
- `dummy_sensorservice` registered with 0 avc denials.
- `SystemServer` PID 2589 entered system server and reached boot phase dispatch without early crash.

### Step 3b Flight-01 (Full 2400 s Guest Target)
- Host: 2432.92s, Guest: 2431.70s, exit code 0, 56,604 lines, SHA-256 `129942be454964593fad4eba72d33f504a2f35defabd36b8bc48cc23c979ef78`.
- `/dev/block/vda` mounted `/system` Success at guest 8.92s (L531).
- `SensorsMultiHal` and `ISensors/default` registered in VINTF manifest.
- `queueBuffer` count = 0.
- `presentDisplay` count = 0.
- `boot_completed` count = 0.
- `FATAL EXCEPTION` count = 0.
- `primeCache` GPU errors = 0.

### Step 3b Flight-ab (Canonical Control)
- Canonical kernel `05c43917…` on QEMU 11.1.2: Host 20.02s, Guest 19.48s, exit code 0, 575 lines, SHA-256 `6a2a008cd4a4044f6ef61285055a65812771b186af274e9f813d0c742ff487ff`.
- Polling for `vda` timed out after 10016ms (L545), followed by `InitFatalReboot: signal 6` (L548), proving `CONFIG_VIRTIO_BLK=y` in the rebuilt kernel is the sole enabler of the external mount.

---

## 2. Watchdog Kill Registry (Flight-01)

Exactly 10 Watchdog kills occurred across 11 `system_server` forks (PIDs 2589 to 8983). Every kill occurred after 81–83 s of blocking on `SensorService.onBootPhase(SensorService.java:89)`:

| Kill # | PID | Kill Line | Fork Line | Elapsed Time | Watchdog Stack Citation |
|---|---|---|---|---|---|
| 1 | 2589 | 14667 | 10898 | ~209s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 2 | 3245 | 19588 | 16219 | ~430s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 3 | 3897 | 24523 | 21179 | ~664s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 4 | 4536 | 29514 | 26103 | ~886s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 5 | 5175 | 34429 | 31096 | ~1107s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 6 | 5809 | 39930 | 36566 | ~1325s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 7 | 6445 | 45345 | 42013 | ~1540s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 8 | 7083 | 50891 | 47519 | ~1761s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 9 | 7719 | 56346 | 52988 | ~1972s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| 10 | 8357 | 61858 | 58520 | ~2194s | `SensorService.onBootPhase(SensorService.java:89)` on `FutureTask.get()` |
| Active | 8983 | N/A | 64018 | ~2431s | Active at flight cutoff |

### Annotated Stack Trace (Kill #10, UART L61861-L61875)
```text
11-10 23:54:33.708 W/Watchdog( 8357): main annotated stack trace:
11-10 23:54:33.709 W/Watchdog( 8357):     at jdk.internal.misc.Unsafe.park(Native Method)
11-10 23:54:33.709 W/Watchdog( 8357):     at java.util.concurrent.locks.LockSupport.park(LockSupport.java:211)
11-10 23:54:33.709 W/Watchdog( 8357):     at java.util.concurrent.FutureTask.awaitDone(FutureTask.java:447)
11-10 23:54:33.733 W/Watchdog( 8357):     at java.util.concurrent.FutureTask.get(FutureTask.java:190)
11-10 23:54:33.733 W/Watchdog( 8357):     at com.android.internal.util.ConcurrentUtils.waitForFutureNoInterrupt(ConcurrentUtils.java:86)
11-10 23:54:33.733 W/Watchdog( 8357):     at com.android.server.sensors.SensorService.onBootPhase(SensorService.java:89)
11-10 23:54:33.733 W/Watchdog( 8357):     at com.android.server.SystemServiceManager.startBootPhase(SystemServiceManager.java:291)
11-10 23:54:33.733 W/Watchdog( 8357):     at com.android.server.SystemServer.startOtherServices(SystemServer.java:1614)
11-10 23:54:33.734 W/Watchdog( 8357):     at com.android.server.SystemServer.run(SystemServer.java:945)
11-10 23:54:33.734 W/Watchdog( 8357):     at com.android.server.SystemServer.main(SystemServer.java:667)
11-10 23:54:33.734 W/Watchdog( 8357):     at java.lang.reflect.Method.invoke(Native Method)
11-10 23:54:33.734 W/Watchdog( 8357):     at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:552)
11-10 23:54:33.735 W/Watchdog( 8357):     at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:949)
11-10 23:54:33.735 W/Watchdog( 8357): *** GOODBYE!
```

---

## 3. Detailed Forensic Wall Analysis

### Why the Direct Binary Patch Was Insufficient Alone
In T713, the hypothesis was that in-process `startSensorServiceNative()` hung inside the native C++ daemon. Thus, jumping to the constructor epilogue (`0xaafec`) would allow `startSensorServiceNative()` to return immediately.
We proved:
1. The binary patch at `0xaaee8` (`b 0xaafec`) did in fact take effect and forces `NativeSensorService` to return immediately.
2. Disassembly of `startSensorServiceNative` at `0xac7c8` confirmed it allocates `NativeSensorService`, invokes the patched constructor, and returns `service` pointer to Java.
3. However, `SensorService.java` offloads native invocation via `SystemServerInitThreadPool.submit(...)`.
4. In this build, `SystemServerInitThreadPool` has a pool size of **1** (`I/SystemServerInitThreadPool: Creating instance with 1 threads`).
5. Earlier in bootstrap, `PackageManagerService` submits `AppDataHelper.fixAppsDataOnBoot` to `SystemServerInitThreadPool`.
6. `fixAppsDataOnBoot` repeatedly calls `Installer.fixupAppData(...)`, which loops waiting for `/dev/socket/installd` (`W/Installer: installd not found; trying again`).
7. Because `installd` is absent, the single thread of `SystemServerInitThreadPool` remains 100% blocked on `installd`.
8. The subsequent `mSensorServiceStart` task remains stuck in `SystemServerInitThreadPool`'s work queue, never reaching execution.
9. When the main thread reaches `SensorService.onBootPhase(200)`, it invokes `ConcurrentUtils.waitForFutureNoInterrupt(mSensorServiceStart)`. Since `mSensorServiceStart` has not completed, `FutureTask.get()` blocks indefinitely until Watchdog fires.

### Scaffold Statement
The Vector 1 binary patch is recorded as a **scaffold**. It successfully isolates the native sensor code path from the property resolution mechanism, but does not provide a complete fix because the underlying framework execution pipeline deadlocks on `SystemServerInitThreadPool` starvation and missing `installd`. The real fix (ensuring `installd` starts or decoupling `SensorService` from the blocked thread pool) remains owed.

---

## 4. Recommendations for Next Lane

1. **Option A (Trigger `installd` Service)**:
   Add an explicit `start installd` trigger in early init scripts (or in `early-ramdisk` after switch_root), or satisfy the conditions for `class_start main`, so that `installd` starts, allowing `PackageManager`'s `fixAppsDataOnBoot` to finish promptly and unblock `SystemServerInitThreadPool`.
2. **Option B (Framework Patch of `SystemServerInitThreadPool`)**:
   Increase thread pool size in `SystemServerInitThreadPool.java` (in `services.jar`) from 1 to 4+, or execute `mSensorServiceStart` on a dedicated standalone thread rather than the shared pool.
3. **Option C (Decouple `SensorService.onBootPhase`)**:
   Patch `SensorService.java` in `services.jar` at `onBootPhase` to skip `ConcurrentUtils.waitForFutureNoInterrupt` when `system_init.startsensorservice` is false or a bypass flag is set.
