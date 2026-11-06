Verdict: FAIL

First failing link: the canonical guest kernel has `# CONFIG_DEVMEM is not set` and `# CONFIG_FB is not set`. The T681 Composer's SimpleFB path opens `/dev/mem`, while no framebuffer driver can register a real `/dev/fb0`. A DTB-only framebuffer node cannot make either kernel path available. The service's `0x5F000000` default is ordinary, unreserved guest RAM, not a QEMU scanout aperture.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface is required regardless of implementation language.
sys.boot_completed: not observed in T685 (no flight); reference T683 cycle 1 had a blank `boot_completed=` field at UART line 49952, guest uptime 1175.91s.

# Track T685 — DTB framebuffer-node audit

## Step 0 — Mac disk gate

`df -h /System/Volumes/Data` reported 17Gi available (228Gi total, 182Gi used). The gate passed the 8Gi minimum.

## Step 1 — Input verification

All seven staged input SHA-256 values matched the brief; actual byte counts are included below.

| Input | Bytes | SHA-256 |
|---|---:|---|
| `guest-inputs/Image` | 23,073,280 | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` |
| `guest-inputs/minimal-virt-fixed.dtb` | 1,465 | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` |
| `guest-inputs/system.img` | 2,059,816,960 | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` |
| `guest-inputs/base-t683-fix.cpio.gz` | 958,780,579 | `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70` |
| `guest-inputs/vendor-t683.img` | 134,217,728 | `bed3d6e989e16c8bbd7dd7e0208753c3a0d8e3407c96d291cd778ff43a7daaf1` |
| `guest-inputs/android.hardware.graphics.composer@2.1-service` | 19,941,040 | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` |
| `guest-inputs/extracted_kernel.config` | 133,733 | `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` |

The gzip CPIO stream listed 244 entries. `ramdisk-images/vendor.img` is 134,217,728 bytes. Streaming that member from the archive produced SHA-256 `bed3d6e989e16c8bbd7dd7e0208753c3a0d8e3407c96d291cd778ff43a7daaf1`, identical to the separately staged T683 vendor image. No full initramfs unpack was done.

The read-only T683 reference UART also matched its pinned SHA-256 `73cfd8d61e6c992b913e3c83975a314ea0f39cd3d41c505464d70aa55b5dbe70`.

## Step 2 — DTB / kernel framebuffer audit

The audit uses the verified canonical Image and DTB from the T670/T683 lineage. T683's captured QEMU command passes these as `-kernel .../guest-inputs/Image` and `-dtb .../guest-inputs/minimal-virt-fixed.dtb`; its boot arguments carry the same pinned T670/T683 initramfs hashes. The CPIO used here is the T683 fix image with the T681 Composer ELF installed.

1. **DTB nodes and framebuffer node — absent.** `dtc -I dtb -O dts` and `fdtdump -s` decoded the 1,465-byte blob. The header gives total size `0x5b9`, structure block offset `0x38`/size `0x460`, and strings block offset `0x498`/size `0x121`. The root `compatible` value `arm,virt` is at data offset `0x4c`. The full node set is `/`, `/chosen`, `/aliases`, `/memory@0`, `/cpus`, `/cpus/cpu@0`, `/psci`, `/apb-pclk`, `/pl011@9000000`, `/intc@8000000`, and `/timer`. The compatible strings are: `/` = `arm,virt`; `/cpus/cpu@0` = `arm,cortex-a53`; `/psci` = `arm,psci-1.0`; `/apb-pclk` = `fixed-clock`; `/pl011@9000000` = `arm,pl011`, `arm,primecell`; `/intc@8000000` = `arm,cortex-a15-gic`; `/timer` = `arm,armv8-timer`, `arm,armv7-timer`. `/chosen`, `/aliases`, `/memory@0`, and `/cpus` have no `compatible` property. No node named `framebuffer`, compatible `simple-framebuffer`, or `/reserved-memory` exists.

   `/memory@0` begins at structure offset `0x144`; its `reg` cells start at data offset `0x174`. `fdtget -t x ... /memory@0 reg` returns `0 40000000 0 40000000`: base `0x40000000`, size `0x40000000` (1GiB), ending at `0x80000000` exclusive.

