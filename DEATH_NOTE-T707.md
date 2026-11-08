# DEATH NOTE — T707

Lane: Track T707 — Instrument the Composer 2.1 service to dispatch onHotplug on callback registration + fly for the first queued buffer + first present dispatch (Mac agyx)
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 11 GiB available; Step 1 verified all 10 staged inputs byte-for-byte without network fetch; Step 2 instrumented Composer 2.1 service on the Linux box in `ComposerClientImpl::registerCallback` to immediately dispatch `onHotplug(PRIMARY_DISPLAY_ID, Connection::CONNECTED)`, rebuilt ELF `781dac3b…` with llvm-objdump disassembly proof, repacked `vendor.img` `983131c3…` on Linux loop device with clean `e2fsck -f -n` returncode 0, repacked CPIO initramfs `201bcd05…` with embedded vendor.img hash match, and realigned DTB `3d8341f4…` byte-exactly to `0x1132b5789`; Step 3a smoke flight proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest 7.41s and proved the HWC hotplug wall is 100% CURED ['Initial display configuration failed: HWC did not hotplug' dropped from 31 to 0]; Step 3b Flight-01 reached full 2400 s window [guest 2406.15s, host 2407.74s, exit 0, 51,999 lines, log SHA `13699cbe…`]; Step 3c Flight-ab canonical control proved canonical kernel `05c43917…` times out polling vda and reboots at guest 17.85s; however, after SurfaceFlinger successfully initialized display 0 via the hotplug event, it advanced to SkiaRenderEngine::primeCache() in `SurfaceFlinger::init()+3764` and aborted with `Abort message: 'output buffer not gpu writeable'`, triggering init onrestart killing Zygote before forkSystemServer(); system_server did not fork; queueBuffer 0, present 0)
UTC close: 2026-11-08T22:00:00Z
Worktree: /Users/Shared/wt-track707-composer-hotplug-dispatch
Branch: t707 (from T706 commit 3d4987f)

## What ended

Track T707 was opened to instrument the Composer 2.1 service to dispatch `onHotplug` on callback registration, rebuild the service ELF on the Linux box, package it into the vendor partition, and fly for the first queued buffer and first present dispatch on the T706 clean stack.

1. **Instrumentation & Pre-Flight Rebuild on Linux Box**:
   - Modified `ComposerClientImpl::registerCallback` in `/home/muse/t681-link/t681-service.cpp` on the Linux box.
   - Dispatched `callback->onHotplug(PRIMARY_DISPLAY_ID, IComposerCallback::Connection::CONNECTED)` immediately on client registration.
   - Rebuilt ELF via NDK r29 / clang toolchain (`build-service.sh`): 19,943,144 bytes, SHA-256 `781dac3bae61652d424a5f6622ac72d0801fcdcdeab0565445f28668b0eebed5`.
   - Disassembly verified with `llvm-objdump`: verified immediate vtable call to `onHotplug` with `x1=0` and `w2=1`.
2. **Vendor Repack & DTB Re-alignment on Linux Box**:
   - Repacked `vendor.img` on Linux via `/usr/sbin/debugfs` and `/usr/sbin/e2fsck`: replaced `/bin/hw/android.hardware.graphics.composer@2.1-service` (inode 93) with mode 0755, uid 0, gid 2000, and SELinux context `u:object_r:hal_graphics_composer_default_exec:s0`. Clean `e2fsck -f -n` verified returncode 0. New vendor.img: 134,217,728 bytes, SHA-256 `983131c300997dd7041e588d1c2fe00ea0d27dbffb3862eb8b37b8b1324b7ad2`.
   - Replaced CPIO member `ramdisk-images/vendor.img` in fix initramfs (`compresslevel=9`, `mtime=0.0`). Rebuilt initramfs: 53,172,105 bytes, SHA-256 `201bcd058b65533ec7ad904bc79ee5b062c51855046b0e787cc81835508f0f94`.
   - Re-aligned DTB `linux,initrd-end` byte-exactly to `0x110000000 + 53172105 = 0x1132b5789`. New DTB: 2,049 bytes, SHA-256 `3d8341f4ea6c49f4f4d4eeb8593c3270eedbcc27ae948b21d6e521e200994ba9`.
