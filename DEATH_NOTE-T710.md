# DEATH NOTE — T710

Lane: Track T710 — Cure the sensorservice wall + fly for the first queued buffer + first present dispatch (Mac agyx)  
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 10.86 GiB available [re-verified 9.5 GiB, >= 8 GiB threshold]; Step 1 verified all 13 staged inputs byte-for-byte without network fetch; Step 2 implemented Priority 1 sensorservice wall cure [`dummy_sensorservice` binder daemon 9,448 B, SHA `867a5bca…`; updated `t710_logcat_streamer` supervisor 780,432 B, SHA `a8eafb18…`; repacked CPIO initramfs `t710-fix.cpio.gz` 53,501,694 B, SHA `e839cb18cd60068d804bb0a93fc69b6c31ebe5ebbcd93cdd0bf30bb088066665`; realigned DTB `t710-modified.dtb` 2,049 B, SHA `1ef7939b7058305b857da4bd1f449493552bfb12c0a8e8d8d0cb808dfb9c2627` to `0x113305efe`]; Step 3a smoke flight on Mac QEMU 11.1.2 [host 125.26s, guest 123.95s, exit 0, 11,123 lines, SHA `3143c2fb…`] proved external `/dev/block/vda` mounted as `/system` at guest 8.70s, simplefb registered, primeCache skipped with 0 aborts, and 0 composer client failures, while exposing `dummy_sensorservice` registration was SELinux-denied by servicemanager [`scontext=u:r:kernel:s0 permissive=0`]; Step 3b flight-ab canonical control on `05c43917…` [host 25.03s, guest 19.95s, exit 0, 573 lines, log SHA `923025b8…`] re-proved missing `vda` polling timeout and `InitFatalReboot` due to lack of `CONFIG_VIRTIO_BLK=y`; Step 3b flight-01 ran full 2400s guest window [host 2402.96s, guest 2401.63s, exit 0, 61,085 lines, log SHA `e02f22e7007f66cd9f9cdadd5e9e47e2bef621256531742c6e224819f15ca9b9`]; guest UART proved `dummy_sensorservice` attempted registration 461 times but each was denied by servicemanager SELinux domain check [`avc: denied { add } for pid=... name=sensorservice scontext=u:r:kernel:s0 tcontext=u:object_r:sensorservice_service:s0 tclass=service_manager permissive=0`]; consequently `sensorservice` remained unregistered on `/dev/binder`, `SystemSensorManager.nativeCreate` blocked on the main thread in `DeviceStateProviderImpl.setStateConditions`, and `Watchdog` killed system_server across 10 instances after 81-82s each [PIDs 2574, 3246, 3897, 4538, 5181, 5820, 6455, 7105, 7747, 8391], with the 11th instance [PID 9018] active at cutoff; queueBuffer 0, presentDisplay 0)  
UTC close: 2026-11-09T07:10:00Z  
Worktree: /Users/Shared/wt-track710-sensorservice-wall-cure  
Branch: t710  

## What ended

Track T710 was opened to cure the sensorservice wall so `system_server` survives `startOtherServices`, and fly the full 2400 s guest window for the first `queueBuffer` and first present dispatch on the T709 stack on Mac QEMU 11.1.2.

1. **Step 0 — Mac Disk Gate: PASS**:
   - 10.86 GiB available on `/System/Volumes/Data` at launch; 9.5 GiB re-verified at recovery. Exceeds the 8 GiB threshold. No WAIT-DISK.
2. **Step 1 — Verify Inputs: PASS**:
   - All 13 staged inputs verified byte-for-byte against specifications with zero network fetch.
3. **Step 2 — Priority 1 Cure Implemented & Repacked: PASS**:
   - Dynamically cross-compiled `dummy_sensorservice` (9,448 B, SHA `867a5bca…`), linking against `libbinder_ndk.so`.
   - Statically cross-compiled `t710_logcat_streamer` (780,432 B, SHA `a8eafb18…`), incorporating a supervisor thread for `dummy_sensorservice`.
   - Repacked CPIO initramfs `t710-fix.cpio.gz` (53,501,694 B, SHA `e839cb18cd60068d804bb0a93fc69b6c31ebe5ebbcd93cdd0bf30bb088066665`).
   - Re-aligned DTB `t710-modified.dtb` (2,049 B, SHA `1ef7939b7058305b857da4bd1f449493552bfb12c0a8e8d8d0cb808dfb9c2627`) to `linux,initrd-end = <0x01 0x13305efe>`.
