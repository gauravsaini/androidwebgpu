## 🎯 **PRIMARY GOAL**

> **Android-x86 ke GPU stack ko WebGPU se back karna, taaki modern GPU‑based APKs browser me chal sakein — including Unity/Unreal/Godot games.**

Core idea:

- Android ke **GLES / EGL / SurfaceFlinger / hwcomposer / gralloc** pipeline ko **intercept** karna  
- v86 sirf CPU/virtio handle kare  
- GPU ka kaam **Rust + WebGPU** kare  
- Architecture **real Android GPU pipeline jaisa hi ho**, bas backend WebGPU ho

---

# 🧩 **ARCHITECTURE DIAGRAM (UPDATED)**

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

# 🧱 **RUST WORKSPACE STRUCTURE & UPSTREAM MAPPINGS**

```text
gpu-android/
  Cargo.toml
  crates/
    gles2wgpu/              # GLES → WebGPU translator (Reference: ANGLE src/libANGLE/renderer/wgpu)
    virtio_gpu_bridge/      # v86 ↔ Rust GPU command bridge (Reference: crosvm rutabaga_gfx, virglrenderer)
    webgpu_compositor/      # SurfaceFlinger-like compositor (Reference: Android RenderEngine, Smithay)
    webgpu_swapchain/       # WebGPU surface + canvas integration (Reference: wgpu::Surface, web-sys)
    apk_gpu_analyzer/       # optional: GPU feature analyzer
    metrics_overlay/        # optional: FPS, GPU time, debug UI (Reference: egui-wgpu)
```

---

# 🪜 **STEP-BY-STEP TODO & REAL IMPLEMENTATION REFERENCES**

---

## ✅ **STEP 1 — GLES → WebGPU Translator POC**

### 🎯 Goal  
Host side me ek GLES demo WebGPU se chal jaye.

### 📝 TODO  
- `gles2wgpu` crate bootstrap  
- GL state machine implement  
- GLSL → WGSL conversion (Naga / glslang / ANGLE translator)  
- Basic GL calls implement:
  - `glClear`
  - `glDrawArrays`
  - `glBindTexture`
- Rust + wgpu test harness

### 🔗 Real Implementation References  
- **[Google ANGLE WebGPU Backend](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/libANGLE/renderer/wgpu/)**: ANGLE's native OpenGL ES to WebGPU/Dawn translation layer.
  - [ContextWgpu.cpp](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/libANGLE/renderer/wgpu/ContextWgpu.cpp): State machine management and pipeline generation.
  - [FramebufferWgpu.cpp](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/libANGLE/renderer/wgpu/FramebufferWgpu.cpp) & [TextureWgpu.cpp](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/libANGLE/renderer/wgpu/TextureWgpu.cpp): Resource translation to WebGPU views.
  - [ANGLE Shader Translator](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/compiler/translator/): GLSL compilation and AST transformations.
