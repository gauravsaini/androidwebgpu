# Findings — Track T707: Instrument Composer 2.1 Service to Dispatch onHotplug on Callback Registration + Flight-01 & Canonical Control (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track707-composer-hotplug-dispatch`  
Branch: `t707`, from `3d4987f` (the T706 commit)  
Predecessor: T706 driver-certified honest FAIL (clean base system.img cured services.jar corruption with 0 ArtField SIGSEGV, but isolated independent HWC hotplug wall where vendor.hwc-2-1 never sent onHotplug to SurfaceFlinger, causing 31 SurfaceFlinger aborts and init onrestart SIGKILLing Zygote before forkSystemServer()).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 11 GiB available; Step 1 verified all 10 staged inputs byte-for-byte without network fetch; Step 2 instrumented Composer 2.1 service on the Linux box in `ComposerClientImpl::registerCallback` to immediately dispatch `onHotplug(PRIMARY_DISPLAY_ID, Connection::CONNECTED)`, rebuilt ELF `781dac3b…` with llvm-objdump disassembly proof, repacked `vendor.img` `983131c3…` on Linux loop device with clean `e2fsck -f -n` returncode 0, repacked CPIO initramfs `201bcd05…` with embedded vendor.img hash match, and realigned DTB `3d8341f4…` byte-exactly to `0x1132b5789`; Step 3a smoke flight proved external virtio-blk `/dev/block/vda` mounts cleanly as `/system` at guest 7.41s and proved the HWC hotplug wall is 100% CURED ['Initial display configuration failed: HWC did not hotplug' dropped from 31 to 0]; Step 3b Flight-01 reached full 2400 s window [guest 2406.15s, host 2407.74s, exit 0, 51,999 lines, log SHA `13699cbe…`]; Step 3c Flight-ab canonical control proved canonical kernel `05c43917…` times out polling vda and reboots at guest 17.85s; however, after SurfaceFlinger successfully initialized display 0 via the hotplug event, it advanced to SkiaRenderEngine::primeCache() in `SurfaceFlinger::init()+3764` and aborted with `Abort message: 'output buffer not gpu writeable'`, triggering init onrestart killing Zygote before forkSystemServer(); system_server did not fork; queueBuffer 0, present 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, the HWC hotplug failure from T706 was completely cured: 0 occurrences of `Initial display configuration failed: HWC did not hotplug` across all 51,999 lines (dropped from 31 in T706 to 0). SurfaceFlinger successfully created and configured primary display 0 upon receiving `onHotplug(PRIMARY_DISPLAY_ID, CONNECTED)`. However, at line 3704 (guest 102.82s, PID 2441), `surfaceflinger` aborted inside `SkiaRenderEngine::primeCache()` with signal 6 (`Abort message: 'output buffer not gpu writeable'`). Each SurfaceFlinger abort triggered `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`, sending SIGKILL (signal 9) to Zygote before `forkSystemServer()` could be reached. As a result, `forkSystemServer()` did not complete and `sys.boot_completed` remained unset. In control flight `flight-ab`, `boot_completed=` was empty throughout execution until clean reboot on vda polling timeout at guest 17.85s.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same userspace display HAL interface, gralloc buffer usage contract (USAGE_HW_RENDER), RenderEngine drawLayers validation, and init restart cascading topology are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.087491 s (line 188: `[    3.087491] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.554858 s (line 186: `[    3.554858] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.442686 s (line 188: `[    3.442686] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 6.394083 s (line 515: `[    6.394083] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 7.209692 s (line 516: `[    7.209692] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 7.013834 s (line 515: `[    7.013834] init: init first stage started!`).

system_server:
- `system_server` did not fork in Flight-01 (0). SurfaceFlinger's abort at `primeCache()` triggered init onrestart to send SIGKILL to Zygote before `forkSystemServer()` could be invoked.
- `system_server` did not fork in Flight-ab (0). Canonical kernel failed to mount `/system` from `/dev/block/vda` due to lack of `CONFIG_VIRTIO_BLK=y` and triggered `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- Composer `executeCommands`: **0**
- `queueBuffer`: **0** (SurfaceFlinger held 0 queued client buffers).

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block devices, but could not detect `/dev/vda` due to absence of `CONFIG_VIRTIO_BLK=y` in the canonical build. At guest 17.735486 s (line 550), Android init logged: `init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`, followed by line 552 `init: InitFatalReboot: signal 6` and line 560 `reboot: Restarting system with command 'bootloader'`. QEMU cleanly rebooted and terminated under `-no-reboot` at guest 17.848 s (host 20.03 s, exit code 0, 560 lines, log SHA-256 `51431be7b8f14cea1cde193171a102bda6ff5378c62fa6f4deab64ad76bb57bb`). This conclusively re-proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was not executed because `system_server` never forked and SurfaceFlinger was restarted by init.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T707 executed the primary fix named by T706: instrumenting the Composer 2.1 HIDL vendor service to dispatch `onHotplug(PRIMARY_DISPLAY_ID, IComposerCallback::Connection::CONNECTED)` immediately upon client callback registration, and flown the full stack across a 2400 s window on Mac QEMU 11.1.2.

### Key Accomplishments
1. **Source Instrumentation & Rebuild on Linux Box (PASS)**:
   - Modified `ComposerClientImpl::registerCallback` in `/home/muse/t681-link/t681-service.cpp` on the Linux box.
   - Defined `PRIMARY_DISPLAY_ID = 0` and dispatched `callback->onHotplug(PRIMARY_DISPLAY_ID, IComposerCallback::Connection::CONNECTED)` on callback arrival.
   - Rebuilt ELF via NDK r29 / clang toolchain (`build-service.sh`). Rebuilt binary: 19,943,144 bytes, SHA-256 `781dac3bae61652d424a5f6622ac72d0801fcdcdeab0565445f28668b0eebed5`.
   - Verified symbols and disassembly via `llvm-objdump`: verified immediate dispatch to vtable entry `onHotplug` with `x1=0` (`PRIMARY_DISPLAY_ID`) and `w2=1` (`Connection::CONNECTED`).
2. **Vendor Partition Repack & DTB Alignment on Linux Box (PASS)**:
   - Extracted `ramdisk-images/vendor.img` from the clean T706 initramfs.
   - Repacked `vendor.img` on Linux using `/usr/sbin/debugfs` and `/usr/sbin/e2fsck`: replaced `/bin/hw/android.hardware.graphics.composer@2.1-service` (inode 93) with mode 0755, uid 0, gid 2000, and SELinux context `u:object_r:hal_graphics_composer_default_exec:s0`. Clean `e2fsck -f -n` verified returncode 0. New vendor.img: 134,217,728 bytes, SHA-256 `983131c300997dd7041e588d1c2fe00ea0d27dbffb3862eb8b37b8b1324b7ad2`.
   - Replaced CPIO member `ramdisk-images/vendor.img` in the fix initramfs (`compresslevel=9`, `mtime=0.0`). Rebuilt initramfs: 53,172,105 bytes, SHA-256 `201bcd058b65533ec7ad904bc79ee5b062c51855046b0e787cc81835508f0f94`.
   - Re-aligned DTB `linux,initrd-end` byte-exactly to `0x110000000 + 53172105 = 0x1132b5789` (`<0x01 0x132b5789>`). New DTB: 2,049 bytes, SHA-256 `3d8341f4ea6c49f4f4d4eeb8593c3270eedbcc27ae948b21d6e521e200994ba9`.
3. **Smoke Flight Proves External Mount + HWC Hotplug Elimination (PASS)**:
   - 120 s smoke flight verified external virtio-blk `/dev/block/vda` mounted as `/system` cleanly at guest line 524 (`__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`).
   - Freeing kernel memory, init first stage, fb0 simplefb registered. Zero kernel panic/Oops/hang.
   - **Critical Breakthrough**: The HWC hotplug wall was 100% CURED. The abort message `Initial display configuration failed: HWC did not hotplug` dropped from 31 occurrences in T706 to **0 occurrences** in T707. SurfaceFlinger successfully created and configured primary display 0 upon receiving the hotplug callback.
4. **Full Flight-01 & Canonical A/B Control Flown (PASS Execution)**:
   - Flight-01 ran full 2400 s target (host 2407.74 s, guest 2406.15 s, exit 0, 51,999 lines, SHA `13699cbe…`).
   - Flight-ab ran on canonical kernel `05c43917…` and confirmed missing `CONFIG_VIRTIO_BLK=y` triggers vda polling timeout and `InitFatalReboot` at guest 17.85 s.
5. **Exact First Failing Link Isolated (RenderEngine GPU Writable Output Buffer)**:
   - Having passed the HWC hotplug gate, SurfaceFlinger progressed deeper into `SurfaceFlinger::init()` up to line 3764: calling `SkiaRenderEngine::primeCache()`.
   - Inside `SkiaRenderEngine::drawLayersInternal` (tombstone_00 at line 3704, guest 102.82 s, PID 2441), RenderEngine asserted: `Abort message: 'output buffer not gpu writeable'` because the output render target buffer allocated for shader cache priming did not have `USAGE_HW_RENDER` preserved by gralloc.
   - SurfaceFlinger death triggered init onrestart killing Zygote before `forkSystemServer()`.

---

## Input Verification Table

All 10 staged inputs verified in `/Users/Shared/wt-track707-composer-hotplug-dispatch` prior to execution without network fetch:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t706.md` | `cb6ebb2d1f07e1c6d733dec183bcd508194f0481985671a8809a9d001804e4e8` | `cb6ebb2d1f07e1c6d733dec183bcd508194f0481985671a8809a9d001804e4e8` | 17,266 B | MATCH (PASS) |
| `findings-t705.md` | `d99361c71a23a41bff7f059794fc1ba6b06256dd61453ab61d2e6aacdd0313da` | `d99361c71a23a41bff7f059794fc1ba6b06256dd61453ab61d2e6aacdd0313da` | 17,345 B | MATCH (PASS) |
| `T706 flight-01 UART` | `3e8121d08c8e1f2176eb3d1e5274137f7019220baefb264c2730abc0367ca1d0` | `3e8121d08c8e1f2176eb3d1e5274137f7019220baefb264c2730abc0367ca1d0` | 5,735,995 B | MATCH (PASS) |
| `T706 flight-ab UART` | `6088d1dcde55d75e0ad70c077fd7bcd92ce62157309fc3127c583b83564b6dcf` | `6088d1dcde55d75e0ad70c077fd7bcd92ce62157309fc3127c583b83564b6dcf` | 46,858 B | MATCH (PASS) |
| `T706 patched system.img` | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` | 2,059,816,960 B | MATCH (PASS) |
| `T705 rebuilt Image` | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` | 20,126,208 B | MATCH (PASS) |
| `T705 fix initramfs` | `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965` | `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965` | 53,169,644 B | MATCH (PASS) |
| `T705 modified.dtb` | `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01` | `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01` | 2,049 B | MATCH (PASS) |
| `Composer ELF` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| `extracted_kernel.config` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | 133,733 B | MATCH (PASS) |

---

## Step 2 Pre-Flight Proofs Table (Rebuilt Artifacts)

| Proof # | Verification Item | Target Specification | Observed Value | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| **Proof 1** | Rebuilt Composer ELF SHA | Rebuilt on Linux box via NDK r29 | `781dac3bae61652d424a5f6622ac72d0801fcdcdeab0565445f28668b0eebed5` (19,943,144 B) | PASS |
| **Proof 2** | onHotplug Dispatch in Binary | `llvm-objdump -d --demangle` disassembly | `mov x1, xzr` (display 0), `mov w2, #1` (CONNECTED), `ldr x9, [x8, #0x68]` (onHotplug vtable call) | PASS |
| **Proof 3** | Repacked vendor.img SHA | Repacked on Linux loop device | `983131c300997dd7041e588d1c2fe00ea0d27dbffb3862eb8b37b8b1324b7ad2` (134,217,728 B) | PASS |
| **Proof 4** | vendor.img Filesystem Check | `e2fsck -f -n` read-only check | Pass 1-5 clean, returncode 0 | PASS |
| **Proof 5** | Rebuilt Fix Initramfs SHA | Replaced CPIO member vendor.img | `201bcd058b65533ec7ad904bc79ee5b062c51855046b0e787cc81835508f0f94` (53,172,105 B) | PASS |
| **Proof 6** | DTB Re-alignment SHA | Byte-exact `0x110000000 + 53172105 = 0x1132b5789` | `3d8341f4ea6c49f4f4d4eeb8593c3270eedbcc27ae948b21d6e521e200994ba9` (2,049 B) | PASS |
| **Proof 7** | Clean Base system.img SHA | Unmodified from T706 | `2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601` (2,059,816,960 B) | PASS |
| **Proof 8** | Rebuilt Route B Image SHA | Unmodified from T705 | `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859` (20,126,208 B) | PASS |

---

## Flights Execution Tables

### Step 3a: Smoke Flight (120 s Target)
- Command: QEMU 11.1.2 with `t705-Image` (`53d9563e…`), `t707-modified.dtb` (`3d8341f4…`), `t707-systemserver-fb0-fix.cpio.gz` (`201bcd05…`), `system.img` (`2de0533e…`).
- Host elapsed: 125.25 s | Guest log max: 124.544 s | QEMU exit code: 0
- Log metrics: 3,992 lines | 313,027 bytes | SHA-256 `b0ab15073b9622e1e0fe6b4fd15011e90715e1893c686e084d2260ae584220b5`
- External mount: line 524 `[    7.407992] init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
- HWC hotplug abort count: **0** (Eliminated!)

### Step 3c: Canonical A/B Control (`flight-ab`) (240 s Target)
- Command: QEMU 11.1.2 with `canonical-Image` (`05c43917…`), `t707-modified.dtb`, `t707-systemserver-fb0-fix.cpio.gz`, `system.img`.
- Host elapsed: 20.03 s | Guest log max: 17.848 s | QEMU exit code: 0
- Log metrics: 560 lines | 46,938 bytes | SHA-256 `51431be7b8f14cea1cde193171a102bda6ff5378c62fa6f4deab64ad76bb57bb`
- Polling timeout: line 550 `[   17.735486] init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`
- Fatal reboot: line 552 `[   17.760178] init: InitFatalReboot: signal 6`
- Clean exit: line 560 `[   17.848104] reboot: Restarting system with command 'bootloader'`

### Step 3b: Flight-01 (Route B Kernel, 2400 s Target)
- Command: QEMU 11.1.2 with `t705-Image` (`53d9563e…`), `t707-modified.dtb` (`3d8341f4…`), `t707-systemserver-fb0-fix.cpio.gz` (`201bcd05…`), `system.img` (`2de0533e…`).
- Host elapsed: 2407.74 s | Guest log max: 2406.15 s | QEMU exit code: 0
- Log metrics: 51,999 lines | 5,571,703 bytes | SHA-256 `13699cbebe1149b246409a4bd56e6228ebf4eb59983bc25ea9e0b06cfc72f9c9`
- External mount: line 524 `[    6.770087] init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
- HWC hotplug aborts (`Initial display configuration failed: HWC did not hotplug`): **0** (Completely Cured!)
- Zygote SIGSEGV in `art::ArtField::GetDeclaringClass`: **0**
- SurfaceFlinger new abort: line 3704 `Abort message: 'output buffer not gpu writeable'` (1×) followed by 29× `failed to create composer client`.
- Binder ioctl `40046210` (-22 EINVAL): 2,895 occurrences

---

## First Failing Link & Exact Next Fix

### First Failing Link
The first failing link is: **SurfaceFlinger aborts during `SkiaRenderEngine::primeCache()` with `Abort message: 'output buffer not gpu writeable'`, cascading into Zygote process-group SIGKILL via init onrestart**.

Evidence:
1. `flights/flight-01/uart.log` line 3680 (guest 105.35 s):
   `T637_TOMBSTONE_DETECTED name=tombstone_00 size=137538 pid: 2441, tid: 2441, name: surfaceflinger  >>> /system/bin/surfaceflinger <<< Cmdline: /system/bin/surfaceflinger`
2. `flights/flight-01/uart.log` line 3695:
   `signal 6 (SIGABRT), code -1 (SI_QUEUE), fault addr --------`
   `Abort message: 'output buffer not gpu writeable'`
3. `flights/flight-01/uart.log` lines 3708–3714 (backtrace):
   ```text
   #00 pc 00000000000669e4  /apex/com.android.runtime/lib64/bionic/libc.so (abort+164)
   #01 pc 00000000000142c0  /system/lib64/liblog.so (__android_log_default_aborter+12)
   #02 pc 0000000000014410  /system/lib64/liblog.so (__android_log_assert+308)
   #03 pc 00000000001becd8  /system/bin/surfaceflinger (SkiaRenderEngine::drawLayersInternal+14320)
   #04 pc 00000000003ea394  /system/bin/surfaceflinger (RenderEngine::drawLayers+340)
   #05 pc 000000000040552c  /system/bin/surfaceflinger (Cache::primeShaderCache+1692)
   #06 pc 0000000000400dcc  /system/bin/surfaceflinger (SkiaRenderEngine::primeCache+16)
   #07 pc 000000000037dbac  /system/bin/surfaceflinger (SurfaceFlinger::init+3764)
   ```
4. `flights/flight-01/uart.log` line 3724:
   `init: Command 'restart --only-if-running zygote' action=onrestart (<Service 'surfaceflinger' onrestart>:1) took 276ms and succeeded`

### Mechanism & Root Cause
In Android 14 `frameworks/native/libs/renderengine/skia/SkiaRenderEngine.cpp`:
```cpp
void SkiaRenderEngine::drawLayersInternal(...) {
    const auto& buffer = dstTexture->getBuffer();
    LOG_ALWAYS_FATAL_IF(!(buffer->getUsage() & GraphicBuffer::USAGE_HW_RENDER),
                        "output buffer not gpu writeable");
    ...
}
```
During `SurfaceFlinger::init()`, after successfully hotplugging and configuring the primary display, SurfaceFlinger calls `mRenderEngine->primeCache()` (line 3764). SkiaRenderEngine attempts to pre-warm the Skia shader pipeline by rendering dummy layers to an offscreen target `GraphicBuffer`. However, the virtual display / goldfish gralloc allocator (`android.hardware.graphics.allocator@2.0-service`) does not return `GraphicBuffer::USAGE_HW_RENDER` in the allocated buffer's usage flags (or gralloc strips the flag in software/simplefb mode). As a result, the assertion `!(buffer->getUsage() & GraphicBuffer::USAGE_HW_RENDER)` triggers `LOG_ALWAYS_FATAL("output buffer not gpu writeable")`.

Furthermore, when SurfaceFlinger aborts and init restarts it, `ComposerImpl::createClient` in `t681-service.cpp` returns `Error::NO_RESOURCES` because `mClient` was never cleared on client death, causing all subsequent restarts to abort with `failed to create composer client`.

### Exact Next Fix for Track T708
1. **Disable SurfaceFlinger Shader Cache Priming (Primary)**:
   In `init.virt.rc` (or `system/build.prop`), set the system property `service.sf.prime_shader_cache=0` (or `debug.renderengine.skia.cache.disable=1` / `ro.surface_flinger.prime_shader_cache=false`). In Android 14 `SurfaceFlinger::init()`, `mPrimeShaderCache` is guarded by `property_get_bool("service.sf.prime_shader_cache", true)`. Setting it to `0` skips `mRenderEngine->primeCache()` entirely, allowing `SurfaceFlinger::init()` to complete cleanly and enter the main message pump without aborting.
2. **Support Reconnect in ComposerImpl::createClient (Secondary)**:
   In `t681-service.cpp`, update `ComposerImpl::createClient`:
   Allow overwriting `mClient` or implement `linkToDeath` cleanup, ensuring that if SurfaceFlinger ever restarts, it can reconnect to Composer 2.1 without hitting `Error::NO_RESOURCES`.
