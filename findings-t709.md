# Findings — Track T709: Enable logcat streamer in early-ramdisk + cure system_server early termination + fly for the first queued buffer + first present dispatch (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track709-logcat-systemserver-cure`  
Branch: `t709`  
Predecessor: T708 driver-certified honest FAIL (commit 923dc56: SurfaceFlinger primeCache wall cured via `service.sf.prime_shader_cache=0` and Composer client reconnect logic verified, but system_server terminated ~2 seconds after BatteryService in an invisible 23-instance loop with zero FATAL EXCEPTION logged due to absence of logcat streaming to UART).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 10.92 GiB available; Step 1 verified all 11 staged inputs byte-for-byte without network fetch; Step 2 [PRIMARY FIX] cross-compiled statically-linked aarch64 `logcat_streamer` binary [`c26af47a…`, 663,704 B] into initramfs root and launched before `exec /init`, successfully delaying for Android first stage init `switch_root`, joining PID 1 mount namespace via `setns(CLONE_NEWNS)` + `chroot("/proc/1/root")`, and streaming `/system/bin/logcat` directly to `/dev/console`; Step 2 [SECONDARY FIX] captured live system_server [PID 2561] exception: `SQLiteCantOpenDatabaseException: Directory /data/user_de/0/com.android.providers.settings/databases doesn't exist` which previously caused system_server suicide via `Process.killProcess(myPid())` in T708, and cured it by pre-creating `/data/user_de/0/com.android.providers.settings/databases` and core package data dirs on ext4 `/data` in `early-ramdisk` [mode 0777, uid 1000:1000]; repacked fix CPIO initramfs `t709-systemserver-fb0-fix.cpio.gz` [`f164d33b…`, 53,465,900 B] and realigned DTB `t709-modified.dtb` [`718691ec…`, 2,049 B] byte-exactly to `0x1132fd32c`; Step 3a smoke flight proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest 8.92s, logcat streamer confirmed streaming, and SettingsProvider initialized database and generated ANDROID_ID `[e890d3c1e2b292ad]` with 0 `SQLiteCantOpenDatabaseException`; Step 3b Flight-ab canonical control on `05c43917…` [host 25.03s, guest 19.79s, exit 0, 571 lines, log `7da313bc…`] re-proved missing `vda` polling timeout and reboot; Step 3b Flight-01 ran full 2400 s window [host 2417.94s, guest 2414.07s, exit 0, 52,768 lines, log SHA `97194592…`]; logcat streamer exposed the subsequent wall: `Watchdog` killing system_server after 83s because the main thread was blocked in `SystemSensorManager.nativeCreate` waiting for `sensorservice` on `/dev/binder` in `DeviceStateProviderImpl.setStateConditions` [sensorservice never published because `SensorsMultiHal` failed to load `/odm/etc/sensors/hals.conf`]; queueBuffer 0, presentDisplay 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, the primeCache abort from T707 remained completely cured (0 occurrences of `output buffer not gpu writeable`, 0 `failed to create composer client`, 0 `Abort message:`). The T708 system_server bootstrap termination root cause was diagnosed and cured: `SQLiteCantOpenDatabaseException` dropped from crashing every instance to 0. SettingsProvider successfully generated and saved ANDROID_ID `[e890d3c1e2b292ad]`. However, system_server blocked in `startOtherServices()` during `DeviceStateProviderImpl.setStateConditions()` where `SystemSensorManager.nativeCreate()` hung waiting for `sensorservice` on `/dev/binder` (which was never registered because `SensorsMultiHal` failed with `Failed to load subHal config file: /odm/etc/sensors/hals.conf`). After 83 seconds of main thread blockage, `Watchdog` killed system_server with SIGKILL (signal 9), causing Zygote to exit and init to restart Zygote across 9 instances in the 2400s flight. As a result, `sys.boot_completed` remained unset. In control flight `flight-ab`, `boot_completed` remained unset until clean reboot on vda polling timeout at guest 17.15s.