- **[gfx-rs / Naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga)**: Rust shader translation engine.
  - [Naga GLSL Front-end](https://github.com/gfx-rs/wgpu/tree/trunk/naga/src/front/glsl): Parses GLSL vertex/fragment shaders into Naga IR.
  - [Naga WGSL Back-end](https://github.com/gfx-rs/wgpu/tree/trunk/naga/src/back/wgsl): Emits valid WGSL from Naga IR.
- **[gfx-rs / wgpu](https://github.com/gfx-rs/wgpu)**: Idiomatic Rust WebGPU implementation targeting browser WebGPU and native backends.
- **[Google Dawn](https://dawn.googlesource.com/dawn)**: Chromium's WebGPU implementation in C++ ([src/dawn/native](https://dawn.googlesource.com/dawn/+/refs/heads/main/src/dawn/native/)).

### ✔ Success  
GLES → WebGPU engine working.

---

## ✅ **STEP 2 — Virtio-GPU Protocol Design**

### 🎯 Goal  
Guest Android ke GPU commands host Rust engine tak reliably aa jayein.

### 📝 TODO  
- Command types define:
  - `CreateContext`
  - `CreateBuffer`
  - `UploadBuffer`
  - `Draw`
  - `Present`
- Binary protocol define  
- SharedArrayBuffer + postMessage bridge  
- `virtio_gpu_bridge` crate implement

### 🔗 Real Implementation References  
- **[OASIS Virtio 1.2 GPU Specification](https://docs.oasis-open.org/virtio/virtio/v1.2/csd01/virtio-v1.2-csd01.html#x1-3440007)**: Formal specification for 2D/3D GPU virtio devices (`VIRTIO_GPU_CMD_CTX_CREATE`, `VIRTIO_GPU_CMD_RESOURCE_CREATE_2D/3D`, `VIRTIO_GPU_CMD_SUBMIT_3D`, `VIRTIO_GPU_CMD_RESOURCE_FLUSH`).
- **[Google crosvm / Rutabaga GFX](https://github.com/google/crosvm/tree/main/rutabaga_gfx)**: Production Rust paravirtualized GPU abstraction supporting virtio-gpu, Virgl, Gfxstream, and WebGPU contexts.
  - [rutabaga_core.rs](https://github.com/google/crosvm/blob/main/rutabaga_gfx/src/rutabaga_core.rs): Context creation, resource mapping, and command dispatch.
  - [crosvm virtio-gpu device](https://github.com/google/crosvm/tree/main/devices/src/virtio/gpu): Rust implementation of the virtio-gpu device interface.
- **[Mesa / virglrenderer](https://gitlab.freedesktop.org/virgl/virglrenderer)**: Library for decoding guest 3D commands and executing them on host GPU.
  - [vrend_decode.c](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/master/src/vrend_decode.c): Command stream packet parser.
- **[Google / Mesa gfxstream](https://android.googlesource.com/platform/hardware/google/gfxstream/)** / **[gitlab mirror](https://gitlab.freedesktop.org/mesa/gfxstream)**: Stream forwarding protocol for Android guest-to-host graphics.
- **[AOSP goldfish-opengl wire protocol](https://android.googlesource.com/platform/device/generic/goldfish-opengl/+/refs/heads/master/system/GLESv2_enc/)**: Auto-generated GLES wire encoder used by Android Emulator.

### ✔ Success  
Host side `handle_command(GpuCommand)` stable.

---

## ✅ **STEP 3 — Android-x86 GPU Stack Mapping**

### 🎯 Goal  
Android-x86 GPU call flow fully understood.

### 📝 TODO  
- Study:
  - SurfaceFlinger
  - hwcomposer
  - gralloc
  - Mesa llvmpipe
- Identify:
  - GLES entrypoints
  - Buffer allocation
  - Composition flow

### 🔗 Real Implementation References  
- **[AOSP SurfaceFlinger](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/master/services/surfaceflinger/)**: Android system compositor.
  - [SurfaceFlinger.cpp](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/master/services/surfaceflinger/SurfaceFlinger.cpp): Frame composition loop, VSYNC handling, layer hierarchy.
  - [libs/renderengine](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/master/libs/renderengine/): Backend rendering engine (SkiaGL / SkiaVK) used when hardware composition falls back.
- **[AOSP Hardware Composer (HWC2 / AIDL Composer)](https://android.googlesource.com/platform/hardware/interfaces/+/refs/heads/master/graphics/composer/)**: HAL interface for hardware multi-plane composition.
  - [libhardware/include/hardware/hwcomposer.h](https://android.googlesource.com/platform/hardware/libhardware/+/refs/heads/master/include/hardware/hwcomposer.h): Legacy C-struct definitions (`hwc_composer_device_1_t`).
- **[AOSP Gralloc HAL (Allocator & Mapper)](https://android.googlesource.com/platform/hardware/interfaces/+/refs/heads/master/graphics/allocator/)**: Graphic buffer allocation interface.
  - [libhardware/include/hardware/gralloc.h](https://android.googlesource.com/platform/hardware/libhardware/+/refs/heads/master/include/hardware/gralloc.h): `gralloc_module_t` buffer allocation & locking API.
  - [ChromiumOS minigbm](https://chromium.googlesource.com/chromiumos/platform/minigbm/): Lightweight DRM/GBM-backed gralloc implementation used in Android-x86 and ChromeOS.
- **[Mesa Gallium3D Target Drivers](https://gitlab.freedesktop.org/mesa/mesa/-/tree/main/src/gallium/targets/dri)**:
  - [llvmpipe](https://gitlab.freedesktop.org/mesa/mesa/-/tree/main/src/gallium/drivers/llvmpipe): CPU rasterizer driver used as default software fallback on x86.
  - [virgl](https://gitlab.freedesktop.org/mesa/mesa/-/tree/main/src/gallium/drivers/virgl): Guest kernel/userspace Gallium driver talking to virtio-gpu.

### ✔ Success  
Document: **Android-x86 GPU Call Flow**.

---

## ✅ **STEP 4 — Android-x86 Patch Plan**

### 🎯 Goal  
Android-x86 ko tumhare GPU HAL tak redirect karna.

### 📝 TODO  
- Mesa ko stub driver se replace  
- EGL loader patch  
- hwcomposer patch  
- virtio-gpu driver integrate

### 🔗 Real Implementation References  
- **[AOSP EGL Loader (libEGL)](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/master/opengl/libs/EGL/)**:
  - [Loader.cpp](https://android.googlesource.com/platform/frameworks/native/+/refs/heads/master/opengl/libs/EGL/Loader.cpp): Dynamically scans `/vendor/lib/egl/` or `/system/lib/egl/` for `libGLES_*.so` and hooks `eglGetProcAddress`.
- **[Android-x86 Hardware & HAL Integration](https://github.com/android-x86/device-generic-common)**: Real repository containing Android-x86 init scripts, Mesa configurations, and HAL wiring.
- **[Google Cuttlefish Virtual Device GPU HAL](https://android.googlesource.com/device/google/cuttlefish/)**: Reference architecture for connecting guest Android HALs to host hypervisor channels (virtio-gpu / vsock).
- **[Linux DRM Virtio-GPU Kernel Driver](https://github.com/torvalds/linux/tree/master/drivers/gpu/drm/virtio)**: Kernel driver (`virtio-gpu.ko`) managing GEM objects, command queues, and displays in guest Linux.

### ✔ Success  
Patch plan ready.

---

## ✅ **STEP 5 — WebGPU Compositor POC**

### 🎯 Goal  
SurfaceFlinger-like compositor WebGPU me chal jaye.

### 📝 TODO  
- `webgpu_compositor` crate  
- Layer model define  
- Quad-based render pipeline  
- Alpha blending + z-order  
- Test harness with dummy layers

### 🔗 Real Implementation References  
- **[Google Skia Graphite (Dawn/WebGPU Backend)](https://skia.googlesource.com/skia/+/refs/heads/main/src/gpu/graphite/dawn/)**: Modern GPU rendering engine with full Dawn/WebGPU composition pipelines.
  - [DawnGraphicsPipeline.cpp](https://skia.googlesource.com/skia/+/refs/heads/main/src/gpu/graphite/dawn/DawnGraphicsPipeline.cpp): Blend states, color attachment management, and vertex attributes.
- **[Smithay (Rust Wayland Compositor Framework)](https://github.com/Smithay/smithay)**:
  - [smithay/src/backend/renderer/](https://github.com/Smithay/smithay/tree/master/src/backend/renderer): Clean quad-rendering abstractions, damaged-rect composition, and buffer sync primitives in Rust.
- **[iced-rs / iced_wgpu](https://github.com/iced-rs/iced/tree/master/wgpu)** & **[egui_wgpu](https://github.com/emilk/egui/tree/master/crates/egui-wgpu)**: Production quad-batching, textured quad shaders, and render pass encoders in WebGPU.

### ✔ Success  
WebGPU compositor working.

---

## ✅ **STEP 6 — Full Host Pipeline Integration**

### 🎯 Goal  
GL → WebGPU → compositor → canvas end-to-end pipeline.

### 📝 TODO  
- `gles2wgpu` + `webgpu_compositor` + `webgpu_swapchain` integrate  
- Render GL scene → texture  
- Compose layers  
- Present to canvas

### 🔗 Real Implementation References  
- **[wgpu::Surface & Canvas Context](https://docs.rs/wgpu/latest/wgpu/struct.Surface.html)**: Canvas presentation bridge in Rust Wasm via `web-sys`.
- **[web-sys GPUCanvasContext](https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.GpuCanvasContext.html)**: Wasm bindings for HTML5 `<canvas>` WebGPU context configuration.
- **[Google Dawn Wire](https://dawn.googlesource.com/dawn/+/refs/heads/main/src/dawn/wire/)**: Client-server serialization mechanism for IPC WebGPU commands.

### ✔ Success  
Host GPU engine complete.

---

## ✅ **STEP 7 — v86 Integration (virtio-gpu device)**

### 🎯 Goal  
Guest Android ke GPU commands host Rust engine tak aa rahe hon.

### 📝 TODO  
- v86 fork  
- New virtio-gpu device add  
- MMIO/PCI mapping  
- JS glue → Rust bridge

### 🔗 Real Implementation References  
- **[copy/v86 Repository](https://github.com/copy/v86)**: The WebAssembly x86 PC emulator.
  - [src/virtio.js](https://github.com/copy/v86/blob/master/src/virtio.js): Base class for all virtio devices in v86 (handling rings, descriptors, and queues).
  - [src/pci.js](https://github.com/copy/v86/blob/master/src/pci.js): PCI configuration space and BAR allocations.
  - [src/browser/main.js](https://github.com/copy/v86/blob/master/src/browser/main.js): Host integration bridge and web worker communication.
  - [v86 Issue #51 (virtio-gpu discussion)](https://github.com/copy/v86/issues/51): Design discussions regarding virtio-gpu in v86.
- **[QEMU Virtio-GPU Implementation](https://github.com/qemu/qemu/blob/master/hw/display/virtio-gpu.c)**: Reference C implementation of the PCI virtio-gpu hardware emulation device.

### ✔ Success  
Guest → host GPU command flow working.

---

## ✅ **STEP 8 — Android-x86 GPU Driver Patch**

### 🎯 Goal  
Android-x86 ke GLES/EGL/hwcomposer tumhare virtio-gpu device use karein.

### 📝 TODO  
- Stub GLES driver  
- Patch EGL SwapBuffers  
- Patch hwcomposer composition  
- Buffer mapping → host textures

### 🔗 Real Implementation References  
- **[AOSP goldfish-opengl (Complete Guest Driver Suite)](https://android.googlesource.com/platform/device/generic/goldfish-opengl/)**:
  - [system/egl/egl.cpp](https://android.googlesource.com/platform/device/generic/goldfish-opengl/+/refs/heads/master/system/egl/egl.cpp): In-guest `eglSwapBuffers`, `eglCreateWindowSurface`, and context switching.
  - [system/GLESv2_enc/GL2Encoder.cpp](https://android.googlesource.com/platform/device/generic/goldfish-opengl/+/refs/heads/master/system/GLESv2_enc/GL2Encoder.cpp): Intercepts all `glDraw*`, `glTexImage2D`, `glBind*` and packs them into stream buffers.
  - [system/gralloc/gralloc.cpp](https://android.googlesource.com/platform/device/generic/goldfish-opengl/+/refs/heads/master/system/gralloc/gralloc.cpp): Guest buffer allocation synced to host color buffers.
  - [system/hwc2/EmuHWC2.cpp](https://android.googlesource.com/platform/device/generic/goldfish-opengl/+/refs/heads/master/system/hwc2/EmuHWC2.cpp): HWC2 implementation forwarding layer composition to host.

### ✔ Success  
Android-x86 ke andar se GL app WebGPU se render hota hai.

---

## ✅ **STEP 9 — Run Real GPU APKs**

### 🎯 Goal  
Unity/Unreal/Godot APKs browser me chalna.

### 📝 TODO  
- Unity cube demo  
- Godot GLES2 demo  
- Simple 3D APK  
- Fix:
  - Shader translation bugs  
  - Missing GL features  
  - Performance bottlenecks

### 🔗 Real Implementation References  
- **[Godot Engine GLES Drivers](https://github.com/godotengine/godot)**:
  - [drivers/gles3](https://github.com/godotengine/godot/tree/master/drivers/gles3) & [3.x drivers/gles2](https://github.com/godotengine/godot/tree/3.x/drivers/gles2): Game engine GLES state patterns and shader structures.
- **[Khronos VK-GL-CTS (OpenGL ES Conformance Suite)](https://github.com/KhronosGroup/VK-GL-CTS)**: Standard test suite for validating GLES 2.0 / 3.0 conformance.
- **[Android NDK Native GLES Samples](https://github.com/android/ndk-samples/tree/main/gles3-jni)**: Minimal test APKs with JNI and pure native GLES3 render loops.

### ✔ Success  
First GPU-heavy APK playable.

---

# 🔍 **EXTRA ENGINEERING NOTES & UPSTREAM INDEX**

| Upstream Project | Primary Role | Reference Link |
|---|---|---|
| **Google ANGLE** | GLES → WebGPU/Vulkan/Metal Translation | [angle/src/libANGLE/renderer/wgpu](https://chromium.googlesource.com/angle/angle/+/refs/heads/main/src/libANGLE/renderer/wgpu/) |
| **AOSP goldfish-opengl** | Guest GLES/EGL/Gralloc/HWC Serializer Driver | [platform/device/generic/goldfish-opengl](https://android.googlesource.com/platform/device/generic/goldfish-opengl/) |
| **AOSP / Mesa gfxstream** | High-performance Android Guest-to-Host GPU Streaming | [platform/hardware/google/gfxstream](https://android.googlesource.com/platform/hardware/google/gfxstream/) |
| **Google crosvm rutabaga_gfx** | Paravirtualized Rust GPU Layer (Virtio-GPU / WebGPU) | [google/crosvm/rutabaga_gfx](https://github.com/google/crosvm/tree/main/rutabaga_gfx) |
| **Mesa virglrenderer** | Virtio-GPU 3D Command Stream Executor | [virgl/virglrenderer](https://gitlab.freedesktop.org/virgl/virglrenderer) |
| **copy/v86** | x86 Wasm PC Emulator & Virtio Bus | [copy/v86](https://github.com/copy/v86) |
| **OASIS Virtio Spec** | Standard Virtio GPU Device Specification | [Virtio v1.2 GPU Spec](https://docs.oasis-open.org/virtio/virtio/v1.2/csd01/virtio-v1.2-csd01.html#x1-3440007) |
| **Google Skia Graphite** | SurfaceFlinger / Android 2D WebGPU Renderer | [skia/src/gpu/graphite/dawn](https://skia.googlesource.com/skia/+/refs/heads/main/src/gpu/graphite/dawn/) |
| **gfx-rs / Naga** | Rust GLSL → WGSL Shader Translator | [gfx-rs/wgpu/naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga) |
| **Smithay** | Rust Quad Compositor & Multi-Plane Architecture | [Smithay/smithay](https://github.com/Smithay/smithay) |

---
