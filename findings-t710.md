# Findings — Track T710: Cure the sensorservice wall + fly for the first queued buffer + first present dispatch (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track710-sensorservice-wall-cure`  
Branch: `t710`  
Predecessor: T709 driver-certified honest FAIL (commit 01229a4: Logcat streamer enabled in early-ramdisk and proved operational; SettingsProvider SQLiteCantOpenDatabaseException cured via ext4 /data pre-creation; system_server advanced into startOtherServices, but blocked in SystemSensorManager.nativeCreate waiting for sensorservice which never published, causing 9 Watchdog kills across 2400s flight; queueBuffer 0, presentDisplay 0).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 10.86 GiB available on /System/Volumes/Data [re-verified 9.5 GiB, >= 8 GiB threshold]; Step 1 verified all 13 staged inputs byte-for-byte against specifications with zero network fetch; Step 2 implemented Priority 1 cure [mock/stub sensorservice binder daemon `dummy_sensorservice` dynamically cross-compiled aarch64, 9,448 B, SHA `867a5bca…`; updated `t710_logcat_streamer` supervisor daemon statically cross-compiled aarch64, 780,432 B, SHA `a8eafb18…` launched before `exec /init`; repacked CPIO initramfs `t710-fix.cpio.gz`, 53,501,694 B, SHA `e839cb18cd60068d804bb0a93fc69b6c31ebe5ebbcd93cdd0bf30bb088066665`; realigned DTB `t710-modified.dtb`, 2,049 B, SHA `1ef7939b7058305b857da4bd1f449493552bfb12c0a8e8d8d0cb808dfb9c2627` byte-exactly to `0x113305efe`]; Step 3a smoke flight on Mac QEMU 11.1.2 [host 125.26s, guest 123.95s, exit 0, 11,123 lines, SHA `3143c2fb…`] proved external `/dev/block/vda` mounted as `/system` at guest 8.70s, simplefb registered, primeCache skipped with 0 aborts, and 0 composer client failures, while revealing `dummy_sensorservice` registration was SELinux-denied by servicemanager [`scontext=u:r:kernel:s0 permissive=0`]; Step 3b flight-ab canonical control on `05c43917…` [host 25.03s, guest 19.95s, exit 0, 573 lines, log SHA `923025b8…`] re-proved missing `vda` polling timeout and `InitFatalReboot` due to lack of `CONFIG_VIRTIO_BLK=y`; Step 3b flight-01 ran full 2400s guest window [host 2402.96s, guest 2401.63s, exit 0, 61,085 lines, log SHA `e02f22e7007f66cd9f9cdadd5e9e47e2bef621256531742c6e224819f15ca9b9`]; guest UART proved `dummy_sensorservice` attempted registration 461 times but each was denied by servicemanager SELinux domain check [`avc: denied { add } for pid=... name=sensorservice scontext=u:r:kernel:s0 tcontext=u:object_r:sensorservice_service:s0 tclass=service_manager permissive=0`]; consequently `sensorservice` remained unregistered on `/dev/binder`, `SystemSensorManager.nativeCreate` blocked on the main thread in `DeviceStateProviderImpl.setStateConditions`, and `Watchdog` killed system_server across 10 instances after 81-82s each [PIDs 2574, 3246, 3897, 4538, 5181, 5820, 6455, 7105, 7747, 8391], with the 11th instance [PID 9018] active at cutoff; queueBuffer 0, presentDisplay 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, the primeCache abort from T707 remained completely cured (0 occurrences of `output buffer not gpu writeable`, 0 `failed to create composer client`, 0 `Abort message:`). The T708/T709 database pre-creation fix held throughout the flight (0 `SQLiteCantOpenDatabaseException`). However, the Priority 1 cure (`dummy_sensorservice`) failed to register `sensorservice` with Android's `servicemanager` because servicemanager enforced userspace SELinux checks (`permissive=0`) on the calling domain `u:r:kernel:s0`, rejecting `AServiceManager_addService` with returncode -1 (461 registration attempts, 4,597 sensorservice avc denials). As a result, `sensorservice` was never published on `/dev/binder`, `SystemSensorManager.nativeCreate` hung on the main thread during `DeviceStateProviderImpl.setStateConditions`, and `Watchdog` SIGKILLed `system_server` across 10 distinct instances (PIDs 2574, 3246, 3897, 4538, 5181, 5820, 6455, 7105, 7747, 8391), with the 11th instance (PID 9018) alive at guest cutoff (2401.63s). Thus `sys.boot_completed` remained unset. In canonical control flight `flight-ab`, `boot_completed` remained unset until clean reboot on vda polling timeout at guest 19.86s.