3. **Step 3a — 120 s Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 125.25 s, guest 124.544 s, exit 0, 3,992 lines, log SHA `b0ab1507…`.
   - Verified virtio-blk probe and clean `/dev/block/vda` mount as `/system` at guest line 524 (`__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`).
   - Verified freeing unused kernel memory (guest 3.44s), init first stage (guest 7.01s), fb0 simplefb registered (guest 0.32s). Zero kernel panic/Oops.
   - **HWC Hotplug Breakthrough**: `Initial display configuration failed: HWC did not hotplug` dropped from 31 occurrences in T706 to **0 occurrences** in T707. SurfaceFlinger hotplugged display 0 and initialized display device successfully.
4. **Step 3c — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 20.03 s, guest 17.848 s, exit 0, 560 lines, log SHA `51431be7…`.
   - Proved canonical missing `vda` failure: line 550 `partition(s) not found after polling timeout: vda`, triggering `InitFatalReboot` at line 552 and clean reboot at line 560. Re-proves `CONFIG_VIRTIO_BLK=y` is required for external image mount.
5. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2407.74 s, guest 2406.15 s, exit 0, 51,999 lines, log SHA `13699cbe…`.
   - External `/dev/block/vda` mounted cleanly as `/system` at guest 6.77s (line 524).

## Why the verdict is FAIL

Under the proving criteria of Track T707:
1. **`sys.boot_completed=1` Not Observed (0)**:
   - `sys.boot_completed` remained unset throughout the 2400 s window.
2. **`system_server` Did Not Fork (0)**:
   - SurfaceFlinger passed the HWC hotplug gate and proceeded to line 3764 in `SurfaceFlinger::init()`: calling `SkiaRenderEngine::primeCache()`.
   - Inside `SkiaRenderEngine::drawLayersInternal` (line 3695), SurfaceFlinger aborted with signal 6:
     `Abort message: 'output buffer not gpu writeable'`.
   - Each SurfaceFlinger death triggered `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`, killing Zygote via signal 9 before Zygote could reach `forkSystemServer()`.
3. **QueueBuffer & Present Dispatch Not Reached (0)**:
   - `queueBuffer`: 0
   - `presentDisplay`: 0, `validateDisplay`: 0, `presentOrValidate`: 0, `executeCommands`: 0.

## Exact First Failing Link & Exact Next Fix (Track T708)

1. **First Failing Link**:
   - SurfaceFlinger in Android 14 `SurfaceFlinger::init()` calls `mRenderEngine->primeCache()` to prime the Skia shader cache.
   - `SkiaRenderEngine::drawLayersInternal` asserts `LOG_ALWAYS_FATAL_IF(!(buffer->getUsage() & GraphicBuffer::USAGE_HW_RENDER), "output buffer not gpu writeable")`.
   - The virtual display buffer allocated by gralloc for offscreen cache priming does not contain `USAGE_HW_RENDER` (or gralloc strips it in software framebuffer mode), triggering SIGABRT.
   - Android init onrestart cascades each SurfaceFlinger abort into `restart --only-if-running zygote`, terminating Zygote before `forkSystemServer()`.
2. **Exact Next Fix (Track T708)**:
   - **Fix 1 (Primary)**: Disable SurfaceFlinger shader cache priming by setting `service.sf.prime_shader_cache=0` (or `debug.renderengine.skia.cache.disable=1` / `ro.surface_flinger.prime_shader_cache=false`) in `init.virt.rc` or `system/build.prop`. SurfaceFlinger checks `property_get_bool("service.sf.prime_shader_cache", true)`; setting it to `0` cleanly skips `mRenderEngine->primeCache()` and allows `SurfaceFlinger::init()` to complete successfully.
   - **Fix 2 (Secondary)**: Update `ComposerImpl::createClient` in `t681-service.cpp` to handle client reconnection (implement `linkToDeath` or allow overwriting `mClient`), eliminating `failed to create composer client` errors if SurfaceFlinger ever restarts.
