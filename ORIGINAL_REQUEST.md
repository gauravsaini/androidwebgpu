# Original User Request

## Initial Request — 2026-08-26T00:08:24Z

Implement the five real-path components to execute real Android APKs in the browser with genuine pixel rendering through WebGPU.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Real Guest Build Artifacts
- Build real x86 Linux `bzImage` and output to `guest/build/bzImage`.
- Build real x86 `boot.art` and `framework.jar` and package inside `guest/build/initrd.img`.
- Ensure guest bootloader loads real binaries without simulated fallbacks.

### R2. Real GraphicBufferProducer Bridge
- In `crates/virtio_gpu_bridge/src/bridge.rs`, replace stub on handles 10, 20, 30 with real `GraphicBufferProducerService`.
- Wire buffer allocation and queuing directly to `SurfaceComposerService` buffer queues and unified `wgpu::Device`/`wgpu::Queue`.

### R3. Real System Services Wiring
- Connect PMS to real ServiceManager registration and manifest resolution.
- Connect AMS to real `zygote-client` process forking and Activity lifecycle.
- Connect WMS to allocate and bind real Surface layers to SurfaceFlinger.
- Connect InputFlinger to real `InputChannel` socketpairs.

### R4. Single-Set Offscreen Swapchain
- Fix `crates/webgpu_swapchain/src/swapchain.rs` so `Offscreen` target uses one unified texture set for both composition writes and readback.
- Ensure pixel readback always extracts the true rendered texture.

### R5. Real Browser Canvas & Rasterization
- Remove fake UI layers and mock grids in `index.html` and `src/raster_worker.js`.
- Bind real WebGPU rasterization to canvas using `transferControlToOffscreen` and accurate damage rects.

## Acceptance Criteria

### Execution & Visual Pipeline
- [ ] `guest/build/bzImage` and `guest/build/initrd.img` (with `boot.art`/`framework.jar`) exist and validate.
- [ ] Handles 10, 20, 30 allocate real GPU textures via `GraphicBufferProducerService`.
- [ ] Real APK launches, renders frames through `WebGpuSwapchain`, and displays true pixels on canvas without black screens or fake overlays.

### Automated Verification
- [ ] `cargo test --workspace` passes with 0 failures.
- [ ] `node tests/test_v86_guest_boot.mjs` and all test suites pass with 0 failures.