2. **Kernel framebuffer/DRM consumers — unavailable.** In `guest-inputs/extracted_kernel.config`, line 1927 is `# CONFIG_DEVMEM is not set`; line 2734 is `# CONFIG_DRM_FBDEV_EMULATION is not set`; line 2770 is `# CONFIG_DRM_VIRTIO_GPU is not set`; and line 2830 is `# CONFIG_FB is not set`. `CONFIG_FB_SIMPLE`, `CONFIG_DRM_SIMPLEDRM`, and `CONFIG_FB_EFI` have no enabled config entry (the framebuffer core is disabled). `CONFIG_DRM=y` at line 2728 enables generic DRM core, not a display controller. Other relevant display drivers are explicitly disabled, including `CONFIG_DRM_BOCHS` (2769), `CONFIG_DRM_CIRRUS_QEMU` (2765), `CONFIG_DRM_QXL` (2768), `CONFIG_DRM_PL111` (2822), `CONFIG_DRM_ARCPGU` (2817), `CONFIG_DRM_HDLCD` (2747), and `CONFIG_DRM_MXSFB` (2820). Panel/bridge helpers are not a framebuffer controller.

   A `strings -a -t x` scan of the verified ARM64 Image found generic DRM-core strings such as `drivers/gpu/drm/drm_crtc.c` at `0x119f60c` and `drivers/gpu/drm/drm_framebuffer.c` at `0x119f627`; the only targeted framebuffer-related hit was `efi-framebuffer` at `0x11d1747`. There were no `simplefb`, `simple-framebuffer`, `simpledrm`, `virtio_gpu`, `virtio-gpu`, or fbdev-driver strings. The config and Image agree: this kernel has DRM core code but no framebuffer core or usable framebuffer/display driver. The `efi-framebuffer` identifier does not supply an EFI display in this direct `-kernel` flight, and `CONFIG_FB_EFI` is not enabled.

3. **T681 address `0x5F000000` — a service default in ordinary RAM, not a display aperture.** T681's service description records `PRESENT_DISPLAY` dispatch to `0x5F000000`, with defaults 1280×720, stride 1280 pixels, 32 bpp, and environment overrides `T681_FB_WIDTH/HEIGHT/STRIDE/BPP` and `T681_FB_PHYS` (`findings-t681.md`, lines 208–210). The ELF contains `/dev/mem` and the diagnostic strings `simplefb: open /dev/mem failed: %s` (ELF string offset `0x14240`) and `simplefb: mapped 0x%lx %ux%u stride=%u bpp=%u` (offset `0x15eb5`).

   The address is **inside** the DTB's general RAM interval: offset `0x1f000000` from `0x40000000`. At the service defaults, one 32-bit frame is `1280 × 720 × 4 = 0x384000` bytes, ending at `0x5f384000`, also inside that interval. This is not an unbacked physical RAM address, but neither DTB nor QEMU marks it as framebuffer memory: there is no reserved-memory carveout, framebuffer node, or device MMIO region there. It remains ordinary RAM and is not proven safe from kernel/page allocation. The fixed kernel also has `CONFIG_DEVMEM` disabled, so the service's `/dev/mem` route has no kernel device to open. The T683 QEMU command creates no scanout device at that address.

4. **`/dev/fb0` — truncation creates only a tmpfs regular file.** In the verified T683 UART, line 1140 shows init running `/system/bin/toybox truncate -s 16M /dev/fb0`; line 1145 ties it to `/vendor/etc/init/hw/init.virt.rc:4` and says the command succeeded. Line 1158 records `path="/dev/fb0" dev="tmpfs"`; line 41186 records the allocator's `{ open }` denial on that same `dev="tmpfs"` path. These are guest-origin observations of a regular tmpfs placeholder, not a kernel framebuffer device. Removing the truncate would leave no `/dev/fb0`: `CONFIG_FB` is off and no framebuffer driver can register the fbdev device. The truncate is not the only missing link.

