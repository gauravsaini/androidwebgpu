# Project: AndroidWebGPU

## Architecture
- **Browser Shell**: `index.html` fail-closed UI, canvas host, event listeners, dynamic import of `./pkg/android_vm.js`.
- **WASM Runtime Engine (`pkg/android_vm.js`, `pkg/android_vm_bg.wasm`)**: Production entry point exposing `initWasm()` and `createAndroidRuntime()`.
- **VM Core**: `src/vm/` x86 CPU emulator inside Web Worker (`vm_worker.js`), `GuestMem` checked physical memory views.
- **Virtio Transport**: `src/virtio/` PCI configuration space (I/O ports 0xCF8/0xCFC), `Virtqueue` descriptor parser and rings, `IrqController`.
- **Guest I/O Devices**: `src/io/` and `src/storage/` (virtio-blk with OPFS/IndexedDB, virtio-console, virtio-input, virtio-net, virtio-rng, RTC clock, virtio-sound).
- **GPU Pipeline**: `crates/virtio_gpu_bridge` DMA backing memory management, fence completion, `crates/webgpu_swapchain` WebGPU canvas presentation without `putImageData`, `crates/webgpu_compositor` layer composition, `crates/gles2wgpu` 3D rendering.
- **Android Guest OS**: `images/` verified boot images (`manifest.json`, `kernel`, `initrd.img`, `system.img`, `vendor.img`, `product.img`).
- **Validation Engine**: Truth-based dynamic evaluation of Gates G0–G9 reporting immutable JSON records.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | F-VM-01 | v86 x86 VM Web Worker runner & lifecycle | M1 | survey_2 |
| 2 | F-VM-02 | Bounds-checked `GuestMem` DMA view | M1 | survey_2 |
| 3 | F-VIO-01 | Virtio PCI bus & configuration space router | M1 | survey_2 |
| 4 | F-VIO-02 | Virtqueue split ring buffer parser & descriptor validation | M1 | survey_2 |
| 5 | F-VIO-03 | Virtio IRQ assertion & latched status management | M1 | survey_2 |
| 6 | F-DEV-01 | VirtioBlk device with OPFS/IndexedDB persistence | M2 | survey_2 |
| 7 | F-DEV-02 | VirtioConsole device with serial log streaming | M2 | survey_2 |
| 8 | F-DEV-03 | VirtioInput device with DOM to evdev translation | M2 | survey_2 |
| 9 | F-DEV-04 | VirtioNet user-mode proxy network device | M2 | survey_2 |
| 10 | F-DEV-05 | VirtioRng entropy device via crypto.getRandomValues | M2 | survey_2 |
| 11 | F-DEV-06 | CMOS RTC & monotonic clock device | M2 | survey_2 |
| 12 | F-DEV-07 | VirtioSound audio output device | M2 | survey_2 |
| 13 | F-GPU-01 | Virtio-GPU DMA backing attach/detach | M3 | survey_3 |
| 14 | F-GPU-02 | True spec TRANSFER_TO_HOST_2D from guest physical RAM | M3 | survey_3 |
| 15 | F-GPU-03 | Direct WebGPU canvas surface present (strictly no putImageData) | M3 | survey_3 |
| 16 | F-GPU-04 | 3D opcode slice fix & GLES translation hookup | M3 | survey_3 |
| 17 | F-GPU-05 | Monotonic fence completion & async work synchronization | M3 | survey_3 |
| 18 | F-GPU-06 | WebGPU device loss and context recovery | M3 | survey_3 |
| 19 | F-GST-01 | Android guest boot media & images/manifest.json | M4 | survey_3 |
| 20 | F-GST-02 | Android guest HAL fixes (egl, gralloc, hwcomposer) | M4 | survey_3 |
| 21 | F-BLD-01 | Runtime artifact packaging (pkg/android_vm.js, wasm) | M4 | survey_1 |
| 22 | F-BLD-02 | Build scripts & tooling (package.json, build scripts) | M5 | survey_1 |
| 23 | F-VAL-01 | Dynamic truth-based window.runValidationLoop() evaluating G0-G9 | M5 | survey_1 |
| 24 | F-VAL-02 | Runtime artifact acceptance (verify-runtime-artifacts.mjs exit 0) | M5 | survey_1 |
| 25 | F-VAL-03 | Fail-closed HTTP bootstrap and UI state reporting | M5 | survey_1 |
| 26 | F-TST-01 | E2E opaque-box test runner and harness | M-E2E | requirements |
| 27 | F-TST-02 | Tier 1 Feature Coverage tests (>=5 per feature) | M-E2E | requirements |
| 28 | F-TST-03 | Tier 2 Boundary & Corner Case tests (>=5 per feature) | M-E2E | requirements |
| 29 | F-TST-04 | Tier 3 Pairwise Combinatorial tests | M-E2E | requirements |
| 30 | F-TST-05 | Tier 4 Real-World Application scenarios | M-E2E | requirements |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | VM Core & Virtio Transport Subsystem | `src/vm/**`, `src/boot/**`, `src/virtio/**` | none | BLOCKED (fail-closed start, v86 adapter + live 32-bit kernel boot proven; needs guest I/O + frames) |
| M2 | Guest I/O & Storage Subsystem | `src/io/**`, `src/storage/**` | M1 | PLANNED |
| M3 | WebGPU-Backed Virtio-GPU Display Pipeline | `crates/virtio_gpu_bridge/**`, `crates/webgpu_swapchain/**`, `crates/webgpu_compositor/**`, `src/gpu_transport/**` | M1 | PLANNED |
| M4 | Android Boot Media & Guest Images | `images/**`, `guest/patches/**` | M1, M2, M3 | BLOCKED (authentic 32-bit kernel+initrd pinned + booting; system/vendor/product squashfs pending) |
| M5 | Production Runtime & Fail-Closed Validation Loop | `pkg/**`, `src/validation/**`, `scripts/**` | M1, M2, M3, M4, M-E2E | PLANNED |
| M-E2E | E2E Opaque-Box Testing Track | `tests/e2e/**`, `TEST_INFRA.md`, `TEST_READY.md` | none (parallel) | GATED GREEN 62/62 production-backed (live-guest suites pending media) |

