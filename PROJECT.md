# Project: Android WebGPU Real Execution & Visual Pipeline

## Architecture
```
┌────────────────────────────────────────────────────────┐
│                   Android Guest OS                     │
│  - Guest Kernel: bzImage (x86 Linux 5.10 / BinderFS)   │
│  - Initrd: initrd.img (boot.art, framework.jar, HALs)  │
│  - Guest HALs: gralloc / hwcomposer / egl (VirtIO-GPU) │
└──────────────────────────┬─────────────────────────────┘
                           │ VirtIO Rings & Binder IPC
                           ▼
┌────────────────────────────────────────────────────────┐
│        VirtIO-GPU & VirtIO-Binder Bridge               │
│  (crates/virtio_gpu_bridge, crates/virtio_binder)      │
│  - Control queue / wire packet demux                   │
│  - Handles 1, 2, 3, 4, 5, 10, 20, 30 routing           │
└────────────┬─────────────────────────────┬─────────────┘
             │                             │
             ▼                             ▼
┌──────────────────────────┐  ┌──────────────────────────┐
│  GraphicBufferProducer   │  │   System Services        │
│  (Handles 10, 20, 30)    │  │  (Handles 1, 2, 3, 4, 5) │
│ - Real wgpu::Texture     │  │  - PMS, AMS, WMS         │
│ - BufferQueue slots      │  │  - InputFlinger / Socket │
│ - wgpu Queue write       │  │  - SurfaceComposerService│
└────────────┬─────────────┘  └────────────┬─────────────┘
             │                             │
             └──────────────┬──────────────┘
                            ▼
┌────────────────────────────────────────────────────────┐
│      WebGPU Compositor & WebGpuSwapchain               │
│  (crates/webgpu_compositor, crates/webgpu_swapchain)   │
│  - Multi-layer composition & Z-ordering                │
│  - Single-set unified triple-buffered Offscreen target │
│  - GPU readback with pitch unpadding                   │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│      Browser Presentation & Offscreen Canvas           │
│  (src/raster_worker.js, index.html)                    │
│  - transferControlToOffscreen() WebGPU rasterization   │
│  - Accurate damage rect scissoring                     │
│  - Zero mock layers / true APK pixel presentation      │
└────────────────────────────────────────────────────────┘
```

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Real Guest Kernel & Initrd Artifacts | Build and validate x86 bzImage, initrd.img with boot.art & framework.jar | M1 | ORIGINAL_REQUEST §R1 |
| 2 | Real GraphicBufferProducer Bridge | Implement handles 10, 20, 30 with real GraphicBufferProducerService & wgpu texture allocation | M2 | ORIGINAL_REQUEST §R2 |
| 3 | Real System Services Wiring | Wire PMS (ServiceManager), AMS (Zygote/Activity), WMS (SurfaceBridge), InputFlinger (Socketpair) | M3 | ORIGINAL_REQUEST §R3 |
| 4 | Single-Set Offscreen Swapchain | Unified texture set for composition and readback in webgpu_swapchain | M4 | ORIGINAL_REQUEST §R4 |
| 5 | Real Browser Canvas & Rasterization | WebGPU rasterization on OffscreenCanvas, damage rects, remove mock DOM layers | M5 | ORIGINAL_REQUEST §R5 |
| 6 | E2E System & Workspace Test Pass | Complete pass of cargo test --workspace and node test suites with 0 failures | M6 | ORIGINAL_REQUEST §Acceptance Criteria |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Real Guest Build Artifacts | `guest/build/bzImage`, `guest/build/initrd.img`, `boot.art`, `framework.jar` | none | DONE |
| M2 | Real GraphicBufferProducer Bridge | `crates/virtio_gpu_bridge/src/bridge.rs` handles 10, 20, 30 with real `GraphicBufferProducerService` | none | DONE |
| M3 | Real System Services Wiring | Wire PMS, AMS, WMS, InputFlinger in `crates/virtio_gpu_bridge` and system service crates | M2 | DONE |
| M4 | Single-Set Offscreen Swapchain | `crates/webgpu_swapchain/src/swapchain.rs` unified triple-buffered offscreen target | M2, M3 | DONE |
| M5 | Real Browser Canvas & Rasterization | `index.html`, `src/raster_worker.js`, `src/v86_guest_manager.js` WebGPU canvas binding & mock removal | M1, M4 | DONE |
| M6 | Full Workspace & E2E Verification | Run full verification across `cargo test --workspace` and all 10 `node tests/*.mjs` suites | M1..M5 | IN_PROGRESS |

## Interface Contracts

### 1. VirtIO-GPU Bridge ↔ GraphicBufferProducerService (Handles 10, 20, 30)
- **Binder Descriptor**: `android.gui.IGraphicBufferProducer`
- **Transactions**:
  - `CONNECT (code 1)`: Returns status code 0 and connection token.
  - `DISCONNECT (code 2)`: Releases allocated slots.
  - `SET_BUFFER_COUNT (code 3)`: Resizes buffer slots vector.
  - `DEQUEUE_BUFFER (code 4)`: Returns slot index, fence, and buffer properties.
  - `QUEUE_BUFFER (code 5)`: Takes pixel bytes, writes to `wgpu::Texture` via `wgpu::Queue::write_texture`, signals frame available.
  - `CANCEL_BUFFER (code 6)`: Returns buffer slot to available pool.
  - `ALLOCATE_BUFFERS (code 7)`: Pre-allocates textures for slots.

### 2. WMS ↔ SurfaceComposerService (SurfaceBridge)
- **AIDL Descriptors**: `android.view.IWindowManager` ↔ `android.gui.ISurfaceComposer`
- **Transactions**:
  - `allocate_surface(title, width, height, format, flags)` -> Returns `SurfaceControl` with allocated producer handle.
  - `apply_transaction(tx)` -> Translates layer position, size, alpha, z-order into `ComposerState` and commits to `SurfaceComposerService`.

### 3. WebGpuSwapchain ↔ Browser Presentation
- **Format**: `wgpu::TextureFormat::Rgba8UnormSrgb`
- **Usages**: `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC | COPY_DST`
- **Readback Contract**: Dense RGBA unpadded buffer with length `width * height * 4`.
- **Canvas Presentation**: `OffscreenCanvas` bound via `GPUCanvasContext` in `src/raster_worker.js` with damage rect scissoring.

## Code Layout
- `guest/`: Linux kernel defconfig, initrd scripts, AOSP ART/DEX generators, guest HAL patches.
- `guest/build/`: `bzImage`, `initrd.img` binary artifacts.
- `crates/virtio_gpu_bridge/`: VirtIO-GPU command decoder and VirtIO-Binder router.
- `crates/surfaceflinger_gpu_service/`: `GraphicBufferProducerService` and `SurfaceComposerService`.
- `crates/pms_rs/`, `crates/ams_rs/`, `crates/wms_rs/`, `crates/inputflinger_rs/`: Native Android system services.
- `crates/webgpu_compositor/`, `crates/webgpu_swapchain/`: Multi-layer composition and offscreen swapchain.
- `src/`: Browser frontend scripts (`v86_guest_manager.js`, `raster_worker.js`, `apk_launcher.js`, etc.).
- `tests/`: Automated Node.js integration, adversarial fuzzer, and verification test suites.
