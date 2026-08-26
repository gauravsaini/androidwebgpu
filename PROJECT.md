# Project: androidwebgpu

## Architecture
AndroidWebGPU executes real-world Android applications directly against paravirtualized native Rust system services and a client-side browser runtime through WebGPU.

- **Guest Subsystem**: 32-bit x86 Linux kernel bzImage with `CONFIG_ANDROID_BINDER_IPC=y`, `CONFIG_ANDROID_BINDERFS=y`, `CONFIG_DRM_VIRTIO_GPU=y`, `CONFIG_ASHMEM=y`; initrd packaging `boot.art` and `framework.jar` with `classes.dex` as entry 0.
- **Host GPU Bridge & SurfaceFlinger**: `VirtioGpuBridge`, `GraphicBufferProducerService` on handles 10, 20, 30, `SurfaceComposerService`, `WebGpuCompositor`, and `WebGpuSwapchain` sharing unified `wgpu::Device` and `wgpu::Queue`.
- **Native System Services**:
  - `pms_rs`: Binary AXML/ARSC manifest parser, Intent resolution, ServiceManager registration.
  - `ams_rs`: Zygote client socket IPC, Application thread binding, Activity lifecycle state machine.
  - `wms_rs`: Window sessions, InputChannel socketpair creation, SurfaceControl binding to SurfaceFlinger.
  - `inputflinger_rs`: InputDispatcher, touch/motion/key datagram IPC.
- **Frontend & Browser Presentation**: `v86_guest_manager.js`, `virtio_gpu_device.js`, `raster_worker.js`, OffscreenCanvas with damage rect culling.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Real x86 bzImage | 32-bit x86 Linux kernel with HdrS boot header and defconfig | M1 | ORIGINAL_REQUEST R1 |
| 2 | boot.art & framework.jar | ART boot image and Framework DEX archive packaged in initrd.img | M1 | ORIGINAL_REQUEST R1 |
| 3 | v86 bzImage verification | `verifyBzImage` validation in `v86_guest_manager.js` | M1 | ORIGINAL_REQUEST R1 |
| 4 | GraphicBufferProducer on 10, 20, 30 | Real GPU texture allocation and queueing to SurfaceComposer | M2 | ORIGINAL_REQUEST R2 |
| 5 | Shared WebGPU Device & Queue | Unified `wgpu::Device` across GlContext, Bridge, SurfaceComposer, VkDevice | M2 | ORIGINAL_REQUEST R2 |
| 6 | Native System Services Wire-up | PMS, AMS, WMS-SurfaceFlinger binding, InputFlinger socketpairs | M3 | ORIGINAL_REQUEST R3 |
| 7 | Single-Set Offscreen Swapchain | Unified texture set for render writes and readback in Offscreen mode | M4 | ORIGINAL_REQUEST R4 |
| 8 | WebGPU TIMESTAMP_QUERY | GPU execution duration profiling via timestamp query sets | M4 | ORIGINAL_REQUEST R4 |
| 9 | Browser Canvas & Damage Rects | Real canvas presentation, transferControlToOffscreen, damage scissoring | M5 | ORIGINAL_REQUEST R5 |
| 10 | Automated Test Verification | 100% pass for `cargo test --workspace` and `node tests/test_v86_guest_boot.mjs` | M6 | ORIGINAL_REQUEST Acceptance |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Real Guest Boot Assets | bzImage, boot.art, framework.jar, initrd.img, v86 verify | none | DONE |
| M2 | GraphicBufferProducer & Shared Device | Handles 10, 20, 30, VkDevice device sharing | M1 | DONE |
| M3 | End-to-End Native System Services | WMS-SurfaceFlinger connection, PMS, AMS, InputFlinger | M2 | DONE |
| M4 | Single-Set Swapchain & GPU Profiling | WebGpuSwapchain offscreen textures, TIMESTAMP_QUERY | M2 | DONE |
| M5 | Real Canvas & Damage Rect Presentation | Browser canvas rasterization, damage rect culling | M4 | DONE |
| M6 | Full Test & Audit Verification | cargo test --workspace, test_v86_guest_boot.mjs, audit | M1, M2, M3, M4, M5 | DONE |

## Code Layout
- `guest/`: Kernel defconfig, tools (`generate_bzimage.mjs`, `generate_art_assets.mjs`, `build_initrd.sh`), `build/bzImage`, `build/initrd.img`
- `crates/virtio_gpu_bridge/`: Virtio GPU bridge, Binder handle registration
- `crates/surfaceflinger_gpu_service/`: SurfaceComposerService, GraphicBufferProducerService
- `crates/webgpu_swapchain/`: WebGpuSwapchain, timestamp queries
- `crates/webgpu_compositor/`: WebGpuCompositor multi-layer rendering
- `crates/vulkan2wgpu/`: Vulkan to WebGPU translation, VkDevice
- `crates/pms_rs/`, `crates/ams_rs/`, `crates/wms_rs/`, `crates/inputflinger_rs/`: Native system services
- `src/`: `v86_guest_manager.js`, `virtio_gpu_device.js`, `raster_worker.js`
- `tests/`: `test_v86_guest_boot.mjs`, `adversarial_browser_bench_verifier.mjs`, `test_v86_adversarial_challenger.mjs`
