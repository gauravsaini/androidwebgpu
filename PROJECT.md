# Project: AndroidWebGPU Pure Guest Rendering

## Architecture
AndroidWebGPU executes guest Linux and Android userspace inside a WebAssembly v86 x86 VM with hardware-accelerated VirtIO-GPU graphics bridged to WebGPU:
- **VM Layer (`src/virtio_gpu_device.js`, `v86`)**: Emulates OASIS VirtIO-GPU 1.0/1.2 PCI device at BDF 0000:00:06.0 with BAR0 (I/O, 64B) and BAR1 (MMIO, 16MB). Intercepts notify kicks, parses virtqueue descriptor chains, handles BAR sizing/relocation, and raises CPU interrupts.
- **Guest Layer (`guest/initrd/init`, `guest/surfaceflinger.c`, `guest/app_process.c`, `guest/patches/`)**: Guest Linux kernel binds `virtio-pci` and `virtio-gpu`, creating `/dev/dri/card0` and `/dev/dri/renderD128`. Guest userspace (`init -> servicemanager -> zygote -> app_process -> SurfaceFlinger`) allocates DRM GEM buffers via gralloc and submits `TRANSFER_TO_HOST_2D` and `RESOURCE_FLUSH` via `DRM_IOCTL_VIRTGPU_EXECBUFFER`.
- **Bridge & Compositor Layer (`crates/virtio_gpu_bridge`, `crates/webgpu_compositor`)**: Rust/WASM bridge decodes virtqueue packets, resolves scatter-gather DMA memory backing, swizzles BGRX to RGBA, tracks damage bounding boxes, and blits to `<canvas id="screen" width="720" height="1440">`.
- **Gating & Runtime Layer (`src/android_runtime.js`, `src/system_bootstrap.js`)**: Transitions `guestActive=true` and `guestHasPresented=true` upon first guest frame, permanently locking out host rasterizer fallback.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | VirtIO PCI Config & Caps | Emulate BDF 0x06 PCI device with Vendor 0x1AF4, Device 0x1010, BAR0/BAR1, and VirtIO legacy I/O capabilities | M1 | survey_explorer_1 |
| 2 | BAR0/BAR1 Relocation & Sizing | Support BAR sizing probes (0xFFFFFFFF) and partial byte/word writes without corrupting ioBase | M1 | survey_explorer_1 |
| 3 | Driver Binding & Sysfs | Topological module load (12 modules) and sysfs driver binding (`/sys/bus/pci/drivers/virtio-pci/new_id`) | M1 | survey_explorer_1 |
| 4 | DRM Nodes Creation | Create and expose `/dev/dri/card0` and `/dev/dri/renderD128` without ENODEV | M1 | survey_explorer_1 |
| 5 | Guest Userspace Boot Pipeline | Boot sequence: `init -> servicemanager -> zygote -> app_process -> SurfaceFlinger` | M2 | survey_explorer_2 |
| 6 | DRM Gralloc GEM Allocations | Allocate GEM backing buffers via `DRM_IOCTL_VIRTGPU_RESOURCE_CREATE` + `DRM_IOCTL_VIRTGPU_MAP` | M2 | survey_explorer_2 |
| 7 | SurfaceFlinger Command Dispatch | Submit `RESOURCE_CREATE_2D`, `TRANSFER_TO_HOST_2D`, `RESOURCE_FLUSH` via `DRM_IOCTL_VIRTGPU_EXECBUFFER` | M2 | survey_explorer_2 |
| 8 | Virtqueue 0 Ring Processing | Hypervisor consumes Queue 0 descriptors, updates Used ring, raises IRQ 10 | M2 | survey_explorer_2 |
| 9 | Guest Gating & Fallback Lockout | Transition `guestActive=true`, `guestHasPresented=true`, and suppress host rasterizer | M2 | survey_explorer_2 |
| 10 | WebGPU VirtIO Scanout Blit | Deliver 720x1440 RGBA scanout framebuffer to `#screen` canvas via WebGPU / 2D context | M3 | survey_spec_miner_1 |
| 11 | BGRX to RGBA Swizzling | Swizzle Linux DRM BGRX to WebGPU RGBA with alpha=255 | M3 | survey_spec_miner_1 |
| 12 | Damage Rect Tracking | Track dirty bounding boxes on `RESOURCE_FLUSH` for partial canvas updating | M3 | survey_spec_miner_1 |
| 13 | Multi-Layer Compositing Engine | WGSL multi-layer compositor with matrix transforms and blend states | M3 | survey_spec_miner_1 |
| 14 | Shannon Entropy Telemetry | Validate non-black rendered display frames ($H \ge 1.0$) | M3 | survey_spec_miner_1 |
| 15 | E2E 4-Tier Test Suite | Comprehensive unit, integration, boundary, combinatorial, and scenario test matrix | M4 | survey_spec_miner_1 |
| 16 | Adversarial Hardening (Tier 5) | Adversarial stress testing, race condition checking, and memory integrity audits | M4 | survey_spec_miner_1 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | M1: VirtIO-GPU PCI Device Emulation & Driver Binding | VirtIO PCI config space, OASIS 1.0 caps, BAR0/BAR1 sizing & relocation, module loading & driver binding to `/dev/dri/card0` | None | **DONE** |
| 2 | M2: End-to-End Guest Graphics Stack Execution | Userspace boot (`init -> servicemanager -> zygote -> app_process -> SurfaceFlinger`), DRM GEM buffer allocation, VirtIO-GPU command dispatch, virtqueue 0 ring processing, and host fallback lockout (`guestActive=true`, `guestHasPresented=true`) | M1 | IN_PROGRESS |
| 3 | M3: WebGPU VirtIO Scanout Presentation | 720x1440 WebGPU canvas blit, BGRX swizzling, damage rect tracking, and Shannon entropy validation ($H \ge 1.0$) | M2 | PLANNED |
| 4 | M4: E2E Full-Stack Verification & Adversarial Hardening | 100% pass of 4-tier E2E tests (`pnpm test`, `run_e2e_tests.mjs`) + Tier 5 adversarial coverage hardening | M1, M2, M3 | PLANNED |