## Interface Contracts

### VmRuntime ↔ VirtioBus
- `VmRuntime` creates guest physical RAM and exposes `GuestMem`.
- `VirtioBus` registers with `PciBus` at I/O ports `0xCF8`/`0xCFC`.
- IRQs asserted via `IrqController.assertIrq(pciIrq)` and latched in `ISRStatus`.
- Reading `ISRStatus` de-asserts PCI IRQ line.

### VirtioBus ↔ Guest Devices (Blk, Console, Input, Net, Rng, RTC, GPU)
- Devices inherit from `VirtioPciDevice`.
- Expose modern capabilities (`COMMON_CFG`, `NOTIFY_CFG`, `ISR_CFG`, `DEVICE_CFG`) and legacy BAR0 registers.
- Virtqueue descriptor chains parsed with loop and bounds validation; exactly one used entry written per consumed chain.

### VirtioGpu ↔ WebGpuSwapchain / Canvas
- Queue 0 (Control Queue) commands decoded via `crates/virtio_gpu_bridge`.
- `RESOURCE_ATTACH_BACKING` maps guest physical memory ranges (`VirtioGpuMemEntry`) into host texture staging buffers.
- `TRANSFER_TO_HOST_2D` reads pixel bytes from guest DMA memory (not inline packet payload).
- Presentation renders scanout texture directly to `GPUCanvasContext` surface texture without `putImageData`.
- Device loss handled by re-creating WebGPU device/textures while preserving guest VM RAM.

### AndroidRuntime ↔ Browser Entry Point (`index.html`)
- Export `default function initWasm(): Promise<void>;`
- Export `function createAndroidRuntime(input: { canvas: HTMLCanvasElement, onEvent: (event: RuntimeEvent) => void }): Promise<AndroidRuntime>;`
- `AndroidRuntime.runValidationLoop({ runId, epoch, onGate })` evaluates Gates G0 through G9 dynamically, reporting `{ status: 'PASSED'|'FAILED'|'BLOCKED', evidence: string[], error: string|null }`.
- Sets `ready: true` ONLY when G0–G9 all pass in the same epoch.

## Code Layout
- `pkg/`: Generated production runtime bundle (`android_vm.js`, `android_vm_bg.wasm`).
- `src/vm/`: x86 VM coordinator, Web Worker, and bounds-checked guest memory.
- `src/virtio/`: PCI configuration space, virtqueue descriptor engine, and IRQ controller.
- `src/io/`: Virtio device drivers (block, console, input, net, rng, clock, sound).
- `src/storage/`: OPFS and IndexedDB persistence actors.
- `src/gpu_transport/`: JS bridge to native virtio-gpu WebGPU pipeline.
- `src/validation/`: Truth-based dynamic validation loop and gate evaluator.
- `images/`: Pinned Android guest boot media (`manifest.json`, `kernel`, `initrd.img`, `system.img`, `vendor.img`, `product.img`).
- `crates/`: Rust crates (`virtio_gpu_bridge`, `webgpu_swapchain`, `webgpu_compositor`, `gles2wgpu`, `apk_gpu_analyzer`, `metrics_overlay`).
- `tests/e2e/`: Opaque-box E2E test suites (Tiers 1-4).