WALL-TYPE-TAG: guest type-1 on FAIL  
Rust par bhi lagu: haan — the Android userspace binder service manager SELinux permissions, `servicemanager` IPC domain constraints, `SystemSensorManager` bootstrap dependency, and Zygote lifecycle are identical regardless of whether QEMU or a Rust emulator runs the guest.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.100690 s (line 188: `[    3.100690] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.496266 s (line 186: `[    3.496266] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.132310 s (line 188: `[    3.132310] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 8.311798 s (line 519: `[    8.311798] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 9.310464 s (line 521: `[    9.310464] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 8.313720 s (line 519: `[    8.313720] init: init first stage started!`).

system_server:
- In flight `flight-01`, a total of 11 `system_server` instances were spawned by Zygote:
  - Instance 1 (PID 2574): line 11220 (`I/system_server_start( 2574): [1,138689,138689]`) -> Watchdog killed line 15113 after 82s main thread hang.
  - Instance 2 (PID 3246): line 16409 (`I/system_server_start( 3246): [2,394554,394554]`) -> Watchdog killed line 19792 after 82s main thread hang.
  - Instance 3 (PID 3897): line 21051 (`I/system_server_start( 3897): [3,620417,620417]`) -> Watchdog killed line 24436 after 82s main thread hang.
  - Instance 4 (PID 4538): line 25380 (`I/system_server_start( 4538): [4,836182,836182]`) -> Watchdog killed line 28697 after 82s main thread hang.
  - Instance 5 (PID 5181): line 29937 (`I/system_server_start( 5181): [5,1056990,1056990]`) -> Watchdog killed line 33230 after 82s main thread hang.
  - Instance 6 (PID 5820): line 35015 (`I/system_server_start( 5820): [6,1267704,1267704]`) -> Watchdog killed line 38325 after 82s main thread hang.
  - Instance 7 (PID 6455): line 39747 (`I/system_server_start( 6455): [7,1481585,1481585]`) -> Watchdog killed line 43089 after 82s main thread hang.
  - Instance 8 (PID 7105): line 44933 (`I/system_server_start( 7105): [8,1707239,1707239]`) -> Watchdog killed line 48283 after 82s main thread hang.
  - Instance 9 (PID 7747): line 49825 (`I/system_server_start( 7747): [9,1928034,1928034]`) -> Watchdog killed line 53127 after 82s main thread hang.
  - Instance 10 (PID 8391): line 54980 (`I/system_server_start( 8391): [10,2151095,2151095]`) -> Watchdog killed line 58256 after 81s main thread hang.
  - Instance 11 (PID 9018): line 59666 (`I/system_server_start( 9018): [11,2365473,2365473]`) -> Alive at cutoff (guest 2401.63s).
- In control flight `flight-ab`, `system_server` did not fork (0) because the canonical kernel failed to mount `/system` from `/dev/block/vda` due to lack of `CONFIG_VIRTIO_BLK=y`, triggering `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- `queueBuffer`: **0**

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block nodes, but failed to detect `/dev/block/vda` due to absence of `CONFIG_VIRTIO_BLK=y`. At guest 19.86 s, Android init timed out polling vda and triggered `InitFatalReboot` (signal 6, line 565). QEMU cleanly terminated under `-no-reboot` at guest 19.95 s (host 25.03 s, exit code 0, 573 lines, log SHA-256 `923025b8e180ac74bf12c298501f31cb109e6918bc83b1db6f1b203652a137c5`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` repeatedly terminated during `startOtherServices` before reaching Home activity launch.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T710 executed the Priority 1 cure for the `sensorservice` wall diagnosed in T709: cross-compiling a dedicated binder daemon (`dummy_sensorservice`) to register the `sensorservice` binder interface, integrating it into the initramfs under `t710_logcat_streamer` supervision, repacking the CPIO initramfs and byte-aligning the DTB, and flying the complete test matrix (Step 3a smoke flight, Step 3b canonical control `flight-ab`, and Step 3b full 2400 s `flight-01`) on Mac QEMU 11.1.2.

### Key Accomplishments & Evidence
1. **Priority 1 Cure Implemented and Verified (Step 2 PASS)**:
   - Dynamically cross-compiled `dummy_sensorservice` aarch64 native binary (9,448 B, SHA `867a5bca…`), linking against Android's `libbinder_ndk.so` via `AServiceManager_addService`.
   - Statically cross-compiled `t710_logcat_streamer` (780,432 B, SHA `a8eafb18…`), incorporating a supervisor thread that launches `/data/dummy_sensorservice` in the Android root namespace once `/dev/binder` is available.
   - Repacked CPIO initramfs `t710-fix.cpio.gz` (53,501,694 B, SHA `e839cb18cd60068d804bb0a93fc69b6c31ebe5ebbcd93cdd0bf30bb088066665`, compresslevel=9).
   - Re-aligned DTB `t710-modified.dtb` (2,049 B, SHA `1ef7939b7058305b857da4bd1f449493552bfb12c0a8e8d8d0cb808dfb9c2627`) with `linux,initrd-end = <0x01 0x13305efe>`.
2. **Smoke Flight (Step 3a PASS)**:
   - Flown on Mac QEMU 11.1.2: host 125.26 s, guest 123.95 s, exit 0, 11,123 lines, SHA `3143c2fb…`.
   - External `/dev/block/vda` mounted as `/system` at guest 8.70 s (`Success`).
   - `fb0: simplefb registered!` at guest 0.31 s.
   - `primeCache` skipped (0 aborts), composer client failures: 0.
   - Live UART diagnostic revealed `dummy_sensorservice` launched and executed, but `AServiceManager_addService` returned -1 due to SELinux denial.
3. **Canonical Control Flight-ab (Step 3b PASS)**:
   - Flown on canonical kernel `05c43917…`: host 25.03 s, guest 19.95 s, exit 0, 573 lines, SHA `923025b8…`.
   - Proved `CONFIG_VIRTIO_BLK=y` requirement: partition polling timed out and triggered `InitFatalReboot` at guest 19.86 s.
4. **Full 2400 s Flight-01 (Step 3b PASS Execution)**:
   - Flown on rebuilt kernel `53d9563e…`: host 2402.96 s, guest 2401.63 s, exit 0, 61,085 lines, SHA `e02f22e7007f66cd9f9cdadd5e9e47e2bef621256531742c6e224819f15ca9b9`.
   - Re-verified guest boot pipeline: Freeing kernel memory @3.10s (line 188), Init first stage @8.31s (line 519), VDA mount @8.69s (line 528).
   - Zero primeCache aborts, zero composer failures, zero database crashes.
5. **Exact Mechanism of Failure Uncovered**:
   - `dummy_sensorservice` executed in the `u:r:kernel:s0` SELinux domain (inherited from the early-ramdisk launcher).
   - When calling `AServiceManager_addService("sensorservice", ...)`, Android's `servicemanager` queried its SELinux policy:
     ```
     SELinux: avc: denied { add } for pid=... name=sensorservice scontext=u:r:kernel:s0 tcontext=u:object_r:sensorservice_service:s0 tclass=service_manager permissive=0
     ```
   - Because `servicemanager` enforces domain restrictions even in permissive mode for critical service registration (`permissive=0`), the registration failed with returncode -1.
   - `dummy_sensorservice` retried 461 times across the flight, each denied.
   - In `system_server`, `DeviceStateProviderImpl.setStateConditions` called `SystemSensorManager.nativeCreate()`, which called `waitForService("sensorservice")` on `/dev/binder`.
   - The main thread hung indefinitely, triggering `Watchdog` SIGKILL after 81-82 seconds across 10 instances.

---

## Exact First Failing Link & Exact Next Fix (Track T711)

1. **First Failing Link**:
   `servicemanager` denies `dummy_sensorservice` the `{ add }` permission for `sensorservice_service` because the process runs under `scontext=u:r:kernel:s0` where `permissive=0` is enforced by `servicemanager`. Because `sensorservice` is not registered in the binder registry, `SystemSensorManager.nativeCreate()` blocks `system_server`'s main thread in `DeviceStateProviderImpl.setStateConditions`, causing `Watchdog` to kill `system_server` every ~82 seconds.

2. **Exact Next Fix (Track T711)**:
   - **Option A (SELinux domain fix for Priority 1 cure)**: Launch `dummy_sensorservice` under a domain permitted to add `sensorservice_service` (e.g. `u:r:su:s0`, `u:r:system_server:s0`, or run via `runcon u:r:su:s0 /data/dummy_sensorservice`), or patch `servicemanager` / SELinux policy in `system.img` / `vendor.img` so `u:r:kernel:s0` has `add` permission for `sensorservice_service`.
   - **Option B (Priority 2 cure — hals.conf)**: Deploy dummy `/odm/etc/sensors/hals.conf` (or `/vendor/etc/sensors/hals.conf`) in `vendor.img` on the Linux box via loopback mount so `SensorsMultiHal` loads and registers `sensorservice` natively under its own domain.
   - **Option C (Priority 3 cure — disable DeviceState sensor conditions)**: Set system property or framework overlay to prevent `DeviceStateManagerService` from creating `SensorManager` during `startOtherServices()`.
