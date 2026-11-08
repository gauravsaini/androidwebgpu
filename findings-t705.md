# Findings — Track T705: Rebuild Route B Kernel with CONFIG_VIRTIO_BLK=y, Mount /dev/vda, Drop Embedded Image, Fly for First Queued Buffer + Present Dispatch (Mac agyx)

Date: 2026-11-09  
Worktree: `/Users/Shared/wt-track705-virtio-blk-kernel-rebuild`  
Branch: `t705`, from `2d6c6b2` (the T704 commit)  
Predecessor: T704 driver-certified honest FAIL (commit 2d6c6b2: embedded-image bypass solved with patched CPIO, but ext4 shared_blocks dedup corruption identified on services.jar from Homebrew debugfs write, exact next fix named).

## Verdict

Verdict: FAIL

(Honest FAIL: Step 0 Mac disk gate passed with 13 GiB available; Step 1 verified all 9 staged inputs and proved the VIRTIO_BLK wall in the Route B kernel; Step 2 successfully rebuilt the Route B kernel with CONFIG_VIRTIO_BLK=y and CONFIG_VIRTIO_MMIO=y on the box passing symbol and initcall gates, rebuilt the fix initramfs dropping the 905 MB embedded ramdisk-images/system.img, and re-aligned DTB byte-exactly to 0x1132b4dec with virtio_mmio@a000000; Step 3a smoke flight proved external virtio-blk /dev/block/vda mounts cleanly as /system at guest 7.41s; Step 3b Flight-01 reached full 2400 s window [guest 2402.067 s, host 2402.76 s, exit 0, 45,314 lines, log SHA b255056b…]; Step 3c Flight-ab canonical control proved canonical kernel 05c43917… times out polling vda and aborts at guest 17.75s; however, external system.img afc0add0… inherited from T703 was originally patched in T702 via macOS Homebrew debugfs write, retaining corrupted block 145414 in services.jar, while SurfaceFlinger aborted with 'Initial display configuration failed: HWC did not hotplug' due to vendor.hwc-2-1 binder ioctl -22 EINVAL; Zygote repeated SIGSEGV in art::ArtField::GetDeclaringClass; system_server did not fork; queueBuffer 0, present 0).

## Required markers

sys.boot_completed: not observed (0) — `sys.boot_completed=1` was NOT reached in flight `flight-01` (`flights/flight-01/uart.log`). In Flight-01, Zygote (PID 2366, 2491, 2687, 3030...) crashed repeatedly with `SIGSEGV` code 1 (SEGV_MAPERR, fault addr `0x0000000000000000`) in `art::ArtField::GetDeclaringClass` inside `art::ClassLinker::InitializeClass` called from `boot-framework.oat (com.android.internal.os.ZygoteInit.main+2164)`. In parallel, `surfaceflinger` (PID 2445, 3733, 4161...) aborted with signal 6 (`Abort message: 'Initial display configuration failed: HWC did not hotplug'`) because `vendor.hwc-2-1` received `-22` (EINVAL) on binder ioctl and did not register or hotplug a display with SurfaceFlinger. Each SurfaceFlinger death triggered `restart --only-if-running zygote`. As a result, `forkSystemServer()` did not complete and `sys.boot_completed` remained unset. In control flight `flight-ab`, `boot_completed=` was empty throughout its execution until QEMU clean exit on mount abort at guest 17.86 s.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest userspace boot, initramfs mount topology, virtio-blk kernel driver requirement, ext4 shared_blocks deduplication, ART execution, and display pipeline paths are required regardless of implementation language.

