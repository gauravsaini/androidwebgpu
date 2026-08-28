# Project: Pure In-Guest Android Userspace Rendering Pipeline & Display Metrics Unification

## Architecture
The system executes a pure in-guest Android userspace runtime (AOSP services & HALs) inside v86 x86 Linux virtualization, rendering directly to `/dev/dri/card0` VirtIO scanout backed by WebGPU:
1. **In-Guest Userspace (initrd)**: Linux kernel boots into `guest/initrd/init`, mounts BinderFS (`/dev/binderfs`), initializes `/dev/dri/card0` and `/dev/fb0`, starts `servicemanager`, `surfaceflinger`, `app_process` (Zygote), native Rust AOSP services (`ams_rs`, `wms_rs`, `pms_rs`, `inputflinger_rs`), and HALs (`egl_webgpu.so`, `gralloc.virtgpu.so`, `hwcomposer.virtgpu.so`).
2. **VirtIO GPU Scanout Pipeline**: Guest software flushes 2D/3D frames via DRM GEM / VirtIO PCI I/O ports (`0xC100`). Upon receiving VirtIO commands, `VirtioGpuDevice` sets `guestActive = true` and permanently disables host-side synthetic fallback.
3. **Display Metrics & Layout**: Host canvases (`android.html`, `index.html`), Rust bridge (`VirtioGpuBridge`), CSS (`css/android.css`), and in-guest HALs are unified to portrait `720x1440` (1:2 aspect ratio, DPR 2.0 / 320dpi) with `object-fit: contain` and zero cropping/letterboxing.
4. **App Execution**: `F-Droid.apk` is staged in `/data/app/org.fdroid.fdroid/base.apk`. AMS dispatches cold start intent to `/dev/socket/zygote` to fork the application process.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Real In-Guest HALs | `egl_webgpu.so`, `gralloc.virtgpu.so`, `hwcomposer.virtgpu.so` compiled and placed in `system/lib` | M1 | R1 |
| 2 | In-Guest Core Binaries | Real compiled ELF binaries for `surfaceflinger`, `app_process`, `synthetic_virtio_probe`, `skia_fb_test` in `system/bin` | M1 | R1 |
| 3 | Guest Boot & BinderFS | `init` mounts `/dev/binderfs`, `/dev/dri/card0`, starts `servicemanager` and HAL daemons | M1 | R1 |
| 4 | VirtIO Scanout Lockout | VirtIO PCI port `0xC100` kicks set `VirtioGpuDevice.guestActive = true` and gate host fallback | M1 | R1 |
| 5 | Resolution Unification | Unify host HTML/JS, Rust bridge, and guest HALs to portrait `720x1440` | M2 | R2 |
| 6 | CSS Viewport & Sizing | Update `css/android.css` `.phone-bezel` to 1:2 ratio, `object-fit: contain`, zero letterboxing/clipping | M2 | R2 |
| 7 | Layout Viewport Bounds | `RecyclerView` and `ScrollView` calculate layout bounds against full 720x1440 portrait bounds | M2 | R2 |
| 8 | F-Droid APK Staging | Stage `F-Droid.apk` into `/data/app/org.fdroid.fdroid/base.apk` on guest `tmpfs` | M3 | R3 |
| 9 | Zygote Socket IPC | `app_process` listens on `/dev/socket/zygote`, processes fork requests, returns 4-byte LE PID | M3 | R3 |
| 10 | HWUI/Skia Rendering | In-guest view hierarchy rendered through Skia/EGL surface to `/dev/dri/card0` | M3 | R3 |
| 11 | Boot & HAL Verification | `node tests/test_v86_guest_boot.mjs` passes all stages (138+ assertions) | M4 | R4 |
| 12 | Test Suite Pass | `pnpm test` (all unit & E2E suites) and `cargo test --workspace` pass with 0 failures | M4 | R4 |
| 13 | Headless Browser Validation | `node validate_browser.mjs` passes with Shannon entropy $H \ge 1.0$ and captures `screenshot.png` | M4 | R4 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | In-Guest HALs & ELF Binaries | Compile ELF binaries (`surfaceflinger`, `app_process`, `synthetic_virtio_probe`, HALs), package `initrd.img`, verify VirtIO scanout lockout | none | DONE |
| M2 | Display Metrics & CSS Sizing | Unify resolution across host JS/Rust/HTML/Guest to `720x1440`, fix CSS `.phone-bezel` and canvas sizing | M1 | PLANNED |
| M3 | Zygote IPC & APK Staging | Stage `F-Droid.apk` to `/data/app/org.fdroid.fdroid/base.apk`, implement Zygote socket server & HWUI rendering | M1, M2 | PLANNED |
| M4 | E2E Verification & Browser Validation | Run full test suites, validate browser visual entropy, generate `screenshot.png`, forensic audit | M1, M2, M3 | PLANNED |

## Interface Contracts
### Guest Initrd ↔ Kernel / Devices
- `/dev/binderfs`: mounted at `/dev/binderfs` with nodes `binder`, `hwbinder`, `vndbinder`
- `/dev/dri/card0` (major 226, minor 0): VirtIO DRM device node
- `/dev/socket/zygote`: UNIX domain socket, line-delimited request, 4-byte LE `i32` PID response
- VirtIO PCI I/O Port `0xC100`: Queue 0 notify triggers scanout and locks `VirtioGpuDevice.guestActive`

### Host ↔ Guest Display Metrics Contract
- Standard Resolution: `720` (width) x `1440` (height) pixels (1:2 aspect ratio, 320 dpi, DPR = 2.0)
- Canvas: `<canvas id="screen" width="720" height="1440"></canvas>`
- Rust Bridge: `GET_DISPLAY_INFO` returns rect `[0, 0, 720, 1440]`
- CSS Viewport: `.phone-bezel` dimensioned for 1:2 inner viewport (400x800 container inside 420x892 bezel), `object-fit: contain`, zero padding voids

## Code Layout
- `guest/`: In-guest C/C++ HAL sources, build scripts, headers
  - `guest/patches/`: `egl_webgpu.cpp`, `gralloc_virtgpu.cpp`, `hwcomposer_virtgpu.cpp`
  - `guest/include/`: Khronos, DRM, and Android hardware headers
  - `guest/initrd/`: Root filesystem staging for guest initrd
  - `guest/tools/`: `build_initrd.sh`, `build_synthetic_probe.sh`
- `crates/`: Rust workspace (AOSP services, HALs, VirtIO GPU bridge, Zygote client)
- `src/`: Host JavaScript runtime (`virtio_gpu_device.js`, `view_rasterizer.js`, `android_runtime.js`, `system_bootstrap.js`, `raster_worker.js`)
- `css/`: `android.css`
- `android.html`, `index.html`: Entry point Web pages
- `tests/`: JavaScript test suites
- `validate_browser.mjs`: Headless browser validation and screenshot generation