5. **Alternative paths — none viable with this canonical kernel and flight.** QEMU 11.1.2's device inventory offers `virtio-gpu-pci`, `virtio-gpu-device`, `ramfb`, `bochs-display`, and VGA devices. The T683 launch command uses `-machine virt,...`, `-device virtio-blk-device,...`, and `-display none`; it has no GPU, ramfb, VGA, or firmware device. Adding `virtio-gpu-pci` or `virtio-gpu-device` is blocked by `# CONFIG_DRM_VIRTIO_GPU is not set`; DRM fbdev emulation is also off. `ramfb` has no consumer in this kernel (`FB_SIMPLE`/`DRM_SIMPLEDRM`/`FB_EFI` are unavailable) and is not instantiated in the flight. VGA/bochs/Cirrus alternatives are blocked by disabled `CONFIG_FB`, `CONFIG_DRM_BOCHS`, and `CONFIG_DRM_CIRRUS_QEMU`. No display alternative is both delivered by the captured QEMU command and consumable by the canonical kernel.

## Step 3 — Fix decision

**FAIL — no concrete fix exists for the required canonical kernel and flight.** Adding a SimpleFB DTB node, even at the service's default address, cannot supply `CONFIG_DEVMEM`, `CONFIG_FB`, or a SimpleFB driver. The candidate address is not reserved and is only ordinary RAM; no QEMU scanout device uses it. A QEMU GPU addition is likewise unusable because the matching guest driver is absent. Therefore no fixed DTB, new initramfs, or service rebuild was produced, and no identical image was re-flown. The exact first failing link is the kernel-to-framebuffer access layer (`CONFIG_DEVMEM` and `CONFIG_FB` both unset), before a compositor write can reach a real framebuffer.

## Step 4 — QEMU flight

Skipped as required by Step 3: no fix was built, so no T685 QEMU flight ran. No T685 host/guest elapsed time, UART, logcat, or QEMU exit code exists.

## Step 5 — Present path, boot, service, and regression adjudication

- **Present path:** no T685 flight means there is no new guest-origin present or fb0 commit evidence. The T684 audit of the verified T683 UART had no Composer present/execute dispatch and no frame commit; this lane makes no PASS claim.
- **Boot bar:** not re-tested because Step 3 built no fix. In the referenced T683 cycle 1, UART line 49952 at uptime 1175.91s has blank `boot_completed=` and no `system_server` PID. `sys.boot_completed=1`, user unlock, CE availability, and end-of-flight system-server health were not observed for T685.
- **Service and runtime risks:** the T683 UART did not print the short init service name `vendor.hwc-2-1`, but the process `android.hardware.graphics.composer@2.1-service` ran as PID 20236 and completed a binder exchange. The staged T681 ELF was loaded by the guest's `/system/bin/linker64`; no Composer `CANNOT LINK EXECUTABLE` was reported. The flight used SELinux permissive mode. The T683 log has no Composer `/dev/mem` denial and does not prove `/dev/mem` was opened; this kernel's `CONFIG_DEVMEM` setting is the direct blocker. The allocator did log `avc: denied  { open } for  comm="android.hardwar" path="/dev/fb0" dev="tmpfs" ... permissive=1` at UART line 41186. The exact target-panel physical address/stride remains unverified: T681 defaults are known, but no panel/scanout is supplied by this QEMU flight. The device linker accepted the ELF, while the guest API level was not independently read; T681's Android 34 `LIBC_R` requirement is therefore runtime-load compatible in the reference flight but not separately API-attested.
- **T664–T684 regressions:** T685 changed no guest kernel, DTB, initramfs, vendor/system image, boot path, ACL, media.c2 arrangement, or toolchain/link lineage. No flight ran, so no fresh regression check is claimed. The referenced T683 report had no CE unlock, reported media.c2 respawning, and used the T681 ELF byte-for-byte; those are reference observations, not T685 passes.

## Result

The missing-framebuffer lead is confirmed as a real guest display-path wall, but not as a DTB-only defect. The first actionable failure is the canonical kernel lacking both `/dev/mem` support and fbdev support, with no display device in QEMU. This audit is the deliverable; the verdict is FAIL.