Freeing unused kernel memory:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.454186 s (line 188: `[    3.454186] Freeing unused kernel memory: 960K`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 3.522480 s (line 186: `[    3.522480] Freeing unused kernel memory: 1024K`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 3.470378 s (line 180: `[    3.470378] Freeing unused kernel memory: 960K`).

init first stage:
- Flight `flight-01` (Rebuilt Route B kernel `53d9563e…`): observed at guest 7.018582 s (line 515: `[    7.018582] init: init first stage started!`).
- Flight `flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 7.219286 s (line 517: `[    7.219286] init: init first stage started!`).
- Smoke flight (Rebuilt Route B kernel `53d9563e…`): observed at guest 7.051126 s (line 512: `[    7.051126] init: init first stage started!`).

system_server:
- `system_server` did not fork in Flight-01 (0). Zygote crashed on `art::ArtField::GetDeclaringClass` before `forkSystemServer()` could be reached.
- `system_server` did not fork in Flight-ab (0). Init failed to mount `/system` from `/dev/block/vda` and triggered `InitFatalReboot`.

SurfaceFlinger present dispatch:
- `presentDisplay`: **0**
- `validateDisplay`: **0**
- `presentOrValidate`: **0**
- Composer `executeCommands`: **0**
- `queueBuffer`: **0** (SurfaceFlinger held 0 queued client buffers).

Canonical kernel control (`flight-ab`):
- Canonical kernel `05c43917…` booted to `/early-ramdisk`, created block devices, but could not detect `/dev/vda` due to lack of `CONFIG_VIRTIO_BLK` in the canonical kernel build. At guest 17.750656 s (line 551), Android init logged: `init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`, followed by line 552 `init: Failed to mount required partitions early ...` and line 553 `init: InitFatalReboot: signal 6`. QEMU cleanly rebooted and terminated under `-no-reboot` at guest 17.864 s (host 20.03 s, exit code 0, 561 lines, log SHA-256 `3d889b26ee6ae81b1d7cca42b7186dcc385e15d5fcd38828ad884cf74623c259`). This conclusively proves that `CONFIG_VIRTIO_BLK=y` in the rebuilt Route B kernel is the sole enabler of the external image mount.

Count of FATAL EXCEPTION for `com.android.launcher3`: **0** (Target: 0).
- Launcher3 was never launched because `system_server` never forked and SurfaceFlinger remained in an abort loop.

TEN-PASSES: NOT run in this lane — recorded honestly; no 10/10 claim.  
Snapshot byte differences: Stated explicitly that snapshot byte differences were NOT used as frame-commit proof (T673 standing rule).

---

## Executive Summary

Track T705 resolved the architectural blocker identified in T704: the Route B kernel had `# CONFIG_VIRTIO_BLK is not set`, forcing the system to mount the embedded `ramdisk-images/system.img`. T705 rebuilt the Route B kernel on the box with `CONFIG_VIRTIO_BLK=y` and `CONFIG_VIRTIO_MMIO=y`, dropped the 905 MB embedded image from the initramfs, aligned the DTB byte-exactly, and proved the external virtio-blk mount of `afc0add0…` at `/dev/block/vda`.

### Key Accomplishments
1. **Route B Kernel Rebuild with VirtIO-Blk & VirtIO-MMIO (PASS)**:
   - Configured `CONFIG_VIRTIO_BLK=y` and `CONFIG_VIRTIO_MMIO=y` in `/home/muse/t686-kernel/src`.
   - Rebuilt kernel with Route B non-LTO + linker-script globs (`include/asm-generic/vmlinux.lds.h`).
   - Passed Gate 3a Symbol Gate: `_etext` at `0xffffff8008b50000`, critical symbols (`printk`, `__die`, `panic`, `dump_stack`, `show_regs`, `free_initmem`) strictly below `_etext`.
   - Passed Gate 3b Initcall Gate: `__initcall_start` (`0xffffff8009250b48`) < `__initcall_end` (`0xffffff8009251664`), exactly 0 suffixed sections.
   - Synchronized to Mac: `guest-inputs/t705-Image` (20,126,208 B, SHA-256 `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859`).
2. **Initramfs Rebuild Without Embedded Image (PASS)**:
   - Removed `ramdisk-images/system.img` from CPIO archive, dropping archive size from 958,780,579 B to 53,169,644 B.
   - Modified `fstab.virt` and `system/etc/fstab.virt` to point `/system` to `/dev/block/vda`.
   - Output: `guest-inputs/t705-systemserver-fb0-fix.cpio.gz` (SHA-256 `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965`).
3. **DTB Realignment & VirtIO MMIO Transport (PASS)**:
   - Calculated `linux,initrd-end`: `0x110000000 + 53169644 = 0x1132b4dec` (`<0x01 0x132b4dec>`).
   - Injected missing QEMU virtio-mmio node: `virtio_mmio@a000000` at DT root with `reg = <0x00 0xa000000 0x00 0x200>` and `interrupts = <0x00 0x10 0x01>`.
   - Output: `guest-inputs/t705-modified.dtb` (2,049 B, SHA-256 `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01`).
4. **Smoke Flight & External Mount Verification (PASS)**:
   - 120 s smoke flight verified virtio-blk probe: line 127 `virtio_blk virtio0: [vda] 4023080 512-byte logical blocks (2.06 GB/1.92 GiB)`.
   - First-stage init mounted `/dev/block/vda` directly to `/system`: line 524 `init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`.
5. **Full Flight-01 & Canonical Flight-ab Flown (PASS Execution)**:
   - Flight-01 ran for 2402.76 host seconds (guest 2402.067 s), producing 45,314 lines of preserved UART evidence.
   - Flight-ab ran on canonical kernel `05c43917…` and proved that without `CONFIG_VIRTIO_BLK=y`, init times out and fails to boot.

---

## Input Verification Table

All inputs verified in `/Users/Shared/wt-track705-virtio-blk-kernel-rebuild` prior to execution without network fetch:

| Input File | Expected SHA-256 | Actual Verified SHA-256 | Size (Bytes) | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| `findings-t704.md` | `c572f0dfe9f0dd02e164952d4402ddb27cf20a4c1fb7a4b24dd5fec6f3fb6d34` | `c572f0dfe9f0dd02e164952d4402ddb27cf20a4c1fb7a4b24dd5fec6f3fb6d34` | 19,065 B | MATCH (PASS) |
| `findings-t703.md` | `6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583` | `6e6582ab94b9d8a76b669cce3ff0682654e69aa77d7d9a047b853b31c0aaa583` | 22,137 B | MATCH (PASS) |
| `findings-t696.md` | `7184046210a2b439bf57c49305aa3558a119e4f1bdec1f8bb8beda58785c5102` | `7184046210a2b439bf57c49305aa3558a119e4f1bdec1f8bb8beda58785c5102` | 20,656 B | MATCH (PASS) |
| `system.img` | `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7` | `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7` | 2,059,816,960 B | MATCH (PASS) |
| `routeB-Image` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` | 20,126,208 B | MATCH (PASS) |
| `t699-systemserver-fb0-fix.cpio.gz` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b` | 958,780,579 B | MATCH (PASS) |
| `modified.dtb` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` | 1,920 B | MATCH (PASS) |
| `composer` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B | MATCH (PASS) |
| `extracted_kernel.config` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` | 133,733 B | MATCH (PASS) |

---

## Step 2 Rebuilt Artifacts & Pre-Flight Proofs Table

| Proof # | Verification Item | Target Specification | Observed Value | Verdict |
| :--- | :--- | :--- | :--- | :--- |
| **Proof 1** | Rebuilt Route B Kernel Image | `CONFIG_VIRTIO_BLK=y`, `CONFIG_VIRTIO_MMIO=y`, symbol + initcall gates pass | SHA-256 `53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859`, 20,126,208 B | PASS |
| **Proof 2** | Kernel Symbol Gate | printk, __die, panic, dump_stack, show_regs < _etext (0xffffff8008b50000) | All 5 symbols < _etext (max: 0xffffff800827f8a8) | PASS |
| **Proof 3** | Kernel Initcall Gate | `__initcall_start < __initcall_end`, 0 suffixed sections | `0xffffff8009250b48 < 0xffffff8009251664`, 0 suffixed | PASS |
| **Proof 4** | Rebuilt Initramfs CPIO | `ramdisk-images/system.img` member removed entirely | 236 members, 0 system.img members, size: 53,169,644 B, SHA-256 `583147fd4e8d0fd6fb6baee11eff41b95fd551a4477c825ac56d9c9450beb965` | PASS |
| **Proof 5** | DTB Byte-Exact Alignment | `linux,initrd-end = 0x110000000 + 53169644 = 0x1132b4dec` | `linux,initrd-end = <0x01 0x132b4dec>;`, SHA-256 `0205a94aeed15bba06c2ad0a8fea0a310fe40d87a9c26ee054b6c37074339e01` | PASS |
| **Proof 6** | DTB VirtIO-MMIO Device | `virtio_mmio@a000000` node present at DT root | Injected with reg `<0x00 0xa000000 0x00 0x200>` and interrupts `<0x00 0x10 0x01>` | PASS |

---

## Flights Execution Tables

### Step 3a: Smoke Flight (120 s Target)
- Command: QEMU 11.1.2 with `t705-Image`, `t705-modified.dtb`, `t705-systemserver-fb0-fix.cpio.gz`, `system.img` (virtio-blk).
- Host elapsed: 125.21 s | Guest log max: 124.604 s | QEMU exit code: 0
- Log metrics: 4,128 lines | 323,042 bytes | SHA-256 `c3719a792db04271850632df35338b5cd48ce09183fd2437088b78a2c70a4cf2`
- Mount observation: line 524 `[    7.448726] init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
- Virtio probe: line 127 `[    3.271804] virtio_blk virtio0: [vda] 4023080 512-byte logical blocks (2.06 GB/1.92 GiB)`

### Step 3c: Canonical A/B Control (`flight-ab`) (240 s Target)
- Command: QEMU 11.1.2 with `canonical-Image` (`05c43917…`), `t705-modified.dtb`, `t705-systemserver-fb0-fix.cpio.gz`, `system.img`.
- Host elapsed: 20.03 s | Guest log max: 17.864 s | QEMU exit code: 0
- Log metrics: 561 lines | 47,006 bytes | SHA-256 `3d889b26ee6ae81b1d7cca42b7186dcc385e15d5fcd38828ad884cf74623c259`
- Polling timeout: line 551 `[   17.750656] init: bool android::init::BlockDevInitializer::InitDevices(std::set<std::string>): partition(s) not found after polling timeout: vda`
- Fatal reboot: line 553 `[   17.775180] init: InitFatalReboot: signal 6`
- Clean exit: line 559 `[   17.863788] reboot: Restarting system with command 'bootloader'`

### Step 3b: Flight-01 (Route B Kernel, 2400 s Target)
- Command: QEMU 11.1.2 with `t705-Image` (`53d9563e…`), `t705-modified.dtb`, `t705-systemserver-fb0-fix.cpio.gz`, `system.img`.
- Host elapsed: 2402.76 s | Guest log max: 2402.067 s | QEMU exit code: 0
- Log metrics: 45,314 lines | 5,091,977 bytes | SHA-256 `b255056bfb607bcf1bddf4d8298600406365b1a68a884e48db9e030d58454a58`
- External mount: line 524 `[    7.411158] init: [libfs_mgr] __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
- SurfaceFlinger abort message: `Initial display configuration failed: HWC did not hotplug` (tombstone_01, line 3656)
- Zygote SIGSEGV: `art::ArtField::GetDeclaringClass` (tombstone_00, line 3640)

---

## Detailed Root Cause Analysis: The Inherited Shared-Block & HWC Walls

Although Track T705 cleanly resolved the kernel driver and initramfs mount topology issues, Flight-01 did not achieve `sys.boot_completed=1` or SurfaceFlinger present dispatch due to two concrete, independent defects:

### 1. Inherited Shared-Block Corruption in External `system.img` (`afc0add0…`)
- **Origin**: T702 patched `Launcher3QuickStep.apk` using macOS Homebrew `debugfs write`. T703 took that exact patched image and deleted the `oat/arm64` directory, producing `afc0add0…`.
- **Mechanism**: Android's read-only ext4 filesystem uses `shared_blocks` deduplication. Block 145414 is shared between `system/framework/services.jar` and `Launcher3QuickStep.apk`. Homebrew's `debugfs write` in T702 clobbered block 145414 with string table data (`ase__packages__SystemUI__common__android_common__SystemUICommon.`).
- **Confirmation**: Inspection of block 145414 in `guest-inputs/system.img` confirmed SHA-256 `4e243eb63ce1d74e1781c44a8f19e3fc1f02038285f87275a1d149ec9bab9b0b` containing the corrupted string table instead of the original fill pattern `4ea21150…`.
- **Impact on ART**: Inside `boot-framework.oat`, when `ZygoteInit.main+2164` attempts to resolve static fields during class initialization, it accesses corrupt field metadata originating from the corrupted classes, resulting in `field == nullptr` and crashing in `art::ArtField::GetDeclaringClass` with null pointer dereference (`x0 == 0x0`, fault addr `0x0`).

### 2. SurfaceFlinger HWC Hotplug Abort Loop
- **Observation**: SurfaceFlinger started at guest ~81 s, but failed to initialize:
  `Abort message: 'Initial display configuration failed: HWC did not hotplug'`
  `[ 109.329876] init: Service 'surfaceflinger' (pid 2485) received signal 6`
  `[ 334.660032] init: process with updatable components 'surfaceflinger' exited 4 times before boot completed`
- **Root Cause**: `vendor.hwc-2-1` (PID 2369) was started by init, but its call to binder ioctl `40046210` returned `-22` (`EINVAL`) at guest 83.121946 s (line 3540). It failed to communicate with the display HAL or trigger a display hotplug event to SurfaceFlinger.
- **Cascading Effect**: Because SurfaceFlinger crashed, Android `init` repeatedly triggered its `onrestart` action: `<Service 'surfaceflinger' onrestart>: restart --only-if-running zygote`.

---

## Exact Next Fix (Track T706)

1. **Clean GSI `system.img` with Linux VFS Loop Patching (Primary)**:
   - Discard the corrupted `afc0add0…` image.
   - Obtain the clean, uncorrupted base `system.img` where block 145414 of `services.jar` retains its original contents (`4ea21150…`).
   - Patch `Launcher3QuickStep.apk` (or remove `oat/arm64`) **EXCLUSIVELY on Linux** via loopback mount (`mount -o loop system.img /mnt/system`). The Linux kernel's ext4 driver natively unshares deduplicated extents upon write via copy-on-write, preserving `services.jar` intact.
2. **HWC Hotplug & Binder -22 Resolution**:
   - Investigate why `vendor.hwc-2-1` receives `EINVAL` on binder ioctl `40046210` in the Route B environment.
   - Ensure the graphics composer service binds to `/dev/binder` and successfully issues the initial display hotplug callback to SurfaceFlinger.
