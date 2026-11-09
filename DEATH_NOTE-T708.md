# DEATH NOTE — T708

Lane: Track T708 — Disable SurfaceFlinger shader-cache priming (primary) + Composer client reconnect (secondary) + fly for the first queued buffer + first present dispatch (Mac agyx)  
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 11 GiB available; Step 1 verified all 11 staged inputs byte-for-byte without network fetch; Step 2 implemented Fix 1 [PRIMARY] setting `service.sf.prime_shader_cache=0` in CPIO initramfs boot properties and Fix 2 [SECONDARY] updating `ComposerImpl::createClient` on the Linux box to allow client reconnection, rebuilt Composer ELF `8d62a642…`, repacked `vendor.img` `8d5506ab…` on Linux loop device with clean `e2fsck -f -n` returncode 0, repacked CPIO initramfs `49d395d3…` [53,172,530 B], and realigned DTB `9a85c074…` byte-exactly to `0x1132b5932`; Step 3a smoke flight proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest 7.39s and proved the primeCache wall is 100% CURED ['output buffer not gpu writeable' dropped from 1 in T707 to 0, 0 aborts]; Step 3b Flight-01 reached full 2400 s window [guest 2407.143 s, host 2407.80 s, exit 0, 5,562 lines, log SHA `689f4f84…`]; Step 3b Flight-ab canonical control proved canonical kernel `05c43917…` times out polling vda and reboots at guest 17.15s; SurfaceFlinger ran stably throughout the full 2400 s flight with 0 aborts, 0 crashes, and 0 restarts; Zygote survived and successfully forked `system_server` 47 times across the flight; however, `system_server` terminated during early bootstrap ~2 seconds after `BatteryService` contacted `healthd`, causing Zygote's SIGCHLD handler to kill Zygote with signal 9 and trigger an init restart loop; `system_server` never reached display power-on or Home activity launch; queueBuffer 0, present 0)  
UTC close: 2026-11-09T00:25:00Z  
Worktree: /Users/Shared/wt-track708-primecache-disable  
Branch: t708 (from T707 commit 923dc56)  

## What ended

Track T708 was opened to disable SurfaceFlinger shader-cache priming (`service.sf.prime_shader_cache=0`) to eliminate the `output buffer not gpu writeable` abort, enable client reconnect in `ComposerImpl::createClient`, rebuild and repack on the Linux box, and fly the full 2400 s window on Mac QEMU 11.1.2.

1. **Fix 1 (PRIMARY) — SurfaceFlinger PrimeCache Disable (100% CURED)**:
   - Configured `service.sf.prime_shader_cache=0` in CPIO initramfs boot properties (`build.prop`, `system/build.prop`, `system/etc/ramdisk/build.prop`, `second_stage_resources/system/etc/ramdisk/build.prop`, `system/etc/prop.default`, and `t636-diag.sh`).
   - Cleanly bypassed `mRenderEngine->primeCache()` in `SurfaceFlinger::init()+3764`.
   - `output buffer not gpu writeable` dropped from 1 in T707 to **0** in T708 across all 5,562 lines.
   - `Abort message:` dropped from 30 in T707 to **0** in T708.
   - SurfaceFlinger ran with **zero aborts, zero crashes, zero restarts** across the entire 2407 s flight.
2. **Fix 2 (SECONDARY) — Composer 2.1 Client Reconnect Logic**:
   - Updated `ComposerImpl::createClient` in `/home/muse/t681-link/t681-service.cpp` on the Linux box: added code to check if `mClient != nullptr`, log `"reconnect after client death"`, reset the old client reference, and successfully register the new client instead of returning `Error::NO_RESOURCES`.
   - Rebuilt Composer ELF via NDK r29 / clang toolchain (`build-service.sh`): 19,942,712 bytes, SHA-256 `8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401`.
   - Repacked `vendor.img` on Linux via debugfs: 134,217,728 bytes, SHA-256 `8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092`, clean `e2fsck -f -n` verified returncode 0.
   - Because SurfaceFlinger never crashed, the secondary reconnect path was not exercised during the flight; `failed to create composer client` dropped from 29 in T707 to **0** in T708.
3. **Initramfs Repack & DTB Re-alignment**:
   - Replaced `ramdisk-images/vendor.img` in fix initramfs (`compresslevel=9`, `mtime=0.0`): 53,172,530 bytes, SHA-256 `49d395d3e195cdbe2686c61a3b53b32a48ed16e81b592a67e71f1b8236fe1ad9`.
   - Re-aligned DTB `linux,initrd-end` byte-exactly to `0x110000000 + 53172530 = 0x1132b5932`: 2,049 bytes, SHA-256 `9a85c074df4a6c0a84fec65f793cb1b63a73c94ba135d247ea5bf936539fb85c`.
4. **Step 3a — 120 s Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 145.27 s, guest 141.162 s, exit 0, 3,657 lines, log SHA `e3ab4d16…`.
   - Proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest line 523 (`__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`).
   - Proved primeCache wall eliminated: 0 `output buffer not gpu writeable`, 0 aborts.
5. **Step 3b — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 20.03 s, guest 17.145 s, exit 0, 560 lines, log SHA `ac667385…`.
   - Proved canonical missing `vda` failure: line 550 `partition(s) not found after polling timeout: vda`, triggering `InitFatalReboot` at line 552 and clean reboot at line 560. Re-proves `CONFIG_VIRTIO_BLK=y` is required for external image mount.
6. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2407.80 s, guest 2407.143 s, exit 0, 5,562 lines, log SHA `689f4f84…`.
   - External `/dev/block/vda` mounted cleanly as `/system` at guest 6.81s (line 524).
   - Zygote successfully forked `system_server` 47 times across the flight (first instance PID 2494, guest 181.59s).

## Why the verdict is FAIL

Under the proving criteria of Track T708:
1. **`sys.boot_completed=1` Not Observed (0)**:
   - `sys.boot_completed` remained unset throughout the 2400 s window.
2. **`system_server` Terminated Early (Restart Loop)**:
   - While Zygote successfully forked `system_server` 47 times across the flight, each `system_server` instance terminated ~2 seconds after initializing `BatteryService` with `healthd` (lines 3658–3662).
   - When `system_server` terminated, Zygote's `SIGCHLD` handler sent SIGKILL (signal 9) to Zygote, triggering init to restart Zygote in a loop.
3. **QueueBuffer & Present Dispatch Not Reached (0)**:
   - `queueBuffer`: 0
   - `presentDisplay`: 0, `validateDisplay`: 0, `presentOrValidate`: 0, `executeCommands`: 0.
   - `com.android.launcher3` was not launched because `system_server` never completed startup.

## Exact First Failing Link & Exact Next Fix (Track T709)

1. **First Failing Link**:
   `system_server` repeatedly terminates during early bootstrap ~2 seconds after `BatteryService` contacts `healthd` (`healthd: battery none chg=`), causing Zygote to kill itself with SIGKILL and triggering an init restart loop (47 instances across 2400s). `system_server` never reached display power-on or Home activity launch.
2. **Exact Next Fix (Track T709)**:
   - Enable `logcat_streamer` in `early-ramdisk` (uncommenting `/system/bin/logcat -b all -v time *:V >/dev/console 2>&1 &` or piping `logcat -s AndroidRuntime:E SystemServer:E`) so `system_server`'s uncaught Java exception / crash stack trace is directly visible on UART console.
   - Cure the root cause of `system_server` startup termination (e.g., missing SELinux policy or service mock following `BatteryService`/`UsageStatsService`/`WebViewUpdateService` in `SystemServer.java`).
