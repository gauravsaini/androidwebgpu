# Project: Authentic In-Guest Android UI Runtime & Presentation

## Architecture
The architecture comprises:
1. **Authentic In-Guest Android Runtime & Framework (`guest/initrd/system/framework/`, `guest/initrd/system/lib/`)**:
   - Genuine Android binary runtime image (`boot.art`), framework bytecode (`framework.jar`, `core-libart.jar`, `ext.jar`, `services.jar`), and shared libraries (`libart.so`).
   - Packaged into `initrd.img` without synthetic JS asset generation (`guest/tools/generate_art_assets.mjs`).
2. **Real Zygote Daemon & System Services IPC (`guest/app_process.c`, `guest/initrd/init`, `crates/`)**:
   - `init` exports authentic `BOOTCLASSPATH`, `ANDROID_ROOT`, and `ANDROID_DATA`.
   - Zygote daemon (`app_process`) listens on `/dev/socket/zygote` and handles `fork()` requests for application processes.
   - Binder IPC routing over `/dev/binder` connects `servicemanager` (Handle 0), `ams_rs` (Handle 4), and `pms_rs` (Handle 5).
   - In-guest execution of F-Droid `MainActivity` lifecycle (`onCreate` -> `onStart` -> `onResume`).
3. **Pure In-Guest SurfaceFlinger & VirtIO-GPU DRM Presentation (`guest/surfaceflinger.c`, `guest/patches/`, `crates/virtio_gpu_bridge/`)**:
   - In-guest `surfaceflinger` ELF, `egl_webgpu.cpp`, `gralloc_virtgpu.cpp`, and `hwcomposer_virtgpu.cpp` render directly to `/dev/dri/card0`.
   - VirtIO-GPU DRM presentation delivers scanout 0 frames directly to WebGPU canvas with damage rect tracking.
   - Host `LayoutInflater` fallback and mock view injection disabled during active in-guest execution.
4. **End-to-End Quality Gates & Test Suites (`tests/`, `validate_browser.mjs`)**:
   - `/dev/ttyS0` serial milestone logging and circular Logcat buffer tag validation.
   - Shannon entropy $H \ge 1.0$ validation on canvas framebuffer and screenshot PNG.
   - 100% pass across `cargo test --workspace`, `pnpm test`, `tests/run_e2e_tests.mjs`, and `tests/test_v86_guest_boot.mjs`.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Authentic Runtime Asset Staging | Stage genuine `boot.art`, `framework.jar`, `core-libart.jar`, `ext.jar`, `services.jar`, `libart.so` | M1 | Survey / R1 |
