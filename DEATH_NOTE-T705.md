# DEATH NOTE — T705

Lane: Track T705 — Rebuild the Route B kernel with CONFIG_VIRTIO_BLK=y, mount /dev/vda, drop the embedded image, fly for the first queued buffer + first present dispatch (Mac agyx)
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Step 0 Mac disk gate passed with 13 GiB available; Step 1 verified all 9 staged inputs and proved the VIRTIO_BLK wall in Route B kernel; Step 2 successfully rebuilt Route B kernel with CONFIG_VIRTIO_BLK=y and CONFIG_VIRTIO_MMIO=y on box passing symbol and initcall gates, rebuilt fix initramfs dropping the 905 MB embedded ramdisk-images/system.img, and re-aligned DTB byte-exactly to 0x1132b4dec with virtio_mmio@a000000; Step 3a smoke flight proved external virtio-blk /dev/block/vda mounts cleanly as /system at guest 7.41s; Step 3b Flight-01 reached full 2400 s window [guest 2402.067 s, host 2402.76 s, exit 0, 45,314 lines, log SHA b255056b…]; Step 3c Flight-ab canonical control proved canonical kernel 05c43917… times out polling vda and aborts at guest 17.75s; however, external system.img afc0add0… inherited from T703 was originally patched in T702 via macOS Homebrew debugfs write, retaining corrupted block 145414 in services.jar, while SurfaceFlinger aborted with 'Initial display configuration failed: HWC did not hotplug' due to vendor.hwc-2-1 binder ioctl -22 EINVAL; Zygote repeated SIGSEGV in art::ArtField::GetDeclaringClass; system_server did not fork; queueBuffer 0, present 0)
UTC close: 2026-11-08T15:30:00Z
Worktree: /Users/Shared/wt-track705-virtio-blk-kernel-rebuild
Branch: t705 (from T704 commit 2d6c6b2)

## What ended

Track T705 was opened to resolve the core architectural blocker identified in Track T704:
In T704, the Route B kernel had `# CONFIG_VIRTIO_BLK is not set`, forcing early-ramdisk to fall back to the embedded `ramdisk-images/system.img`. T704 proved that patching that embedded image with macOS Homebrew `debugfs write` corrupted block 145414 shared with `services.jar` via ext4 `shared_blocks` deduplication.

Track T705 rebuilt the Route B kernel with `CONFIG_VIRTIO_BLK=y` on the box, rebuilt the fix initramfs without any embedded system image, aligned the DTB byte-exactly, and proved the external virtio-blk mount of `afc0add0…` at `/dev/block/vda`:

1. **Step 0 — Mac Disk Gate: PASS**:
   - `df -h /System/Volumes/Data` reported 13 GiB available (13,170 MB), strictly satisfying the $\ge$ 8 GiB threshold. No WAIT-DISK.
2. **Step 1 — Input Verification & Wall Proof: PASS**:
   - All 9 staged inputs SHA-verified against specification without network fetch.
   - Verified that `CONFIG_VIRTIO_BLK` is `# CONFIG_VIRTIO_BLK is not set` in `extracted_kernel.config` (SHA `d840310a…`) and in box `.config`.
   - Box tree `/home/muse/t686-kernel/src` at HEAD `4da740c10` with Route B linker script globs intact.
3. **Step 2 — Kernel Rebuild & Initramfs Rebuild: PASS**:
   - Rebuilt Route B kernel on box with `CONFIG_VIRTIO_BLK=y` and `CONFIG_VIRTIO_MMIO=y`.
   - Gate 3a Symbol Gate passed: all 5 critical symbols strictly below `_etext` (`0xffffff8008b50000`).
   - Gate 3b Initcall Gate passed: `__initcall_start` < `__initcall_end`, 0 suffixed sections.
   - Synchronized Image to Mac: `guest-inputs/t705-Image` (SHA-256 `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859`, 20,126,208 B).
   - Rebuilt CPIO without embedded `system.img`: size dropped from 958,780,579 B to 53,169,644 B (SHA-256 `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965`).
   - Re-aligned DTB `linux,initrd-end` to `<0x01 0x132b4dec>` (`0x110000000 + 53169644 = 0x1132b4dec`) and added `virtio_mmio@a000000` node (SHA-256 `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01`).
4. **Step 3a — 120 s Smoke Flight: PASS**:
   - Flown on Mac QEMU 11.1.2: host 125.21 s, guest 124.604 s, exit code 0, 4,128 lines, log SHA-256 `c3719a79…`.
   - VirtIO blk probe proven at guest 3.27s (line 127).
   - First-stage init mounted `/dev/block/vda` as `/system` at guest 7.44s (line 524).
5. **Step 3c — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…`: host 20.03 s, guest 17.864 s, exit code 0, 561 lines, log SHA-256 `3d889b26…`.
   - Proved canonical missing `vda` failure: line 551 `partition(s) not found after polling timeout: vda`, triggering `InitFatalReboot` and reboot.
6. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on rebuilt Route B kernel `53d9563e…`: host 2402.76 s, guest 2402.067 s, exit code 0, 45,314 lines, log SHA-256 `b255056b…`.
   - External `/dev/block/vda` mounted cleanly as `/system` at guest 7.41s (line 524).

## Why the verdict is FAIL

Under the proving criteria of Track T705:
1. **`sys.boot_completed=1` Not Observed**:
   - `sys.boot_completed` remained unset throughout the 2400 s window.
2. **`system_server` Did Not Fork**:
   - Zygote crashed repeatedly with `SIGSEGV` (SEGV_MAPERR, fault addr `0x0`) in `art::ArtField::GetDeclaringClass` inside `art::ClassLinker::InitializeClass` called from `boot-framework.oat (com.android.internal.os.ZygoteInit.main+2164)`.
3. **SurfaceFlinger Aborted in Hotplug Loop**:
   - SurfaceFlinger failed initialization: `Abort message: 'Initial display configuration failed: HWC did not hotplug'`.
   - `vendor.hwc-2-1` received `-22` (`EINVAL`) on binder ioctl `40046210` and never hotplugged a display.
   - Init repeatedly restarted Zygote whenever SurfaceFlinger died.
4. **SurfaceFlinger Held 0 Queued Buffers & 0 Present Dispatch**:
   - `queueBuffer`: 0
   - `presentDisplay`: 0, `validateDisplay`: 0, `presentOrValidate`: 0, `executeCommands`: 0.

## Inherited Root Cause & Exact Next Fix (Track T706)

1. **Inherited Shared-Block Corruption in External `system.img`**:
   - External `system.img` (`afc0add0…`) was produced in T703 from T702's image, which was patched using macOS Homebrew `debugfs write`.
   - In Android read-only ext4, block 145414 is shared between `system/framework/services.jar` and `Launcher3QuickStep.apk`.
   - The T702 `debugfs write` overwritten block 145414 in place with string table data, corrupting `classes.dex` in `services.jar` before T705 even launched.
2. **Exact Next Fix**:
   - **T706 Primary**: Take a completely clean GSI baseline `system.img` where block 145414 is uncorrupted, and patch `Launcher3QuickStep.apk` / remove `oat` **EXCLUSIVELY on Linux** via a true loopback mount (`mount -o loop system.img /mnt/system`). The Linux ext4 VFS driver natively breaks shared deduplicated blocks upon write, preserving `services.jar` intact.
   - **T706 Display**: Resolve the binder ioctl `-22` (`EINVAL`) error on `vendor.hwc-2-1` so that HWC successfully issues the primary display hotplug event to SurfaceFlinger.
