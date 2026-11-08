# DEATH NOTE — T706

Lane: Track T706 — Clean GSI system.img Patched on Linux via Loopback Mount + Resolve HWC Hotplug Abort, Fly for First Queued Buffer + Present Dispatch (Mac agyx)
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 12 GiB available; Step 1 verified all 10 staged inputs and proved clean base system.img block 145414 reads original fill 4ea21150…; Step 2 Linux loop patch applied on box, block 145414 unshare proof verified 4ea21150… untouched, patched APK 8a28e1db… verified with exact opcodes, oat/arm64 absent, services.jar byte-identical d4108052…, e2fsck clean exit 0, and critical open item resolved reconciling post-hash e2fsck superblock timestamp variance to authoritative SHA 2de0533e…; Step 3a smoke flight proved external virtio-blk /dev/block/vda mounts cleanly as /system at guest 7.05s and Zygote SIGSEGV eliminated [0]; Step 3b Flight-01 reached full 2400 s window [guest 2406.817 s, host 2407.74 s, exit 0, 53,664 lines, log SHA 3e8121d0…]; Step 3c Flight-ab canonical control proved canonical kernel 05c43917… times out polling vda and aborts at guest 17.75s; however, vendor.hwc-2-1 never issued a display hotplug callback to SurfaceFlinger, causing SurfaceFlinger to abort 31 times with 'Initial display configuration failed: HWC did not hotplug' [tombstone_00 at guest 102.68s through tombstone_30 at 2350.22s], which repeatedly triggered init onrestart killing Zygote before forkSystemServer(); system_server did not fork; queueBuffer 0, present 0)
UTC close: 2026-11-08T19:40:00Z
Worktree: /Users/Shared/wt-track706-clean-gsi-hwc-hotplug
Branch: t706 (from T705 commit d796c8d)

## What ended

Track T706 was opened to cure the inherited services.jar corruption in external `system.img` (block 145414), resolve the HWC hotplug abort, and fly for the first queued buffer and present dispatch on the T705 Route B kernel stack.

1. **Recovery & Pre-Flight Hash Reconciliation**:
   - Resumed execution from agent PID 82802 death.
   - Identified and resolved the critical open item regarding `guest-inputs/system.img`: pinpointed the 5-byte diff in MB 0 to post-hash `e2fsck -f -y` updating ext4 superblock timestamps (`s_wtime`, `s_lastcheck`, `s_checksum`), while chunks 1..1964 were 100% bit-for-bit identical to the Box image.
   - Restored MB 0 to authoritative Box data (authoritative SHA `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601`), updated verification script to use read-only `e2fsck -f -n` (exit 0), and re-proved all Step 2 pre-flight gates against `2de0533e…`.
2. **Inherited services.jar Corruption Completely Cured (0 ArtField SIGSEGV)**:
   - Block 145414 fill verified `4ea21150edc8a12cbbefa883251e77bf106f8f88e4a07673ca9fc416925269ea` (100% untouched fill pattern).
   - Patched APK `8a28e1db…` verified with exact opcodes (`0x199a10=00000000`, `0x199a90=1200`, `0x19b112=0102000000000000`).
   - `oat/arm64` directory absent (de-shadowed).
   - `services.jar` confirmed byte-identical to original clean base (`d4108052…`).
   - In Flight-01 (2406.8 s guest, 53,664 lines), `art::ArtField::GetDeclaringClass` SIGSEGV dropped from 7 occurrences in T705 to **0 occurrences** in T706.
3. **Step 3a — 120 s Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 125.27 s, guest 123.637 s, exit 0, 4,117 lines, log SHA `e43aa379…`.
   - Verified virtio-blk probe and clean `/dev/block/vda` mount as `/system` at guest line 524 (`__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`).
   - Verified freeing unused kernel memory (guest 3.47s), init first stage (guest 7.05s), fb0 simplefb registered. Zero kernel panic.
4. **Step 3c — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 20.02 s, guest 17.757 s, exit 0, 559 lines, log SHA `27163c45…`.
   - Proved canonical missing `vda` failure: line 551 `partition(s) not found after polling timeout: vda`, triggering `InitFatalReboot`. Conclusively re-proves `CONFIG_VIRTIO_BLK=y` is the sole enabler of the external mount.
5. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2407.74 s, guest 2406.817 s, exit 0, 53,664 lines, log SHA `3e8121d0…`.
   - External `/dev/block/vda` mounted cleanly as `/system` at guest 7.41s (line 524).

## Why the verdict is FAIL

Under the proving criteria of Track T706:
1. **`sys.boot_completed=1` Not Observed (0)**:
   - `sys.boot_completed` remained unset throughout the 2400 s window.
2. **`system_server` Did Not Fork (0)**:
   - SurfaceFlinger failed initialization and aborted 31 times with:
     `Abort message: 'Initial display configuration failed: HWC did not hotplug'` (tombstone_00 at 102.68s through tombstone_30 at 2350.22s).
   - Each SurfaceFlinger death triggered `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`, killing Zygote via signal 9 before Zygote could reach `forkSystemServer()`.
3. **QueueBuffer & Present Dispatch Not Reached (0)**:
   - `queueBuffer`: 0
   - `presentDisplay`: 0, `validateDisplay`: 0, `presentOrValidate`: 0, `executeCommands`: 0.

## Exact First Failing Link & Exact Next Fix (Track T707)

1. **First Failing Link**:
   - `vendor.hwc-2-1` (PID 2371, T681 Composer 2.1 service) receives non-fatal binder ioctl `-22` (`BINDER_ENABLE_ONEWAY_SPAM_DETECTION`), but never issues a display hotplug callback (`onHotplug(displayId, Connection::CONNECTED)`) to SurfaceFlinger.
   - SurfaceFlinger in Android 14 mandates a primary display hotplug during initialization; lacking it, SurfaceFlinger aborts with `Initial display configuration failed: HWC did not hotplug`.
   - Android init cascades each SurfaceFlinger abort into `restart --only-if-running zygote`, terminating Zygote before `forkSystemServer()`.
2. **Exact Next Fix (Track T707)**:
   - Instrument `/vendor/bin/hw/android.hardware.graphics.composer@2.1-service` to dispatch `IComposerCallback::onHotplug(PRIMARY_DISPLAY_ID, IComposerCallback::Connection::CONNECTED)` immediately upon client callback registration (`ComposerClientImpl::registerCallback`).
   - Alternatively, decouple SurfaceFlinger failure from Zygote termination in `init.rc` (`surfaceflinger.rc` onrestart action) or provide a virtual display fallback so that Zygote is permitted to fork `system_server`.
