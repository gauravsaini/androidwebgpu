# DEATH NOTE — T709

Lane: Track T709 — Enable logcat streamer in early-ramdisk + cure system_server early termination + fly for the first queued buffer + first present dispatch (Mac agyx)  
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 10.92 GiB available; Step 1 verified all 11 staged inputs byte-for-byte without network fetch; Step 2 [PRIMARY FIX] cross-compiled statically-linked aarch64 `logcat_streamer` binary [`c26af47a…`, 663,704 B] into initramfs root and launched before `exec /init`, successfully delaying for Android first stage init `switch_root`, joining PID 1 mount namespace via `setns(CLONE_NEWNS)` + `chroot("/proc/1/root")`, and streaming `/system/bin/logcat` directly to `/dev/console`; Step 2 [SECONDARY FIX] captured live system_server [PID 2561] exception: `SQLiteCantOpenDatabaseException: Directory /data/user_de/0/com.android.providers.settings/databases doesn't exist` which previously caused system_server suicide via `Process.killProcess(myPid())` in T708, and cured it by pre-creating `/data/user_de/0/com.android.providers.settings/databases` and core package data dirs on ext4 `/data` in `early-ramdisk` [mode 0777, uid 1000:1000]; repacked fix CPIO initramfs `t709-systemserver-fb0-fix.cpio.gz` [`f164d33b…`, 53,465,900 B] and realigned DTB `t709-modified.dtb` [`718691ec…`, 2,049 B] byte-exactly to `0x1132fd32c`; Step 3a smoke flight proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest 8.92s, logcat streamer confirmed streaming, and SettingsProvider initialized database and generated ANDROID_ID `[e890d3c1e2b292ad]` with 0 `SQLiteCantOpenDatabaseException`; Step 3b Flight-ab canonical control on `05c43917…` [host 25.03s, guest 19.79s, exit 0, 571 lines, log `7da313bc…`] re-proved missing `vda` polling timeout and reboot; Step 3b Flight-01 ran full 2400 s window [host 2417.94s, guest 2414.07s, exit 0, 52,768 lines, log SHA `97194592…`]; logcat streamer exposed the subsequent wall: `Watchdog` killing system_server after 83s because the main thread was blocked in `SystemSensorManager.nativeCreate` waiting for `sensorservice` on `/dev/binder` in `DeviceStateProviderImpl.setStateConditions` [sensorservice never published because `SensorsMultiHal` failed to load `/odm/etc/sensors/hals.conf`]; queueBuffer 0, presentDisplay 0)  
UTC close: 2026-11-09T03:00:00Z  
Worktree: /Users/Shared/wt-track709-logcat-systemserver-cure  
Branch: t709  

## What ended

Track T709 was opened to enable the logcat streamer in `early-ramdisk` to make system_server Java stack traces visible on the UART console, diagnose and cure the early termination root cause, rebuild and repack the initramfs/DTB, and fly the full 2400 s window on Mac QEMU 11.1.2.

1. **Fix 1 (PRIMARY) — Logcat Streamer in early-ramdisk (100% OPERATIONAL)**:
   - Statically cross-compiled an aarch64 native ELF `logcat_streamer` binary (`c26af47a…`, 663,704 B) on Linux box and injected it into CPIO initramfs root.
   - Handled Android first stage init `switch_root`: daemon delays 4s, mounts a private `/proc`, joins PID 1's mount namespace via `setns(CLONE_NEWNS)`, chroots into Android rootfs, and streams `/system/bin/logcat` directly to `/dev/console`.
   - Live userspace logs and Java crash traces now stream cleanly to host UART console!
2. **Fix 2 (SECONDARY) — SettingsProvider Database Directory Pre-creation (100% CURED)**:
   - Diagnosed exact T708 termination root cause from visible logcat stream:
     `SQLiteCantOpenDatabaseException: Directory /data/user_de/0/com.android.providers.settings/databases doesn't exist`
   - Cured in `early-ramdisk`: pre-created `/data/user_de/0/com.android.providers.settings/databases` and core package data dirs on ext4 `/data` (0777, 1000:1000).
   - `SettingsProvider` initialized without error and generated `ANDROID_ID` `[e890d3c1e2b292ad]`.
3. **Initramfs Repack & DTB Re-alignment (PASS)**:
   - Rebuilt fix CPIO: 53,465,900 bytes, SHA-256 `f164d33b8b48a8541de4c6e163be654647fca39a1a8a70b421b05a7114f562db`.
   - Re-aligned DTB `linux,initrd-end` to `0x1132fd32c`: 2,049 bytes, SHA-256 `718691ecb661046785bae0b26a2d5cbd2ae90518ef591a810f7484ac03b9263e`.
4. **Step 3a — Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 175.3 s, guest 174.8 s, exit 0. External virtio-blk `/dev/block/vda` mounted as `/system` cleanly. Logcat streamer confirmed active.
5. **Step 3b — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 25.03 s, guest 19.79 s, exit 0, 571 lines, log SHA `7da313bc…`.
   - Proved missing `vda` polling timeout and reboot under canonical kernel lacking `CONFIG_VIRTIO_BLK=y`.
6. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2417.94 s, guest 2414.07 s, exit 0, 52,768 lines, log SHA `97194592…`.
   - `system_server` advanced past bootstrap into `startOtherServices()`.

## Why the verdict is FAIL

Under the proving criteria of Track T709:
1. **`sys.boot_completed=1` Not Observed (0)**:
   `sys.boot_completed` remained unset throughout the 2400 s window.
2. **Watchdog Killing system_server on sensorservice Blockage**:
   In `startOtherServices()`, `DeviceStateManagerService` called `DeviceStateProviderImpl.setStateConditions()` which invoked `SystemSensorManager.nativeCreate()`, blocking the main thread on `waitForService("sensorservice")`.
   `sensorservice` was never registered because `SensorsMultiHal` failed with:
   `E/SensorsMultiHal( 2438): Failed to load subHal config file: /odm/etc/sensors/hals.conf`
   After 83 seconds of main thread blockage, `Watchdog` killed system_server with SIGKILL (signal 9):
   `W/Watchdog: *** WATCHDOG KILLING SYSTEM PROCESS: Blocked in handler on on main thread (main) for 83s`
   Init restarted Zygote in a loop (9 instances across the 2400s flight).
3. **QueueBuffer & Present Dispatch Not Reached (0)**:
   `queueBuffer`: 0, `presentDisplay`: 0, `validateDisplay`: 0.

## Exact First Failing Link & Exact Next Fix (Track T710)

1. **First Failing Link**:
   `SensorsMultiHal` fails to load `/odm/etc/sensors/hals.conf` and does not publish `sensorservice` on `/dev/binder`. `DeviceStateManagerService` initializes `SensorManager` on the main thread in `SystemSensorManager.nativeCreate()`, hanging indefinitely in `waitForService("sensorservice")` until `Watchdog` kills `system_server` after 83 seconds.
2. **Exact Next Fix (Track T710)**:
   - Mock or stub `sensorservice` (e.g., register a dummy `sensorservice` binder service in `early-ramdisk` or init, or deploy a dummy `/odm/etc/sensors/hals.conf` config file in vendor/odm) so that `waitForService("sensorservice")` returns immediately.
   - Alternatively, disable device state sensor conditions via system property or overlay to bypass the `SensorManager` dependency during bootstrap.
