# Project: Android WebGPU Rendering Pipeline

## Architecture
The Android WebGPU rendering pipeline implements an authentic, zero-mock 8-stage graphics pipeline connecting Android guest execution to an HTML5 WebGPU/Canvas presentation surface:

```
[Target APK (F-Droid / Firefox)]
               │
               ▼ Stage 1: APK Ingestion & PMS (apk_client_parser.js, dex_vm.js, pms_rs)
[View Tree & XML AST Layout]
               │
               ▼ Stage 2: View Hierarchy (view_hierarchy.js, MeasureSpec, LayoutParams)
[HWUI / Skia RenderNodes & GraphicBuffers]
               │
               ▼ Stage 3: HWUI Rasterizer & GraphicBufferQueue (view_rasterizer.js, buffer_queue.rs)
[SurfaceFlinger DRM Composition]
               │
               ▼ Stage 4: SurfaceFlinger & Layer Matrix Translation (guest/surfaceflinger.c, service.rs)
[VirtIO-GPU Virtqueue 0 (TRANSFER_TO_HOST_2D & RESOURCE_FLUSH)]
               │
               ▼ Stage 5: VirtIO-GPU Device Emulation (virtio_gpu_device.js, PCI 00:06.0, IRQ 10)
[Rust WASM Bridge (DMA Backing, Damage Rects, BGRX->RGBA Swizzle)]
               │
               ▼ Stage 6: Rust Bridge (crates/virtio_gpu_bridge, bridge.rs, wasm.rs)
[WebGPU Compositor (WGSL Pipeline, Cached Layer Textures, Swapchain)]
               │
               ▼ Stage 7: WebGPU Compositor (crates/webgpu_compositor, crates/webgpu_swapchain)
[HTML5 Screen Canvas Presentation (<canvas id="screen" width="720" height="1440">)]
               │
               ▼ Stage 8: System Presentation (system_bootstrap.js, android.html)
```

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | APK Ingestion & Extraction | Pure-JS DEFLATE & ZIP reader extracting classes.dex, resources.arsc, AndroidManifest.xml | M1 | Survey |
| 2 | AXML Decoder & ARSC Resolver | Binary XML manifest decoding and resource table resolution | M1 | Survey |
| 3 | Dalvik Multi-DEX VM | Bytecode parsing, method resolution, instruction dispatch for APK classes | M1 | Survey |
| 4 | PackageManagerService | Package metadata, permissions, intent filters on Binder `"package"` | M1 | Survey |
| 5 | View Hierarchy & Layout | MeasureSpec bitmask, LayoutParams, ViewGroup measure/layout traversal | M1 | Survey |
| 6 | HWUI RenderNodes & Rasterization | Skia recording traversal, dirty rect accumulation, GraphicBuffer rendering | M1 | Survey |
| 7 | GraphicBufferQueue | `IGraphicBufferProducer` buffer slot lifecycle (DEQUEUE, QUEUE, ALLOCATE) | M1 | Survey |
| 8 | SurfaceFlinger Service | Multi-surface Z-ordering, NDC matrix translation, display config (720x1440) | M2 | Survey |
| 9 | DRM Primary Plane (`/dev/dri/card0`) | In-guest DRM buffer allocation and scanout binding | M2 | Survey |
| 10 | VirtIO-GPU PCI Configuration | PCI BDF 00:06.0, BAR0 I/O (0xC140), BAR1 MMIO (0xD1000000), IRQ 10 | M2 | Survey |
| 11 | Virtqueue 0 Ring Processing | OASIS 1.2 descriptor ring, available/used ring processing, 16-bit wrap | M2 | Survey |
| 12 | `TRANSFER_TO_HOST_2D` Command | Subrectangle pixel transfer with inline and scatter-gather DMA backing | M2 | Survey |
| 13 | `RESOURCE_FLUSH` Command | Scanout damage rect computation, clipped boundary blitting, fence return | M2 | Survey |
| 14 | Rust WASM Bridge | WebAssembly bridge parsing virtqueue descriptors and managing host resources | M3 | Survey |
| 15 | BGRX to RGBA Swizzling | DRM BGRX channel swap to RGBA for canvas/WebGPU compatibility | M3 | Survey |
| 16 | Damage Rect Extraction | Dirty bounding box calculation (`get_scanout_damage`) for partial blits | M3 | Survey |
| 17 | Permanent Fallback Lockout | `guestHasPresented` transition on first guest flush; permanent drop of host synthetic frames | M3 | Survey |
| 18 | WebGPU Pipeline Shaders | WGSL vertex and fragment shaders rendering scanout textures | M4 | Survey |
| 19 | WebGPU Swapchain & Transforms | Triple-buffered swapchain presentation and layer rotation/flip matrices | M4 | Survey |
| 20 | HTML5 Canvas Presentation | 60/120 Hz render loop presenting RGBA scanout to `<canvas id="screen">` | M4 | Survey |
| 21 | 4-Tier E2E Test Suite | 82 automated test cases spanning Tiers 1-4 (Features, Boundaries, Pairs, Real Apps) | M5 | Survey |
| 22 | Headless Browser Validation | Puppeteer validation of live canvas, Shannon entropy $H \ge 2.0$, unique colors $\ge 50$ | M5 | Survey |
| 23 | Tier 5 Adversarial Coverage Hardening | White-box adversarial edge-case generation and verification | M5 | Survey |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | APK, View Tree & HWUI Pipeline | Stages 1-3: APK parsing, Dalvik VM, View Hierarchy, HWUI & GraphicBuffers | None | DONE |
| 2 | SurfaceFlinger & VirtIO-GPU Virtqueue | Stages 4-5: SurfaceFlinger DRM composition, PCI registers, Virtqueue 0 rings, TRANSFER_2D/FLUSH | M1 | DONE |
| 3 | Rust Bridge, Swizzle & Fallback Lockout | Stage 6 & Lockout: Rust WASM bridge, BGRX->RGBA swizzle, damage rects, `guestHasPresented` permanent lockout | M2 | DONE |
| 4 | WebGPU Compositor & Canvas Presentation | Stages 7-8: WGSL pipeline, WebGPU swapchain, scanout texture binding, `<canvas id="screen">` presentation | M3 | DONE |
| 5 | E2E Verification & Adversarial Hardening | Phase 1 (Tiers 1-4 pass + validate_browser.mjs) & Phase 2 (Tier 5 Adversarial Hardening) | M1, M2, M3, M4 | DONE |