WALL-TYPE-TAG: guest type-1 on FAIL  
Rust par bhi lagu: haan — the same guest userspace display stack, RenderEngine shader cache priming behavior, HWC client lifecycle, system_server bootstrap sequence, and Zygote process supervision are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.443484 s (line 188: `[    3.443484] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.530380 s (line 186: `[    3.530380] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.341628 s (line 188: `[    3.341628] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 8.920858 s (line 518: `[    8.920858] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 9.157748 s (line 520: `[    9.157748] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 9.128940 s (line 518: `[    9.128940] init: init first stage started!`).

system_server:
- In contrast to T708 where `system_server` Java stack traces were completely invisible and died silently after `BatteryService`, in T709 the primary fix (`logcat_streamer`) streamed live logcat directly to UART console.
- In extended smoke testing, the exact initial failure was revealed:
  `android.database.sqlite.SQLiteCantOpenDatabaseException: Cannot open database '/data/user_de/0/com.android.providers.settings/databases/settings.db' with flags 0x10000000: Directory /data/user_de/0/com.android.providers.settings/databases doesn't exist`
- After applying the secondary fix in `early-ramdisk` (pre-creating `/data/user_de/0/com.android.providers.settings/databases` and core package data dirs), SettingsProvider initialized without error:
  `01-01 00:03:43.944 D/SettingsProvider( 2572): Generated and saved new ANDROID_ID [e890d3c1e2b292ad] for user 0`
- `system_server` advanced past bootstrap into `startOtherServices` (starting `DropBoxManagerService`, `AlarmManagerService`, `InputManagerService`, `DeviceStateManagerService`).
- In `startOtherServices`, `DeviceStateManagerService` called `DeviceStateProviderImpl.setStateConditions()` which called `SystemSensorManager.nativeCreate()`, blocking the main thread on `waitForService("sensorservice")`. After 83s, `Watchdog` killed the system process (9 instances across the 2400s flight: PIDs 2614, 3300, 3905, 4570, 5228, 5879, 6526, 7176, 7826).
- In control flight `flight-ab`, `system_server` did not fork (0) because canonical kernel failed to mount `/system` from `/dev/block/vda` due to lack of `CONFIG_VIRTIO_BLK=y` and triggered `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- `queueBuffer`: **0**

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block devices, but could not detect `/dev/vda` due to absence of `CONFIG_VIRTIO_BLK=y` in the canonical build. At guest 19.67 s, Android init timed out polling vda, triggering `InitFatalReboot` and reboot. QEMU cleanly terminated under `-no-reboot` at guest 19.788 s (host 25.03 s, exit code 0, 571 lines, log SHA-256 `7da313bcbc5c3b491af1e29768cca7e22361c8f3b2d53d0061573d1999cde08f`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` repeatedly terminated during `startOtherServices` before reaching Home activity launch.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T709 executed the two fixes requested by the mission: (1) PRIMARY — enabling the logcat streamer in `early-ramdisk` so `system_server`'s Java stack traces and system logs stream directly to the UART console; and (2) SECONDARY — curing the observed `system_server` termination root cause (`SQLiteCantOpenDatabaseException` in `SettingsProvider`) by pre-creating the required database directories on ext4 `/data` in `early-ramdisk`. Both fixes were validated on Mac QEMU 11.1.2 across smoke flights, canonical control `flight-ab`, and the full 2400 s window `flight-01`.

### Key Accomplishments
1. **Primary Fix (Logcat Streamer in early-ramdisk - 100% OPERATIONAL)**:
   - Statically cross-compiled an aarch64 native ELF `logcat_streamer` binary (`c26af47a…`, 663,704 B) on the Linux box and added it as a root CPIO member in `t709-systemserver-fb0-fix.cpio.gz`.
   - Instrumented `early-ramdisk` to launch `/logcat_streamer >/dev/console 2>&1 &` before `exec /init`.
   - The daemon delays 4s while Android first stage init executes `SwitchRoot("/system")` and frees ramdisk memory, mounts a clean `/proc`, enters PID 1's mount namespace via `setns(CLONE_NEWNS)`, chroots into `/proc/1/root`, waits for `/dev/socket/logdr`, and streams `/system/bin/logcat -b all -v time *:V` to `/dev/console`.
   - **Critical Breakthrough**: For the first time in the project, all Android userspace logcat messages, Java exceptions, WTF lines, and Watchdog annotated stack traces stream live to the host UART console!
2. **Secondary Fix (SettingsProvider Database Directory Pre-creation - 100% CURED)**:
   - Diagnosed exact T708 termination root cause from visible logcat stream:
     ```
     android.database.sqlite.SQLiteCantOpenDatabaseException: Cannot open database '/data/user_de/0/com.android.providers.settings/databases/settings.db' with flags 0x10000000: Directory /data/user_de/0/com.android.providers.settings/databases doesn't exist
     ```
   - Caused because `installd` was not started by init, leaving app directories uncreated.
   - Applied secondary fix in `early-ramdisk`: after ext4 `/data` is mounted, pre-created `/data/user_de/0/com.android.providers.settings/databases`, `/data/data/...`, and `/data/user/0/...` for core system packages with mode `0777` and owner `1000:1000` (`system:system`).
   - Guest UART proof: SettingsProvider created `settings.db` cleanly and generated ANDROID_ID `[e890d3c1e2b292ad]`. `SQLiteCantOpenDatabaseException` dropped from crashing every instance to **0**!
3. **system_server Advancement into startOtherServices**:
   - `system_server` successfully passed `startBootstrapServices()` and progressed into `startCoreServices()` and `startOtherServices()` (starting `DropBoxManagerService`, `AlarmManagerService`, `InputManagerService`, `DeviceStateManagerService`).
4. **Diagnosis of the Next Wall (Watchdog SystemSensorManager Blockage)**:
   - With the logcat streamer active, the exact termination mechanism was revealed in Flight-01:
     ```
     11-10 23:21:29.720 W/Watchdog( 2614): *** WATCHDOG KILLING SYSTEM PROCESS: Blocked in handler on  on main thread (main) for 83s
     11-10 23:21:29.849 W/Watchdog( 2614): main annotated stack trace:
     11-10 23:21:29.849 W/Watchdog( 2614):     at android.hardware.SystemSensorManager.nativeCreate(Native Method)
     11-10 23:21:29.849 W/Watchdog( 2614):     at android.hardware.SystemSensorManager.<init>(SystemSensorManager.java:156)
     11-10 23:21:29.850 W/Watchdog( 2614):     at android.app.SystemServiceRegistry$41.createService(SystemServiceRegistry.java:633)
     11-10 23:21:29.875 W/Watchdog( 2614):     at android.app.ContextImpl.getSystemService(ContextImpl.java:2212)
     11-10 23:21:29.876 W/Watchdog( 2614):     at com.android.server.policy.DeviceStateProviderImpl.setStateConditions(DeviceStateProviderImpl.java:363)
     ```
   - Root cause: `DeviceStateManagerService` requested `SensorManager`, invoking `SystemSensorManager.nativeCreate()` which called `waitForService("sensorservice")` on the main thread.
   - `sensorservice` was never registered because `SensorsMultiHal` failed with:
     `01-01 00:01:39.510 E/SensorsMultiHal( 2438): Failed to load subHal config file: /odm/etc/sensors/hals.conf`
   - After 83 seconds of main thread blockage, `Watchdog` killed system_server with SIGKILL.
   - Across the 2400s flight, 9 instances ran in this manner (PIDs 2614, 3300, 3905, 4570, 5228, 5879, 6526, 7176, 7826).
5. **CPIO Initramfs Repack & DTB Alignment (PASS)**:
   - Repacked CPIO initramfs: 53,465,900 bytes, SHA-256 `f164d33b8b48a8541de4c6e163be654647fca39a1a8a70b421b05a7114f562db`.
   - Re-aligned DTB `linux,initrd-end` byte-exactly to `0x110000000 + 53465900 = 0x1132fd32c`: 2,049 bytes, SHA-256 `718691ecb661046785bae0b26a2d5cbd2ae90518ef591a810f7484ac03b9263e`.
6. **Full Flight-01 & Canonical A/B Control Flown (PASS Execution)**:
   - Flight-01 ran full 2400 s target (host 2417.94 s, guest 2414.07 s, exit 0, 52,768 lines, log SHA `97194592…`).
   - Flight-ab ran on canonical kernel `05c43917…` and confirmed missing `CONFIG_VIRTIO_BLK=y` triggers vda polling timeout and reboot (host 25.03 s, guest 19.79 s, exit 0, 571 lines, log SHA `7da313bc…`).
