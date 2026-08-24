# Android WebGPU Stack 🚀

> **A Rust + WebGPU virtualization stack backing Android-x86 OpenGL ES, EGL, and SurfaceFlinger rendering for in-browser execution with full Unity, Unreal, and Godot APK support.**

---

## 🏗 Architecture

```text
┌───────────────────────────────────────────────────────────────┐
│                        Browser (Host)                         │
│                                                               │
│   ┌───────────────────────────────────────────────────────┐   │
│   │                 Rust / WebGPU Engine                  │   │
│   │                                                       │   │
│   │  ┌──────────────┐   ┌─────────────────────────────┐  │   │
│   │  │ gles2wgpu    │   │ compositor                  │  │   │
│   │  │ (GLES→WebGPU)│   │ (SurfaceFlinger-equivalent) │  │   │
│   │  └─────┬────────┘   └──────────────┬──────────────┘  │   │
│   │        │                           │                 │   │
│   │  ┌─────▼────────┐   ┌──────────────▼──────────────┐  │   │
│   │  │ virtio-gpu   │   │ swapchain / canvas          │  │   │
│   │  │ bridge       │   │ (WebGPU surface)            │  │   │
│   │  └─────┬────────┘   └──────────────┬──────────────┘  │   │
│   └────────┼───────────────────────────┼──────────────────┘   │
│            │                           │                      │
│   ┌────────▼───────────────────────────▼──────────────────┐   │
│   │                      v86 (Wasm)                       │   │
│   │                                                       │   │
│   │  CPU emu │ RAM │ virtio-blk │ virtio-net │ virtio-gpu │   │
│   └──────────┴─────┴───────────┴────────────┴────────────┘   │
└───────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────┐
│                      Guest: Android-x86                       │
│                                                               │
│  App → GLES → EGL → libGLES (patched) → virtio-gpu driver →   │
│  hwcomposer (patched) → SurfaceFlinger → buffers → host       │
└───────────────────────────────────────────────────────────────┘
```

---

## 🧱 Workspace Crates & Modules

| Crate / Directory | Role | Description |
|---|---|---|
| [`crates/gles2wgpu`](crates/gles2wgpu) | GLES → WebGPU Translator | GL state machine, multi-VBO attribute layout, uniform reflection, and Naga GLSL-to-WGSL compiler. |
| [`crates/virtio_gpu_bridge`](crates/virtio_gpu_bridge) | Virtio-GPU Command Bridge | OASIS Virtio 1.2 protocol decoder (2D/3D command stream, scanout framebuffer blitting, Wasm FFI). |
| [`crates/webgpu_compositor`](crates/webgpu_compositor) | SurfaceFlinger Equivalent | Multi-plane quad compositor with matrix transformations, damage rects, and alpha blending. |
| [`crates/webgpu_swapchain`](crates/webgpu_swapchain) | Swapchain & Presentation | WebGPU canvas integration, VSync timing, and headless pixel readback. |
| [`crates/apk_gpu_analyzer`](crates/apk_gpu_analyzer) | APK Engine Scanner | Inspects Android APK manifests and native libraries to detect Unity, Unreal, and Godot engines. |
| [`crates/metrics_overlay`](crates/metrics_overlay) | Diagnostics HUD | Real-time FPS counter, frame time, draw call tracker, and GPU memory overlay. |
| [`guest/patches/`](guest/patches) | Guest AOSP HAL Drivers | Concrete C++ implementations for Android `gralloc`, `hwcomposer`, and `egl` over virtio-gpu. |
| [`src/`](src) | Browser Virtio Glue | JavaScript PCI virtio-gpu adapter (`virtio_gpu_device.js`) and binary packet builder (`virtio_packet_builder.js`). |

---

## 🧪 Verification Gates

- [x] **Gate 1: Virtio 2D Scanout & Flush** (`RESOURCE_CREATE_2D` + `TRANSFER_2D` + `RESOURCE_FLUSH` with pixel verification)
- [x] **Gate 2: Virtio 3D Submit GLES** (`SUBMIT_3D` command stream with viewport & clear color rasterization)
- [x] **Gate 3: Multi-Layer Compositor & HUD** (Multi-plane quad composition with top status HUD overlay)
- [x] **Gate 4: Real APK Flight Stream** (Stream playback from `unity_cube.apk` and `godot_gles2.apk`)

---

## 🚀 Getting Started

### Prerequisites

- [Rust Toolchain](https://rustup.rs/) (1.85+)
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) (for browser Wasm builds)
- [`uv`](https://docs.astral.sh/uv/) (for running local HTTP server)

### 1. Run Automated Test Suite

```bash
# Run all workspace unit and integration tests
cargo test --workspace

# Run full-stack GLES -> Virtio -> Compositor -> Swapchain test
cargo test -p gles2wgpu --test e2e_full_stack -- --nocapture

# Run real APK flight test harness (Unity & Godot)
cargo test -p apk_gpu_analyzer --test apk_real -- --nocapture
```

### 2. Build WebAssembly Package

```bash
cd crates/virtio_gpu_bridge
wasm-pack build --target web --out-dir ../../pkg -- --features wasm
cd ../..
```

### 3. Launch Interactive Visual Test Bench

```bash
# Start local server
uv run python -m http.server 8089
```

Open in a WebGPU-supported browser (Google Chrome / Microsoft Edge):
👉 **`http://localhost:8089/index.html`**

- Runs automated visual assertions across all 4 gates.
- Displays live pixel inspector and execution logs.
- Includes **"▶ Start Real-Time 60 FPS Loop"** for continuous streaming tests.

---

## 🔗 Upstream Architecture References

- **[Google ANGLE](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/libANGLE/renderer/wgpu/)**: WebGPU backend and OpenGL ES state machine.
- **[Google crosvm / Rutabaga GFX](https://github.com/google/crosvm/tree/main/rutabaga_gfx)**: Rust paravirtualized GPU abstraction.
- **[AOSP goldfish-opengl](https://android.googlesource.com/platform/device/generic/goldfish-opengl/)**: Guest GLES/EGL stream encoder.
- **[Mesa virglrenderer](https://gitlab.freedesktop.org/virgl/virglrenderer)**: 3D Virtio-GPU command parser and executor.
- **[copy/v86](https://github.com/copy/v86)**: WebAssembly x86 PC emulator.
- **[OASIS Virtio Spec v1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/csd01/virtio-v1.2-csd01.html#x1-3440007)**: Formal Virtio-GPU specification.

---

## 📄 License

Apache-2.0
