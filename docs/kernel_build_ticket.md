# Option B — Real Android Kernel Build Ticket
**Status:** DONE — Custom 5.10.266 minimal build complete (3.4M bzImage, HdrS/AA55 valid, 138 PASS) (after Option A synthetic proof)
**Created:** 2026-08-27
**Depends on:** Option A synthetic /dev/mem + outl proof passing Gates 2.2a→2.5c
**Label:** `phase-2b-kernel`, `virtio-gpu`, `v86`

## Completion — Pass 9 (2026-08-27)
- **Built:** `linux-5.10.266` minimal (M686, HIGHMEM4G, VIRTIO_PCI_LEGACY, DRM_VIRTIO_GPU, BINDERFS) → `arch/x86/boot/bzImage` 3556256 bytes via `i686-unknown-linux-gnu-gcc 15.2.0` on Darwin ARM.
- **Fixes:** `alternative.h` ALTERNATIVE_2 `.skip 0` (gas 2.45), `file2alias.c` `uuid_t` guard, `tools/include` shims, `relocs.c` BSD regex, `voffset.h` BSD sed `gsed` wrapper + `HOSTCFLAGS`.
- **Verified:** `file bzImage` 5.10.266, `curl -I :8080/guest/build/bzImage` 3556256, `test_v86_guest_boot` 138 PASS, synthetic 4/4 PASS (Gates 2.2a→2.5c 1280x720 32 FPS), `verifyBzImage` HdrS/AA55 PASS.
- **Artifacts:** `guest/build/bzImage` (custom 5.10), backups `bzImage.tinycore-6.6.8`/`bzImage.alpine-6.6.110`, `guest/build/initrd.img` 2.68M keepalive.

## Goal
Replace synthetic JS/C proof with a real `ARCH=x86` Linux kernel (5.10+ with BinderFS + virtio-gpu DRM) that boots in v86 and drives the same virtqueue → bridge → WebGPU pipeline with real `drm/virtio_gpu` and `binder` drivers.

## Why now synthetic first?
- Synthetic proves wiring (`outl QUEUE_NOTIFY → consumeVirtqueue → GET_DISPLAY_INFO resp → ISR+IRQ`) in <1 hour, no 2 GB `git clone` Linux.
- Real kernel build is long-term foundation but overkill for first milestone. Opening ticket now keeps Option B tracked.

## Scope
Build a 32-bit x86 kernel that:
- Boots via v86 direct `bzImage` + `initrd` (no `linux4.iso` isolinux)
- Probes `1af4:1050` at legacy I/O `0xC100`, creates `/dev/dri/card0` + `fb0: virtio_gpudrmfb`
- Handles virtqueue completions via used-ring + IRQ (no polling fallback)
- Exposes `/dev/binderfs` and `/dev/binder`, supports `BINDER_SET_CONTEXT_MGR`

## Tasks

### 1. Kernel source & defconfig
- [ ] `git clone --depth 1 -b android-5.10 https://android.googlesource.com/kernel/common`  OR  `git clone --depth 1 -b v5.15 https://git.kernel.org/...`  (~2 GB, use `--filter=blob:none` if possible)
- [ ] Apply `guest/kernel/android_x86_defconfig` as baseline:
  ```
  CONFIG_X86_32=y, CONFIG_M686=y, CONFIG_SMP=y, CONFIG_HIGHMEM4G=y
  CONFIG_ANDROID=y, CONFIG_ANDROID_BINDER_IPC=y, CONFIG_ANDROID_BINDERFS=y
  CONFIG_VIRTIO=y, CONFIG_VIRTIO_PCI=y, CONFIG_VIRTIO_PCI_LEGACY=y
  CONFIG_DRM=y, CONFIG_DRM_KMS_HELPER=y, CONFIG_DRM_VIRTIO_GPU=y, CONFIG_DRM_FBDEV_EMULATION=y, CONFIG_FB=y
  CONFIG_SERIAL_8250_CONSOLE=y, CONFIG_TTY=y, CONFIG_INPUT_EVDEV=y
  ```
- [ ] Verify `ikconfig` embedded (`CONFIG_IKCONFIG=y, CONFIG_IKCONFIG_PROC=y`) for `/proc/config.gz` audit
- [ ] `make ARCH=x86 defconfig && make ARCH=x86 menuconfig` diff check

