# Findings — Track T708: Disable SurfaceFlinger shader-cache priming (primary) + Composer client reconnect (secondary) + fly for the first queued buffer + first present dispatch (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track708-primecache-disable`  
Branch: `t708`, from `923dc56` (the T707 commit)  
Predecessor: T707 driver-certified honest FAIL (commit 923dc56: HWC hotplug wall cured by Composer 2.1 onHotplug dispatch, but SurfaceFlinger aborted in `SkiaRenderEngine::primeCache()` with 'output buffer not gpu writeable' and subsequent restarts failed with 'failed to create composer client').

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 11 GiB available; Step 1 verified all 11 staged inputs byte-for-byte without network fetch; Step 2 implemented Fix 1 [PRIMARY] setting `service.sf.prime_shader_cache=0` in CPIO initramfs boot properties and Fix 2 [SECONDARY] updating `ComposerImpl::createClient` on the Linux box to allow client reconnection, rebuilt Composer ELF `8d62a642…`, repacked `vendor.img` `8d5506ab…` on Linux loop device with clean `e2fsck -f -n` returncode 0, repacked CPIO initramfs `49d395d3…` [53,172,530 B], and realigned DTB `9a85c074…` byte-exactly to `0x1132b5932`; Step 3a smoke flight proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest 7.39s and proved the primeCache wall is 100% CURED ['output buffer not gpu writeable' dropped from 1 in T707 to 0, 0 aborts]; Step 3b Flight-01 reached full 2400 s window [guest 2407.143 s, host 2407.80 s, exit 0, 5,562 lines, log SHA `689f4f84…`]; Step 3b Flight-ab canonical control proved canonical kernel `05c43917…` times out polling vda and reboots at guest 17.15s; SurfaceFlinger ran stably throughout the full 2400 s flight with 0 aborts, 0 crashes, and 0 restarts; Zygote survived and successfully forked `system_server` 47 times across the flight; however, `system_server` terminated during early bootstrap ~2 seconds after `BatteryService` contacted `healthd`, causing Zygote's SIGCHLD handler to kill Zygote with signal 9 and trigger an init restart loop; `system_server` never reached display power-on or Home activity launch; queueBuffer 0, present 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, the primeCache abort from T707 was completely cured: 0 occurrences of `output buffer not gpu writeable` across all 5,562 lines (dropped from 1 in T707 to 0), 0 occurrences of `failed to create composer client` (dropped from 29 in T707 to 0), and 0 occurrences of `Abort message:` (dropped from 30 in T707 to 0). SurfaceFlinger initialized display 0 and remained alive with 0 crashes throughout the entire 2407 s flight. Zygote was no longer killed by SurfaceFlinger restarts and successfully forked `system_server` 47 times across the flight (first instance PID 2494 at line 3658, guest 181.59s). However, `system_server` repeatedly terminated during early bootstrap ~2 seconds after initializing `BatteryService` with `healthd` (lines 3658–3662: `idmap2d` ctl.start at guest 181.59s, `healthd: battery none chg=` at guest 183.13s, `init: Service 'zygote' received signal 9` at guest 185.44s). As a result, `system_server` never reached complete service startup and `sys.boot_completed` remained unset. In control flight `flight-ab`, `boot_completed=` was empty throughout execution until clean reboot on vda polling timeout at guest 17.15s.