4. **Step 3a — Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 125.26 s, guest 123.95 s, exit 0, 11,123 lines, SHA `3143c2fb…`.
   - External `/dev/block/vda` mounted as `/system` Success at guest 8.70 s (line 528).
   - `fb0: simplefb registered!` at guest 0.31 s (line 97).
   - `primeCache` skipped (0 aborts), composer failures 0.
   - Diagnosed live SELinux denial on `dummy_sensorservice` registration.
5. **Step 3b — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 25.03 s, guest 19.95 s, exit 0, 573 lines, SHA `923025b8…`.
   - Proved `CONFIG_VIRTIO_BLK=y` requirement: partition polling timed out and triggered `InitFatalReboot` at guest 19.86 s (line 565).
6. **Step 3b — Flight-01 (Full 2400 s Window): PASS Execution**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2402.96 s, guest 2401.63 s, exit 0, 61,085 lines, SHA `e02f22e7007f66cd9f9cdadd5e9e47e2bef621256531742c6e224819f15ca9b9`.
   - Target reached cleanly (`Guest target reached (2401.6s >= 2400s)`).
   - Zero primeCache aborts, zero composer failures, zero database crashes.

## Why the verdict is FAIL

Under the proving criteria of Track T710:
1. **`sys.boot_completed=1` Not Observed (0)**:
   `sys.boot_completed` remained unset throughout the 2400 s window.
2. **Watchdog Killing system_server on sensorservice Blockage**:
   `servicemanager` denied `dummy_sensorservice` the `{ add }` permission for `sensorservice` because the daemon inherited `scontext=u:r:kernel:s0` from `early-ramdisk`, and `servicemanager` enforces SELinux rules in userspace (`permissive=0`).
   As a result, `sensorservice` was never registered on `/dev/binder`.
   In `system_server`, `DeviceStateProviderImpl.setStateConditions()` called `SystemSensorManager.nativeCreate()`, blocking the main thread on `waitForService("sensorservice")`.
   `Watchdog` killed `system_server` with SIGKILL across 10 instances after 81-82 seconds each (PIDs 2574, 3246, 3897, 4538, 5181, 5820, 6455, 7105, 7747, 8391). The 11th instance (PID 9018) was active at cutoff.
3. **QueueBuffer & Present Dispatch Not Reached (0)**:
   `queueBuffer`: 0, `presentDisplay`: 0, `validateDisplay`: 0.

## Exact First Failing Link & Exact Next Fix (Track T711)

1. **First Failing Link**:
   `servicemanager` denies `dummy_sensorservice` the `{ add }` permission for `sensorservice_service` because the process runs under `scontext=u:r:kernel:s0` where `permissive=0` is enforced by `servicemanager`. Because `sensorservice` is not registered in the binder registry, `SystemSensorManager.nativeCreate()` blocks `system_server`'s main thread in `DeviceStateProviderImpl.setStateConditions`, causing `Watchdog` to kill `system_server` every ~82 seconds.

2. **Exact Next Fix (Track T711)**:
   - **Option A (SELinux domain fix for Priority 1 cure)**: Launch `dummy_sensorservice` under a domain permitted to add `sensorservice_service` (e.g. `u:r:su:s0`, `u:r:system_server:s0`, or run via `runcon u:r:su:s0 /data/dummy_sensorservice`), or patch `servicemanager` / SELinux policy in `system.img` / `vendor.img` so `u:r:kernel:s0` has `add` permission for `sensorservice_service`.
   - **Option B (Priority 2 cure — hals.conf)**: Deploy dummy `/odm/etc/sensors/hals.conf` (or `/vendor/etc/sensors/hals.conf`) in `vendor.img` on the Linux box via loopback mount so `SensorsMultiHal` loads and registers `sensorservice` natively under its own domain.
   - **Option C (Priority 3 cure — disable DeviceState sensor conditions)**: Set system property or framework overlay to prevent `DeviceStateManagerService` from creating `SensorManager` during `startOtherServices()`.
