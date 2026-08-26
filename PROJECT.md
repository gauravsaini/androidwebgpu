# Project: Android WebGPU with Real x86 Linux Guest

## Architecture
- **Hypervisor Layer**: v86 x86-32 PC emulator running in WebAssembly with SeaBIOS, SeaVGABIOS, Linux kernel (`bzImage` / `linux4.iso`), and initrd.
- **I/O & IPC Layer**: Serial UART (`ttyS0`) streaming to logcat, Virtio-GPU PCI device (`0x1AF4:0x1050`), Android Binder IPC bridge (`binderfs`).
- **GPU & Framebuffer Bridge**: `virtio_gpu_bridge` WebAssembly bridge translating Virtio-GPU 2D/3D wire commands, scanout framebuffer updates, and damage rects.
- **Rendering & Compositing Layer**: `webgpu_compositor`, `webgpu_swapchain`, and `surfaceflinger_gpu_service` composing live guest OS framebuffer and Android SystemUI layers into a hardware-accelerated WebGPU canvas (`#screen`).
- **Telemetry & Logging**: Structured log emitter (`[v86]`, `[bridge]`, `[compositor]`) with live in-UI Android logcat streaming viewer.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Real v86 Boot & Lifecycle | `libv86.js` export harmonization, `./v86/v86.wasm` path fix, SeaBIOS/VGABIOS/ISO/bzImage boot, 9 lifecycle states, serial TTY milestone tracking | M1 | Survey 1, R1 |
| 2 | Server Security Headers | `serve.py` with COOP (`same-origin`), COEP (`require-corp`), CSP (`wasm-unsafe-eval`, `unsafe-eval`) | M1 | Survey 1, R1 |
| 3 | Structured Debug Logging | Unified logger emitting `[v86]`, `[bridge]`, `[compositor]` prefixes across JS and Rust layers | M2 | Survey 3, R2 |
| 4 | In-UI Logcat Streaming | Real-time formatted logcat streaming UI with priority filter (V/D/I/W/E), tag search, and 5000-line circular buffer | M2 | Survey 3, R2 |
| 5 | Virtio-GPU Framebuffer Bridge | OASIS Virtio 1.2 PCI device command processing, scanout framebuffer tracking, color channel mapping (BGRX/RGBA), damage rect calculation | M3 | Survey 2, R3 |
| 6 | Synthetic Placeholder Removal | Eliminate `queueAppBufferToSurfaceFlinger` synthetic canvas drawing (`"Synthetic placeholder — awaiting guest rendering"`), mock overlays, and fake boot simulation | M3 | Survey 2, R3 |
| 7 | WebGPU Compositor Live Pixels | Pipe live guest framebuffer pixels into WebGPU textures (`queue.write_texture` with dirty damage bounds) and composite with SystemUI layers | M4 | Survey 2, R3 |
| 8 | Opaque-Box E2E Test Suite | 4-tier requirement-driven E2E test suite (Tiers 1-4) with automated runner generating `TEST_READY.md` | M-TEST | Survey 3, R4 |
| 9 | E2E Pass & Adversarial Hardening | 100% pass of E2E test suite (Tiers 1-4) followed by Tier 5 adversarial stress testing, bug fixes, and clean Forensic Audit | M5 | Survey 1-3, R4 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M-TEST | E2E Testing Track | Requirement-driven test harness and test cases (Tiers 1-4) publishing `TEST_READY.md` | None | DONE |
| M1 | Hypervisor & v86 Boot Pipeline | Real v86 guest booting, BIOS/kernel/ISO loading, server headers, serial UART | None | DONE |
| M2 | Structured Debug Logging & In-UI Logcat | Standardized `[v86]`, `[bridge]`, `[compositor]` logging across all modules, live in-UI logcat streaming console with filtering & circular buffer | M1 | DONE |
| M3 | Virtio-GPU & Framebuffer Bridge | Wire protocol, scanout damage buffer, elimination of synthetic placeholders | M1, M2 | PLANNED |
| M4 | WebGPU Compositor & Live Guest Pixel Rendering | Multi-layer compositor, texture upload, SystemUI composition, unified WebGPU viewport | M3 | PLANNED |
| M5 | E2E Verification & Adversarial Hardening | Phase 1: 100% E2E test pass (Tiers 1-4); Phase 2: Tier 5 white-box adversarial coverage hardening & Forensic Audit | M-TEST, M4 | PLANNED |

## Interface Contracts
### `v86_guest_manager` ↔ `v86` Runtime
- Global constructor: `window.V86Starter = window.V86Starter || window.V86`
- Configuration options: `bios: { url: './bios/seabios.bin' }`, `vga_bios: { url: './bios/vgabios.bin' }`, `cdrom: { url: './guest/build/linux4.iso' }`, `wasm_path: './v86/v86.wasm'`
- Serial listener: `emulator.add_listener('serial0-output-char', (char) => handleSerialChar(char))`
- Lifecycle callbacks: `onStateChange(newState)`, `onMilestone(milestoneName)`, `onError(error)`

### `virtio_gpu_device` ↔ `WasmVirtioGpuBridge` ↔ `WebGpuCompositor`
- Command queue transfer: `bridge.process_control_queue(cmd_bytes: Uint8Array) -> Uint8Array`
- Scanout buffer retrieval: `bridge.get_scanout_framebuffer(scanout_id: u32) -> Uint8Array`
- Scanout damage rect: `bridge.get_scanout_damage(scanout_id: u32) -> { x: u32, y: u32, width: u32, height: u32 }`
- WebGPU texture upload: `queue.write_texture({ texture: guestLayerTexture, origin: { x, y } }, damage_bytes, { bytesPerRow, rowsPerImage }, { width, height })`

### Logging Abstraction ↔ UI Streaming
- Emitter API: `logDebug(subsystem: 'v86' | 'bridge' | 'compositor', level: 'V' | 'D' | 'I' | 'W' | 'E', message: string, metadata?: object)`
- Prefix format: `[v86]`, `[bridge]`, `[compositor]`
- Logcat UI listener: `appendLogcat(tag, message, priority)` with circular buffer (max 5000 entries) and filter predicates.

## Code Layout
- `v86/`: v86 runtime assets (`libv86.js`, `v86.wasm`)
- `bios/`: BIOS ROM images (`seabios.bin`, `vgabios.bin`)
- `guest/`: Guest kernel, initrd scripts, defconfigs, tools
- `src/`: JavaScript core modules (`v86_guest_manager.js`, `virtio_gpu_device.js`, `android_runtime.js`, `binder_test_suite.js`, `logger.js`)
- `crates/`: Rust crates (`webgpu_compositor`, `webgpu_swapchain`, `surfaceflinger_gpu_service`, `virtio_gpu_bridge`, `binder_sys`, `ams_rs`, etc.)
- `tests/`: Unit, integration, challenger, and E2E test suites
- `serve.py`: Dev HTTP server with security headers
- `index.html` & `android.html`: Web application entrypoints