## Interface Contracts
### Guest Kernel / Userspace ↔ VirtIO-GPU PCI Device
- PCI BDF: `0000:00:06.0` (`0x1AF4:0x1010`, Subsystem `0x1AF4:0x0010`)
- BAR0: I/O Space (64 Bytes, `0xC140..0xC17F`), BAR1: MMIO Space (16 MB, `0xD1000000..0xD1FFFFFF`)
- Virtqueue 0: Size 256, Control Queue for 2D/3D command descriptors
- Virtqueue 1: Size 16, Cursor Queue
- IRQ: Line 10 (INTA#)

### VirtIO-GPU Device (`src/virtio_gpu_device.js`) ↔ WASM Bridge (`crates/virtio_gpu_bridge`)
- `process_virtqueue_descriptor(guestMem, descTableAddr, headDescIdx)` -> returns processed bytes / result status
- `get_scanout_framebuffer_rgba(scanoutId)` -> returns Uint8Array of $720 \times 1440 \times 4$ bytes
- `get_scanout_damage(scanoutId)` -> returns `[x, y, w, h]` or `null`

### Host Runtime Gating (`src/android_runtime.js`, `src/view_rasterizer.js`)
- `gpuDev.guestActive === true` and `gpuDev.guestHasPresented === true` -> `isHostInjectionAllowed() === false` -> disables `submitToVirtioGpu()` and host activity rasterizer.

## Code Layout
- `src/virtio_gpu_device.js` — VirtIO-GPU PCI and virtqueue hypervisor emulation
- `src/android_runtime.js` — Android runtime management and host fallback gating
- `src/system_bootstrap.js` — System bootstrap and WebGPU presentation loop
- `src/view_rasterizer.js` — Host rasterizer (gated out when guest presents)
- `crates/virtio_gpu_bridge/` — Rust/WASM VirtIO-GPU wire protocol, resource manager, and scanout renderer
- `crates/webgpu_compositor/` — WebGPU multi-layer composition pipeline and WGSL shaders
- `guest/initrd/init` — Guest Linux PID 1 initialization and module binding
- `guest/surfaceflinger.c` — Guest SurfaceFlinger display manager
- `guest/app_process.c` — Guest Zygote and Android application runner
- `guest/patches/` — Gralloc and HWC VirtIO-GPU drivers
- `tests/` — Test suites (unit, integration, adversarial, E2E)
