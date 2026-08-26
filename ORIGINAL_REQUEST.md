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

## Follow-up — 2026-08-26T02:22:42Z

Close all gaps between expected test stubs and real hardware/VM execution path across Guest Build, Virtio-GPU Bridge, AOSP System Services, WebGPU Swapchain, and Browser UI.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Real Guest Kernel & Initrd Boot Assets
- Build real 32-bit x86 Linux `bzImage` and output to `guest/build/bzImage`.
- Build real x86 `boot.art` and `framework.jar` and package into `guest/build/initrd.img`.
- Remove host binaries (`media_host_rs`) from `guest/initrd/init`.
- Update `src/v86_guest_manager.js` to fetch and verify the real `bzImage` binary, invoke `V86Starter`, and avoid dummy array buffer fallbacks.

### R2. Real GraphicBufferProducer Bridge & Shared WebGPU Device
- Replace `IGraphicBufferProducer` stub on handles 10, 20, 30 in `crates/virtio_gpu_bridge/src/bridge.rs` with real `GraphicBufferProducerService`.
- Wire buffer allocation and queuing directly to `SurfaceComposerService` buffer queues so `acquire_latest` retrieves true textures.
- Share single `wgpu::Device` and `wgpu::Queue` instances across `GlContext`, `VirtioGpuBridge`, `VkDevice`, and `SurfaceComposerService`.

### R3. End-to-End Native System Services
- Connect PMS to real ServiceManager registration and manifest activity resolution.
- Connect AMS to real `zygote-client` socket to fork app processes.
- Connect WMS to create and bind active Surface layers to SurfaceFlinger.
- Connect InputFlinger to real `InputChannel` socketpairs.

### R4. Single-Set Offscreen Swapchain & GPU Profiling
- Update `crates/webgpu_swapchain/src/swapchain.rs` so `Offscreen` target uses one unified texture set for both composition writes and readback.
- Request and resolve `TIMESTAMP_QUERY` on GPU device and update measured GPU execution time.

### R5. Real Browser Canvas & Damage Rect Presentation
- Remove fake DOM overlay in `index.html` and mock dock in `src/arcade_demo.js`.
- Direct WebGPU rasterization to real canvas via `transferControlToOffscreen` in `src/v86_guest_manager.js`.
- Enable WebGPU view and damage rect culling in `src/raster_worker.js` and `src/virtio_gpu_device.js`.

## Acceptance Criteria

### Real Path Execution
- [ ] `guest/build/bzImage` exists and passes Linux x86 boot header validation.
- [ ] `guest/build/initrd.img` contains `boot.art` and `framework.jar`.
- [ ] Handles 10, 20, 30 allocate and queue real GPU textures to `SurfaceComposerService`.
- [ ] `WebGpuSwapchain` writes and reads from the exact same texture view for offscreen passes.
- [ ] Real canvas displays rendered frames without black screen or fake placeholder UI.

### Automated Verification
- [ ] `cargo test --workspace` passes with 0 failures across all crates.
- [ ] `node tests/test_v86_guest_boot.mjs` and all test runners pass with 0 failures.