| 2 | Synthetic Generator Removal | Remove/replace `generate_art_assets.mjs` from `build_initrd.sh` and `build.sh` | M1 | Survey / R1 |
| 3 | Genuine Initrd Packaging | Package genuine Android assets into `guest/build/initrd.img` and `dist/initrd.img` | M1 | Survey / R1 |
| 4 | Authentic BOOTCLASSPATH & Init Config | Export `BOOTCLASSPATH`, `ANDROID_ROOT`, `ANDROID_DATA` in `guest/initrd/init` | M2 | Survey / R2 |
| 5 | Zygote Daemon Socket & Fork Lifecycle | Listen on `/dev/socket/zygote`, parse wire args, and fork app processes | M2 | Survey / R2 |
| 6 | ServiceManager & Native Service IPC | Wire `servicemanager` (Handle 0), `ams_rs` (Handle 4), and `pms_rs` (Handle 5) | M2 | Survey / R2 |
| 7 | In-Guest F-Droid Lifecycle | Execute F-Droid `MainActivity` `onCreate` -> `onStart` -> `onResume` in guest | M2 | Survey / R2 |
| 8 | Pure SurfaceFlinger & DRM Scanout | SurfaceFlinger + EGL/Gralloc/HWC render to `/dev/dri/card0` and VirtIO scanout 0 | M3 | Survey / R3 |
| 9 | Host LayoutInflater Fallback Elimination | Disable host `LayoutInflater` mock rendering when in-guest mode is active | M3 | Survey / R3 |
| 10 | Quality Gates & Entropy Verification | Validate `/dev/ttyS0` serial milestones, Logcat tags, and Shannon entropy $H \ge 1.0$ | M4 | Survey / R4 |
| 11 | Complete Test Suite Verification | Pass 100% of `cargo test`, `pnpm test`, `run_e2e_tests.mjs`, `test_v86_guest_boot.mjs` | M4 | Survey / R4 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Authentic ART & Framework Staging | Stage genuine `boot.art`, `framework.jar`, `core-libart.jar`, `ext.jar`, `services.jar`, `libart.so`, remove synthetic generator, build real initrd.img | none | DONE |
| M2 | Zygote Daemon & ServiceManager IPC | Configure `BOOTCLASSPATH` in `init`, verify `/dev/socket/zygote` fork lifecycle, wire Handle 0/4/5 IPC, execute F-Droid lifecycle | M1 | DONE |
| M3 | Pure SurfaceFlinger DRM & Zero Host Fallback | Link SurfaceFlinger ELF + EGL/Gralloc/HWC to `/dev/dri/card0`, disable host LayoutInflater fallback in `src/android_runtime.js` | M2 | DONE |
| M4 | E2E Verification & Quality Gates | Validate serial logging, Logcat tags, damage rects, $H \ge 1.0$ entropy, and execute all test suites with 0 failures | M3 | DONE |

## Interface Contracts
### Boot Classpath & Android Environment
```sh
export ANDROID_ROOT=/system
export ANDROID_DATA=/data
export BOOTCLASSPATH=/system/framework/core-libart.jar:/system/framework/ext.jar:/system/framework/framework.jar:/system/framework/services.jar
export LD_LIBRARY_PATH=/system/lib:/vendor/lib
```

### Zygote IPC Protocol
```rust
pub struct ZygoteSpawnArgs {
    pub package_name: String,
    pub nice_name: String,
    pub target_sdk_version: u32,
    pub uid: u32,
    pub gid: u32,
    pub entry_point: String,
}
```

### Binder Root Handles
- Handle 0: `IServiceManager` (`guest_servicemanager`)
- Handle 1: `ISurfaceComposer` (`SurfaceComposerService`)
- Handle 2: `IInputManager` (`inputflinger_rs`)
- Handle 3: `IWindowManager` (`wms_rs`)
- Handle 4: `IActivityManager` (`ams_rs`)
- Handle 5: `IPackageManager` (`pms_rs`)
- Handles 10, 20, 30: `IGraphicBufferProducer` (`surfaceflinger_gpu_service`)

## Code Layout
- `guest/initrd/init`: In-guest initialization script, mounts, env vars, service startup.
- `guest/initrd/system/framework/`: Authentic Android framework JARs and ART image (`boot.art`).
- `guest/initrd/system/lib/`: Native shared libraries (`libart.so`, `egl_webgpu.so`, `gralloc.virtgpu.so`, `hwcomposer.virtgpu.so`).
- `guest/app_process.c`: In-guest Zygote daemon, `/dev/socket/zygote` listener, app process fork.
- `guest/surfaceflinger.c`: In-guest display compositor.
- `guest/tools/build_initrd.sh`: Initrd cpio.gz builder script.
- `crates/`: 30 Rust crates (system services, Binder IPC, VirtIO-GPU bridge, swapchain, compositor).
- `src/android_runtime.js`: Web runtime and guest/host execution manager.
- `src/virtio_gpu_device.js`: VirtIO-GPU device emulation and DRM packet dispatcher.
- `tests/`: Automated test suites (`run_e2e_tests.mjs`, `test_v86_guest_boot.mjs`, etc.).
- `validate_browser.mjs`: Headless browser validation runner.