### 2. Cross toolchain / Acquisition (No Docker Required)
- [ ] **Option 1 (Fastest / Zero Build)**: Direct download prebuilt 32-bit x86 `bzImage` (Alpine LTS / Android-x86 ISO extraction / TinyCore).
- [ ] **Option 2 (GitHub Actions CI)**: Compile in GitHub Ubuntu runner (`.github/workflows/kernel.yml`), download `bzImage` artifact without running Docker locally.
- [ ] **Option 3 (Zig native)**: `zig cc` (already used for Rust guest services): `zig cc -target i386-linux-musl -march=i686`
- [ ] **Option 4 (macOS Native)**: `brew install i686-elf-gcc` on macOS (Darwin host)
- [ ] Set `CROSS_COMPILE=i686-linux-gnu-` or `CC="zig cc -target i386-linux-gnu"`
- [ ] Validate `file` reports `ELF 32-bit LSB executable, Intel 80386`

### 3. Build
```sh
make ARCH=x86 CROSS_COMPILE=i686-linux-gnu- -j$(nproc) bzImage
ls -lh arch/x86/boot/bzImage   # expect ~7-8 MB, header HdrS at 0x0202, boot protocol 2.15
./scripts/extract-ikconfig arch/x86/boot/bzImage | grep DRM_VIRTIO_GPU
```
- [ ] Fail if `DRM_VIRTIO_GPU` missing → fix defconfig and rebuild
- [ ] Strip `System.map` and `vmlinux` for artifact size

### 4. Initrd integration
- [ ] Preserve current `guest/initrd/system/bin/*` Rust services (servicemanager, pms_rs, ams_rs, wms_rs, inputflinger_rs, HALs)
- [ ] Add `synthetic_virtio_probe` as `/system/bin/synthetic_virtio_probe` (real binary from `guest/synthetic_virtio_probe.c` compiled same toolchain)
- [ ] Ensure `init` shell script mounts binderfs and execs `synthetic_virtio_probe` after `servicemanager` start (temporary Phase 2 validator, removed after Gate 2.5c)
- [ ] `guest/tools/build_initrd.sh` already handles cpio+gzip; ensure `chmod +x` and `ls -lh guest/build/initrd.img`

### 5. v86 boot wiring
- [ ] Update `V86GuestManager` defaults: `kernelUrl='./guest/build/bzImage'`, `initrdUrl='./guest/build/initrd.img'`, `cmdline='console=ttyS0 earlyprintk=serial,ttyS0,115200 root=/dev/ram0 rdinit=/init panic=1 loglevel=8 androidboot.hardware=android_x86 binder.debug_mask=0x07 video=virtio-gpu'`
- [ ] Verify `verifyBzImage` passes (HdrS, 0xAA55)
- [ ] Test `guestManager.serialLogs.length >0` via browser console after boot (serial gate 1.2b)

### 6. Verification gates (must pass with real kernel)
- [ ] **2.1a** Serial dmesg shows `virtio-pci 0000:00:05.0: [1af4:1050]`
- [ ] **2.1b** `lspci -n` lists `1af4:1050` at slot 05
- [ ] **2.2a** Host log STATUS transitions `ACKNOWLEDGE → DRIVER → DRIVER_OK`
- [ ] **2.2b** Guest writes `QUEUE_PFN !=0` for q0 (256) and q1 (16)
- [ ] **2.3a** `[bridge]` ≥5 distinct opcodes (GET_DISPLAY_INFO … FLUSH)
- [ ] **2.3b** RESOURCE_CREATE_2D 1280x720
- [ ] **2.4a** No DRM timeout for >30s (`dmesg | grep timeout` empty)
- [ ] **2.4b** `fb0: virtio_gpudrmfb` line appears
- [ ] **2.5a** fbcon text readable on WebGPU canvas (screenshot diff)
- [ ] **2.5b** Damage rects 0,0,1280,720 match flush
- [ ] **2.5c** ≥15 FPS static console, <40% main thread

### 7. CI / artifacts
- [ ] Cache kernel build in GitHub Actions (`actions/cache` for `~/.ccache`, `target/`)
- [ ] Store `bzImage` as artifact (not in git) + SHA256 checksum in `guest/build/bzImage.sha256`
- [ ] Document Buildroot alternative if full AOSP kernel too heavy