WALL-TYPE-TAG: guest type-1 on FAIL  
Rust par bhi lagu: haan — the same guest userspace display stack, RenderEngine shader cache priming behavior, HWC client lifecycle, system_server bootstrap sequence, and Zygote process supervision are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.116261 s (line 188: `[    3.116261] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.150538 s (line 186: `[    3.150538] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.437164 s (line 187: `[    3.437164] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 6.428018 s (line 515: `[    6.428018] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 6.532844 s (line 516: `[    6.532844] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 6.999632 s (line 514: `[    6.999632] init: init first stage started!`).

system_server:
- In contrast to T707 where SurfaceFlinger repeatedly aborted during primeCache and killed Zygote before `forkSystemServer()`, in T708 `system_server` was successfully forked by Zygote 47 times across the 2400 s window (first instance PID 2494 at line 3658, guest 181.59s: `init: Control message: Processed ctl.start for 'idmap2d' from pid: 2494 (system_server)`).
- However, `system_server` repeatedly terminated ~2s after `BatteryService` contacted `IHealth` / `healthd` (`healthd: battery none chg=`), causing Zygote's SIGCHLD handler to send SIGKILL (signal 9) to itself (`init: Service 'zygote' received signal 9`), triggering init to restart Zygote and repeat the cycle.
- In control flight `flight-ab`, `system_server` did not fork (0) because canonical kernel failed to mount `/system` from `/dev/block/vda` due to lack of `CONFIG_VIRTIO_BLK=y` and triggered `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- `queueBuffer`: **0**

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block devices, but could not detect `/dev/vda` due to absence of `CONFIG_VIRTIO_BLK=y` in the canonical build. At guest 17.034375 s (line 550), Android init logged: `init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`, followed by line 552 `init: InitFatalReboot: signal 6` and line 560 `reboot: Restarting system with command 'bootloader'`. QEMU cleanly rebooted and terminated under `-no-reboot` at guest 17.145 s (host 20.03 s, exit code 0, 560 lines, log SHA-256 `ac66738502486d386c4a6c13e9a90fa3b56fbe3a0d19f1f0e31fce6f3297ffd3`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` repeatedly terminated during early bootstrap before reaching Home activity launch.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T708 executed the two fixes named by T707: (1) PRIMARY — disabling SurfaceFlinger shader-cache priming by setting `service.sf.prime_shader_cache=0` in early boot properties, and (2) SECONDARY — updating `ComposerImpl::createClient` in the Composer 2.1 HIDL vendor service to support client reconnect on death. The full stack was rebuilt, repacked, and flown across the full 2400 s window on Mac QEMU 11.1.2.

### Key Accomplishments
1. **Primary Fix (SurfaceFlinger PrimeCache Disable - 100% CURED)**:
   - Configured `service.sf.prime_shader_cache=0` across the CPIO initramfs boot properties (`build.prop`, `system/build.prop`, `system/etc/ramdisk/build.prop`, `second_stage_resources/system/etc/ramdisk/build.prop`, `system/etc/prop.default`, and `t636-diag.sh`).
   - When second-stage init loaded default properties, `service.sf.prime_shader_cache` was set to `0`. In Android 14 `SurfaceFlinger::init()+3764`, `property_get_bool("service.sf.prime_shader_cache", true)` evaluated to `false`, completely skipping `mRenderEngine->primeCache()`.
   - **Critical Breakthrough**: The primeCache abort wall was completely eliminated!
     - `output buffer not gpu writeable`: dropped from 1 in T707 to **0** in T708 across all 5,562 lines.
     - `Abort message:`: dropped from 30 in T707 to **0** in T708.
     - SurfaceFlinger ran stably throughout the full 2400 s flight with **zero aborts, zero crashes, zero restarts**.
2. **Secondary Fix (Composer 2.1 Reconnect Logic - Rebuilt on Linux Box)**:
   - Updated `ComposerImpl::createClient` in `/home/muse/t681-link/t681-service.cpp` on the Linux box: added code to check if `mClient != nullptr`, log `"reconnect after client death"`, reset the old client reference, and successfully register the new client instead of returning `Error::NO_RESOURCES`.
   - Rebuilt Composer ELF via NDK r29 / clang toolchain (`build-service.sh`). Rebuilt binary: 19,942,712 bytes, SHA-256 `8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401`. Verified reconnect logic present via `strings` and `llvm-objdump` disassembly.
   - Repacked `vendor.img` on Linux box via `/usr/sbin/debugfs`: replaced `/bin/hw/android.hardware.graphics.composer@2.1-service` (mode 0755, uid 0, gid 2000, context `u:object_r:hal_graphics_composer_default_exec:s0`). Clean `e2fsck -f -n` verified returncode 0. New vendor.img: 134,217,728 bytes, SHA-256 `8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092`.
   - Because SurfaceFlinger never crashed, the secondary reconnect path was not exercised during the flight; `failed to create composer client` dropped from 29 in T707 to **0** in T708.
3. **CPIO Initramfs Repack & DTB Alignment (PASS)**:
   - Replaced `ramdisk-images/vendor.img` in the CPIO initramfs and applied boot property modifications (`compresslevel=9`, `mtime=0.0`). Rebuilt initramfs: 53,172,530 bytes, SHA-256 `49d395d3e195cdbe2686c61a3b53b32a48ed16e81b592a67e71f1b8236fe1ad9`.
   - Re-aligned DTB `linux,initrd-end` byte-exactly to `0x110000000 + 53172530 = 0x1132b5932` (`<0x01 0x132b5932>`). New DTB: 2,049 bytes, SHA-256 `9a85c074df4a6c0a84fec65f793cb1b63a73c94ba135d247ea5bf936539fb85c`.
4. **Smoke Flight Proves External Mount & PrimeCache Elimination (PASS)**:
   - 120 s smoke flight verified external virtio-blk `/dev/block/vda` mounted as `/system` cleanly at guest line 523 (`__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`).
   - Freeing kernel memory, init first stage, fb0 simplefb registered. Zero kernel panic/Oops/hang.
   - SurfaceFlinger primeCache skipped cleanly: 0 `output buffer not gpu writeable`, 0 aborts.
5. **Full Flight-01 & Canonical A/B Control Flown (PASS Execution)**:
   - Flight-01 ran full 2400 s target (host 2407.80 s, guest 2407.143 s, exit 0, 5,562 lines, SHA `689f4f84…`).
   - Flight-ab ran on canonical kernel `05c43917…` and confirmed missing `CONFIG_VIRTIO_BLK=y` triggers vda polling timeout and `InitFatalReboot` at guest 17.15 s (SHA `ac667385…`).
6. **system_server Forking Unlocked (47 Instances)**:
   - With SurfaceFlinger no longer crashing, Zygote survived unmolested and successfully invoked `forkSystemServer()` across 47 instances throughout the 2400 s flight (PID 2494, 2769, 3015, 3256, 3500, 3741, 3984, 4221, 4458, 4700, 4951, 5188, 5425, 5667, 5909, 6153, 6386, 6619, 6853...).
7. **Exact First Failing Link Isolated (system_server Early Termination)**:
   - Each `system_server` instance initialized core services (`apexd`, `IPowerStats`, `IStats`, `IPower`, `ILights`, `idmap2d`), but terminated ~2 seconds after `BatteryService` contacted `healthd` (lines 3658–3662: `idmap2d` ctl.start at guest 181.59s, `healthd: battery none chg=` at guest 183.13s, `init: Service 'zygote' received signal 9` at guest 185.44s).
   - Upon `system_server` exit, Zygote's `SIGCHLD` handler in `com_android_internal_os_Zygote.cpp` sent SIGKILL (signal 9) to Zygote, triggering an init onrestart loop.

---

## Input Verification Table

All 11 staged inputs verified in `/Users/Shared/wt-track708-primecache-disable` prior to execution without network fetch:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t707.md` | `48ed44b135f66749ed9f3aa2f4a4fdb1c9b5555f7aa6e375a2f0a9c7af6c58ff` | `48ed44b135f66749ed9f3aa2f4a4fdb1c9b5555f7aa6e375a2f0a9c7af6c58ff` | 19,925 B | MATCH (PASS) |
| `T707 flight-01 UART` | `13699cbebe1149b246409a4bd56e6228ebf4eb59983bc25ea9e0b06cfc72f9c9` | `13699cbebe1149b246409a4bd56e6228ebf4eb59983bc25ea9e0b06cfc72f9c9` | 5,571,703 B | MATCH (PASS) |
| `T707 flight-ab UART` | `51431be7b8f14cea1cde193171a102bda6ff5378c62fa6f4deab64ad76bb57bb` | `51431be7b8f14cea1cde193171a102bda6ff5378c62fa6f4deab64ad76bb57bb` | 46,938 B | MATCH (PASS) |
| `T707 fix initramfs` | `201bcd058b65533ec7ad904bc79ee5b062c51855046b0e787cc81835508f0f94` | `201bcd058b65533ec7ad904bc79ee5b062c51855046b0e787cc81835508f0f94` | 53,172,105 B | MATCH (PASS) |
| `T707 modified.dtb` | `3d8341f4ea6c49f4f4d4eeb8593c3270eedbcc27ae948b21d6e521e200994ba9` | `3d8341f4ea6c49f4f4d4eeb8593c3270eedbcc27ae948b21d6e521e200994ba9` | 2,049 B | MATCH (PASS) |
| `T707 vendor.img` | `983131c300997dd7041e588d1c2fe00ea0d27dbffb3862eb8b37b8b1324b7ad2` | `983131c300997dd7041e588d1c2fe00ea0d27dbffb3862eb8b37b8b1324b7ad2` | 134,217,728 B | MATCH (PASS) |
| `T707 rebuilt Composer ELF` | `781dac3bae61652d424a5f6622ac72d0801fcdcdeab0565445f28668b0eebed5` | `781dac3bae61652d424a5f6622ac72d0801fcdcdeab0565445f28668b0eebed5` | 19,943,144 B | MATCH (PASS) |
| `T706 patched system.img` | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` | 2,059,816,960 B | MATCH (PASS) |
| `T705 rebuilt Image` | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` | 20,126,208 B | MATCH (PASS) |
| `canonical-Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B | MATCH (PASS) |
| `qemu-system-aarch64` | `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869` | `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869` | 31,498,864 B | MATCH (PASS) |

---

## Pre-flight Proofs Table (Rebuilt Artifacts)

| Artifact Name | Generating Procedure | Authoritative SHA-256 | Size (Bytes) | Verification Status |
| :--- | :--- | :--- | :--- | :--- |
| Rebuilt Composer 2.1 ELF | Box clang toolchain with reconnect logic | `8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401` | 19,942,712 B | PASS (`strings` & `llvm-objdump` verified) |
| Repacked `vendor.img` | Linux box debugfs file replace + `e2fsck -f -n` | `8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092` | 134,217,728 B | PASS (inode 93, clean exit 0) |
| Repacked Fix Initramfs | CPIO member replace + property inject | `49d395d3e195cdbe2686c61a3b53b32a48ed16e81b592a67e71f1b8236fe1ad9` | 53,172,530 B | PASS (`service.sf.prime_shader_cache=0`) |
| Realigned DTB | `dtc -I dts -O dtb` to initrd-end `0x1132b5932` | `9a85c074df4a6c0a84fec65f793cb1b63a73c94ba135d247ea5bf936539fb85c` | 2,049 B | PASS (re-decompiled proof matched) |

---

## Flights Execution Table

| Flight Name | Target (s) | Host Elapsed (s) | Guest Max (s) | Exit Code | Lines | UART Log SHA-256 | Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `smoke` | 120 s | 145.27 s | 141.162 s | 0 | 3,657 | `e3ab4d16262fc3bc440d1c9b35ae33ba53dfe54a6fdc1ff9a360d29f9b2e2c45` | PASS |
| `flight-01` | 2400 s | 2407.80 s | 2407.143 s | 0 | 5,562 | `689f4f84df291cbccf0d1d78c1a3ebe1ba53b2a0c717d95ea31e455cbe1ab2d2` | PASS (Target Reached) |
| `flight-ab` | 240 s | 20.03 s | 17.145 s | 0 | 560 | `ac66738502486d386c4a6c13e9a90fa3b56fbe3a0d19f1f0e31fce6f3297ffd3` | PASS (Control Re-proved) |

---

## First Failing Link

### Observation & Evidence
SurfaceFlinger frame presentation was never dispatched to Composer 2.1 (0 `presentDisplay` calls, 0 `validateDisplay` calls, 0 Composer `executeCommands` writes to fb0, 0 guest fb0 pixel writes) because SurfaceFlinger held zero queued buffers (`queueBuffer`: 0).

In Flight-01, SurfaceFlinger's shader cache priming crash was completely solved:
- `output buffer not gpu writeable`: 0 occurrences (down from 1 in T707).
- `failed to create composer client`: 0 occurrences (down from 29 in T707).
- `Abort message:`: 0 occurrences (down from 30 in T707).
- SurfaceFlinger stayed continuously alive from guest 93s through the entire 2407 s flight without a single crash or restart.

Because SurfaceFlinger never crashed, Zygote was no longer repeatedly killed by `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`. Zygote successfully forked `system_server` across 47 instances throughout the flight (first instance PID 2494, guest 181.59s).

However, each `system_server` instance terminated during early bootstrap:
1. `system_server` entered and initialized core IPC and services:
   - Queried `apexd` for active packages (line 3647).
   - Located `android.hardware.power.stats.IPowerStats/default` (line 3648).
   - Located `android.frameworks.stats.IStats/default` (line 3650).
   - Located `android.hardware.power.IPower/default` (line 3652).
   - Located `android.hardware.light.ILights/default` (line 3653).
   - Started `idmap2d` to scan overlays (line 3658: `[  181.592190] init: Control message: Processed ctl.start for 'idmap2d' from pid: 2494 (system_server)`).
   - Initialized `BatteryService` connecting to `android.hardware.health.IHealth/default` (line 3660: `[  183.137512] healthd: battery none chg=`).
2. Approximately 2 seconds after `BatteryService` / `healthd`, `system_server` terminated.
3. In `frameworks/base/core/jni/com_android_internal_os_Zygote.cpp`, Zygote's `SIGCHLD` handler caught the termination of `gSystemServerPid`:
   ```cpp
   if (pid == gSystemServerPid) {
       ALOGE("Exit zygote because system server (%d) has terminated", pid);
       kill(Zygote::gZygotePid, SIGKILL);
   }
   ```
   Zygote sent SIGKILL (signal 9) to itself.
4. Android init logged:
   - Line 3662: `[  185.445701] init: Service 'zygote' (pid 2367) received signal 9`
   - Line 3664: `[  185.447730] libprocessgroup: Successfully killed process cgroup uid 0 pid 2367 in 0ms`
5. Init executed Zygote's `onrestart` actions and restarted Zygote:
   - Line 3675: `[  186.552526] init: starting service 'zygote'...`
   - Line 3680: `[  187.215593] init: ... started service 'zygote' has pid 2666`
6. The new Zygote spawned a new `system_server`, which repeated the identical cycle 47 times across the flight.

Because `system_server` never survived past early bootstrap, `DisplayManagerService` never sent display power mode 2 (NORMAL) to SurfaceFlinger, `ActivityManagerService` never launched `com.android.launcher3`, zero client application windows were bound, zero buffers were submitted (`queueBuffer`: 0), and SurfaceFlinger had no ready buffers to composite (`presentDisplay`: 0).

---

## Exact Next Fix for Track T709

1. **Enable Logcat Streamer / Stderr Pipe in `early-ramdisk`**:
   In `early-ramdisk`, `logcat_streamer` is currently commented out (`# logcat_streamer >/dev/console 2>&1`). Uncomment or add a background logcat pipeline (`/system/bin/logcat -b all -v time *:V >/dev/console 2>&1 &` or `logcat -s AndroidRuntime:E SystemServer:E >/dev/console 2>&1 &`) so that Java stack traces from `system_server` crashes are immediately visible on UART console.
2. **Cure `system_server` Termination Root Cause**:
   Once the exact exception / crash in `system_server` is visible on UART (e.g. following `BatteryService`/`UsageStatsService`/`WebViewUpdateService`/`PackageManagerService`), apply the necessary property, SELinux policy, or service mock in the initramfs to allow `system_server` to complete service bootstrap, power on display 0, and launch `com.android.launcher3`.