## Interface Contracts
### APK / PMS ↔ View Tree
- Input: Parsed layout XML AST and DEX class definitions.
- Output: Root `ViewGroup` / `View` tree instantiated with `LayoutParams` and attributes.
- Error handling: Malformed XML or missing attributes fallback to default view bounds.

### View Tree ↔ HWUI / GraphicBuffers
- Input: Measured and laid out `View` hierarchy.
- Output: `GraphicBuffer` slots queued via `IGraphicBufferProducer.queueBuffer()`.
- Error handling: Null view returns 0-sized damage without rendering errors.

### SurfaceFlinger ↔ VirtIO-GPU
- Input: Composited layers mapped to DRM primary plane framebuffer (`/dev/dri/card0`).
- Output: VirtIO-GPU control packets (`VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D`, `VIRTIO_GPU_CMD_RESOURCE_FLUSH`).
- Error handling: Out-of-bounds coordinates clamped to display bounds (720x1440).

### VirtIO-GPU ↔ Rust WASM Bridge
- Input: Guest physical memory addresses, descriptor tables, head descriptor indices.
- Output: Swizzled RGBA pixel buffer (`get_scanout_framebuffer_rgba(0)`) and dirty damage rectangle (`get_scanout_damage(0)`).
- Error handling: Invalid resource ID or corrupted descriptor returns `VIRTIO_GPU_RESP_ERR_*` and resets descriptor chain safely.

### Rust WASM Bridge ↔ WebGPU Compositor / Canvas
- Input: 720x1440 RGBA framebuffer and dirty damage rect `[x, y, w, h]`.
- Output: GPU texture upload via `wgpu::Queue::write_texture` and canvas blit via `ctx2d.putImageData`.
- Error handling: Missing canvas skips blit safely without unhandled exception.

## Code Layout
- `src/apk_client_parser.js`, `src/dex_vm.js`: APK ingestion, DEX bytecode execution.
- `src/view_hierarchy.js`, `src/view_rasterizer.js`: View tree layout and HWUI rasterizer.
- `src/virtio_gpu_device.js`, `src/virtio_packet_builder.js`: VirtIO-GPU PCI device and virtqueue processing.
- `crates/virtio_gpu_bridge/`: Rust WASM bridge, virtqueue descriptor decoding, BGRX swizzling.
- `crates/surfaceflinger_gpu_service/`: SurfaceFlinger service and BufferQueue management.
- `crates/webgpu_compositor/`, `crates/webgpu_swapchain/`: WebGPU WGSL pipeline and swapchain.
- `src/system_bootstrap.js`, `android.html`: HTML5 Canvas presentation harness.
- `tests/run_e2e_tests.mjs`, `validate_browser.mjs`: Automated verification suites.
