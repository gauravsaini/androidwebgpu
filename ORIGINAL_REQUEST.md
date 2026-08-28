# Original User Request

## Initial Request — 2026-08-28T01:21:15Z

Build and stabilize the full guest Android-x86 graphics and runtime pipeline for AndroidWebGPU, enabling genuine guest-driven APK rendering onto the WebGPU canvas without host synthetic fallbacks.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Stabilize Guest Linux Kernel for v86 (No Stack Overrun Oops)
Rebuild the 32-bit x86 kernel (`linux-5.10`) targeting v86 compatibility:
- Disable `CONFIG_VMAP_STACK` and `CONFIG_STACKPROTECTOR` in defconfig to eliminate the `swapper/0 rewind_stack_and_make_dead Thread overran stack` oops.
- Ensure `CONFIG_ANDROID_BINDERFS=y`, `CONFIG_DRM_VIRTIO_GPU=y`, `CONFIG_FRAMEBUFFER_CONSOLE=y`, `CONFIG_FB_CFB_FILLRECT=y`, `CONFIG_VT=y`, and `CONFIG_FONT_8x16=y` remain active.
- Output valid, bootable `guest/build/bzImage` verified by `HdrS` / `0xAA55` boot headers.

### R2. Compile Guest EGL and HAL Shims
Cross-compile 32-bit x86 ELF shared libraries using the host i686 toolchain:
- Compile `guest/patches/egl_webgpu.cpp` to `guest/initrd/system/lib/egl_webgpu.so`.
- Compile `guest/patches/gralloc_virtgpu.cpp` to `guest/initrd/system/lib/gralloc.virtgpu.so`.
- Compile `guest/patches/hwcomposer_virtgpu.cpp` to `guest/initrd/system/lib/hwcomposer.virtgpu.so`.
- Package libraries into `guest/build/initrd.img` via `guest/tools/build_initrd.sh`.

### R3. Wire Guest Graphics Pipeline to VirtIO 2D/3D
Connect guest userspace graphics to the virtio-gpu DRM driver:
- Verify `/dev/dri/card0` and `/dev/fb0` nodes are created in guest userspace.
- Run `guest/initrd/system/bin/test_triangle` (EGL `glClearColor` + `glDrawArrays` via `VIRTIO_GPU_CMD_SUBMIT_3D`) and `skia_fb_test` (DRM 2D `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH`).
- Verify the Rust bridge receives commands, presents frames to scanout, and emits damage rects.

### R4. Integrate Guest ART Runtime and Zygote IPC
Enable guest bytecode execution:
- Verify `guest/initrd/system/framework/boot.art` (`art\n018`) and `framework.jar` (`dex\n035`) are mounted in rootfs.
- Ensure `zygote` daemon listens on `/dev/socket/zygote` in the guest.
- Wire `crates/zygote_client` and `crates/ams_rs` to dispatch package launches to the guest zygote socket.

### R5. End-to-End Headless Browser Validation
Validate the complete pipeline in a real browser session:
- Execute `validate_browser.mjs` in Chrome / headless runner.
- Ensure canvas displays guest-rendered graphics (test triangle / Skia / APK UI) with Shannon entropy $H \ge 1.0$.
- Capture and inspect `screenshot.png` to confirm bright, recognizable pixels rather than blank/dark fallback frames.

## Acceptance Criteria

### Kernel & Boot Invariants
- [ ] `guest/build/bzImage` boots in v86 without `Thread overran stack` oops or fatal kernel panic.
- [ ] Serial log confirms `VIRTIO_GPU_INIT` milestone (`[drm] fb0: virtio_gpudrmfb` or `modeset initialized`).
- [ ] `/dev/binderfs` mounts and `servicemanager` root handle 0 starts.

### Graphics & HAL Libraries
- [ ] `guest/initrd/system/lib/egl_webgpu.so`, `gralloc.virtgpu.so`, and `hwcomposer.virtgpu.so` exist as valid 32-bit x86 ELF shared objects.
- [ ] `test_triangle` runs in guest and outputs `Blue triangle rendered and presented to WebGPU swapchain successfully`.
- [ ] Host `VirtioGpuDevice` registers virtqueue kicks and `guestActive` remains true.

### Full-Stack Validation
- [ ] `node tests/test_v86_guest_boot.mjs` passes all stages (0 failures).
- [ ] `node --test tests/test_real_guest_rendering.mjs` passes all assertions.
- [ ] `cargo test --workspace` passes cleanly across all member crates.
- [ ] `node validate_browser.mjs` succeeds with Shannon entropy $H \ge 1.0$ and visual confirmation in `screenshot.png`.

## Follow-up — 2026-08-28T16:00:01Z

Parse and bind authentic F-Droid repository metadata (index-v1.jar / index JSON) directly into the inflated res/v9.xml and res/Kt.xml Android View hierarchy, eliminating static mock package arrays and rendering the live catalog onto the WebGPU canvas.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Authentic F-Droid Index Ingestion
Parse authentic F-Droid repository index metadata (index-v1.jar or real F-Droid index JSON) to extract real application definitions, package names, version strings, summaries, and icons.

### R2. Dynamic Layout & Adapter Data Binding
Bind the parsed repository index entries directly to the inflated res/v9.xml and res/Kt.xml RecyclerView items without any hardcoded mock package lists.

### R3. WebGPU Display Verification & Headless Validation
Ensure the rasterized view hierarchy renders accurately to the WebGPU VirtIO scanout (720x1440 portrait) with no edge clipping and verified Shannon entropy.

## Acceptance Criteria

### Data Integrity
- [ ] No hardcoded package arrays (fdroidRepoApps or similar) in src/android_runtime.js.
- [ ] Package items displayed in the RecyclerView originate directly from authentic F-Droid repository index data.

### Visual & Execution Verification
- [ ] node validate_browser.mjs passes with Shannon entropy H >= 1.0.
- [ ] pnpm test passes all test suites with 0 failures.
- [ ] screenshot.png captures the real F-Droid repository catalog filling the 720x1440 viewport edge-to-edge.