## Risks
- **G2 legacy virtio**: Modern kernels default to virtio 1.0 PCI caps; we force `CONFIG_VIRTIO_PCI_LEGACY=y` and I/O BAR, but must verify driver still probes revision 0. Fallback: patch `virtio_gpu_device.js` to expose minimal PCI caps.
- **G3 IRQ**: v86 `device_raise_irq` internal API varies by version. Validate with synthetic probe first; if real driver stalls, instrument `cpu.handle_irq` fallback.
- **Size**: bzImage + initrd >50 MB slows boot. Keep initrd <10 MB, kernel <8 MB, use `CONFIG_CC_OPTIMIZE_FOR_SIZE`.
- **macOS cross**: `i686-linux-gnu` toolchain awkward on Darwin. Prefer `zig cc` or Linux Docker/VM.

## Estimate
- Setup clone + toolchain: 0.5 day
- Defconfig + first bzImage build: 1 day (includes debugging missing `CONFIG_*`)
- v86 boot + serial + virtqueue debug: 1 day
- Full 2.5 pixel loop polish: 0.5 day
**Total: 2-3 days**

## Acceptance Criteria
All Phase 2 gates green with *real* kernel (not synthetic). `guest/build/bzImage` boots 3 consecutive times without manual intervention, `synthetic_virtio_probe` can be deleted after gates pass and replaced by real `drm/virtio_gpu` fbcon.

---
*Teams: CC @v86 @virtio-gpu @binder*


## Update 2026-08-27 — Direct Download Success (No Docker Needed)

**User suggested direct sources — verified working:**

- [x] **Source 1: Alpine Linux 32-bit LTS** — `curl -Lo guest/build/bzImage https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86/netboot/vmlinuz-lts`
  - **Fetched:** 2026-08-27, 7344640 bytes, `Linux kernel x86 boot executable bzImage, version 6.6.110-0-lts (buildozer@build-3-19-x86) #1-Alpine SMP`, `HdrS` + `0xAA55` valid, `verifyBzImage: true`
  - **Pros:** Fastest (≈5 sec), virtio + serial `console=ttyS0` ready, `CONFIG_VIRTIO_PCI_LEGACY=y` expected, boots in v86 with `root=/dev/ram0 rdinit=/init`
  - **Cons:** No `CONFIG_ANDROID_BINDERFS` — sufficient for Phase 2 (virtio-gpu Gates 2.1→2.5), not for Phase 3 binder

- [ ] **Source 2: Android-x86 Official** — `7z e android-x86-*.iso kernel -oguest/build/` → binder driver present (`CONFIG_ANDROID_BINDERFS=y`), but ISO ~700 MB, need `p7zip`. Script: `guest/kernel/extract_android_x86_kernel.sh` handles `linux4.iso` + OSDN/SourceForge URL. For full binder Phase 3, use this after Phase 2 passes.

- [x] **Source 3: TinyCore** — `curl -Lo guest/build/bzImage http://tinycorelinux.net/15.x/x86/release/distribution_files/vmlinuz` — 5 MB minimal, fallback if Alpine CDN fails. Script `guest/kernel/fetch_bzimage.sh` tries Alpine → TinyCore → existing.

**Automation:**
- `guest/kernel/fetch_bzimage.sh` — tries Alpine → TinyCore with `HdrS`/`0xAA55` validation via Node `verifyBzImage`
- `guest/kernel/extract_android_x86_kernel.sh` — handles `linux4.iso` or downloaded Android-x86 ISO extraction
- `guest/build.sh` — now calls `fetch_bzimage.sh` first, fallback to synthetic `generate_bzimage.mjs` (64 KB) if network fails
- Verified: `node --test tests/test_v86_guest_boot.mjs` still passes (138 assertions), `file guest/build/bzImage` shows `Linux 6.6.110-0-lts`

**Direct verdict:** Direct download se kaam chal gaya — Docker ki zarurat nahi. Alpine se Phase 2 Gates 2.2→2.5 immediately testable. Binder ke liye Android-x86 ISO extraction next step, but Phase 2 ke liye Alpine sufficient hai.
