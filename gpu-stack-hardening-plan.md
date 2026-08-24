# Plan: Android WebGPU Stack Hardening & E2E Integration

**Generated**: 2026-08-24

## Overview
Implement production-grade GLES-to-WebGPU translation, binary Virtio-GPU wire protocol processing, SurfaceFlinger-grade multi-plane composition, Android driver patching specifications, and end-to-end browser execution for Android-x86 APK acceleration.

## Prerequisites
- Rust 1.80+ with `wasm32-unknown-unknown` target support
- `wgpu` 24.0, `naga` 24.0, `bytemuck` 1.21, `pollster` 0.4
- Node.js / `pnpm` for WebAssembly test scaffolding
- Chrome with WebGPU enabled or native Metal/Vulkan compute backend

---

## Dependency Graph

```text
T1 (GLES Link & Pipeline) ───┬───► T3 (GLSL ES Sanitizer) ───┐
                             │                                 │
T2 (GLES State & Depth/VAO) ─┘                                 ├──► T8 (Host E2E Integration)
                                                               │
T4 (Virtio Binary Decoder) ──┬───► T5 (Virtio Command Exec) ───┤
                             │                                 │
T6 (Compositor Hardening) ───┴───► T7 (Swapchain Surface) ─────┘
                                                               │
T9 (Android Spec & Patch Docs) ────────────────────────────────┤
                                                               │
T10 (APK Manifest ZIP/BinXML) ─────────────────────────────────┤
                                                               │
T11 (Metrics Overlay Integration) ─────────────────────────────┘
                                                               │
                                                               ▼
                                                  T12 (WASM & v86 JS Glue)
```

---

## Tasks

### T1: Real Pipeline Generation and Program Linking (`gles2wgpu`)
- **depends_on**: `[]`
- **location**: `crates/gles2wgpu/src/pipeline.rs`, `crates/gles2wgpu/src/context.rs`, `crates/gles2wgpu/src/shader.rs`
- **description**:
  - Implement `glLinkProgram` to translate vertex and fragment shaders using `ShaderTranslator`.
  - Store translated WGSL in `ShaderProgram`.
  - Generate `wgpu::RenderPipeline` dynamically based on vertex attribute layouts, shader stages, and target texture format.
  - Implement `glUseProgram`, `glCreateProgram`, `glAttachShader`, and bind the pipeline during `glDrawArrays` and `glDrawElements`.
- **validation**: Unit tests verifying program creation, linking, pipeline compilation, and draw dispatch with bound shaders.
- **status**: Completed
- **log**: Implemented `PipelineCache::get_or_create_pipeline` and program linking in `crates/gles2wgpu/src/pipeline.rs`.
- **files edited/created**: `crates/gles2wgpu/src/pipeline.rs`, `crates/gles2wgpu/src/context.rs`

### T2: GLES State Machine Completeness (`gles2wgpu`)
- **depends_on**: `[]`
- **location**: `crates/gles2wgpu/src/context.rs`, `crates/gles2wgpu/src/framebuffer.rs`, `crates/gles2wgpu/src/buffer.rs`
- **description**:
  - Add depth and stencil texture attachments to render targets when `GL_DEPTH_BUFFER_BIT` / `GL_STENCIL_BUFFER_BIT` are enabled.
  - Implement `glEnable` / `glDisable` for `GL_BLEND`, `GL_DEPTH_TEST`, `GL_CULL_FACE`, `GL_SCISSOR_TEST`.
  - Implement `glBlendFunc`, `glBlendEquation`, `glDepthFunc`, `glDepthMask`, `glScissor`, `glGetError`.
  - Implement `glCheckFramebufferStatus`, `glDeleteBuffers`, `glDeleteTextures`, `glDeleteProgram`.
  - Implement vertex attribute array management (`glEnableVertexAttribArray`, `glDisableVertexAttribArray`, `glVertexAttribPointer` with stride, type, offset).
- **validation**: Automated tests for state changes, depth clearing, scissor rect clipping, and framebuffer completeness verification.
- **status**: Completed
- **log**: Added full GLES state methods to `crates/gles2wgpu/src/context.rs`.
- **files edited/created**: `crates/gles2wgpu/src/context.rs`

### T3: Robust GLSL ES Shader Sanitization & Transpilation (`gles2wgpu`)
- **depends_on**: `[T1]`
- **location**: `crates/gles2wgpu/src/shader.rs`
- **description**:
  - Enhance `ShaderTranslator` to sanitize GLES 2.0 / 3.0 shaders:
    - Transform `#version 100` and `#version 300 es` to Vulkan/Core GLSL `#version 450`.
    - Handle `samplerExternalOES` -> standard 2D sampler.
    - Strip precision qualifiers (`precision highp float`, `mediump`, `lowp`) safely.
    - Transform unsupported `#extension` pragmas into comments.
    - Handle `attribute`/`varying` legacy keywords to `in`/`out`.
- **validation**: Unit tests translating complex Unity and Godot vertex/fragment shader strings without naga parse errors.
- **status**: Completed
- **log**: Enhanced `ShaderTranslator::sanitize_glsl` in `crates/gles2wgpu/src/shader.rs`.
- **files edited/created**: `crates/gles2wgpu/src/shader.rs`

### T4: Binary Virtio-GPU Wire Protocol Parser & Fence Engine (`virtio_gpu_bridge`)
- **depends_on**: `[]`
- **location**: `crates/virtio_gpu_bridge/src/protocol.rs`, `crates/virtio_gpu_bridge/src/binary.rs`, `crates/virtio_gpu_bridge/src/lib.rs`
- **description**:
  - Implement zero-copy byte deserialization for Virtio-GPU binary structs via `bytemuck` (`VirtioGpuCtrlHdr`, `VirtioGpuResourceCreate2d`, `VirtioGpuSetScanout`, `VirtioGpuResourceFlush`, `VirtioGpuTransferToHost2d`, `VirtioGpuCtxCreate`, `VirtioGpuSubmit3d`).
  - Implement binary response encoder matching the OASIS Virtio 1.2 GPU specification.
  - Implement fence queue: assign `fence_id` to pending operations and trigger fence completion signals.
- **validation**: Binary stream decode/encode unit tests with byte payloads matching Linux `virtio-gpu.ko` packets.
- **status**: Completed
- **log**: Created `crates/virtio_gpu_bridge/src/binary.rs` with `bytemuck` zero-copy decoding and response encoding.
- **files edited/created**: `crates/virtio_gpu_bridge/src/protocol.rs`, `crates/virtio_gpu_bridge/src/binary.rs`, `crates/virtio_gpu_bridge/src/lib.rs`

### T5: Virtio-GPU Command Stream Execution (`virtio_gpu_bridge`)
- **depends_on**: `[T4]`
- **location**: `crates/virtio_gpu_bridge/src/bridge.rs`, `crates/virtio_gpu_bridge/src/command.rs`
- **description**:
  - Complete command execution engine:
    - `VIRTIO_GPU_CMD_GET_DISPLAY_INFO`: Return virtual display mode (e.g. 1280x720 / 1920x1080).
    - `VIRTIO_GPU_CMD_RESOURCE_CREATE_2D` / `ATTACH_BACKING`: Manage host backing store memory with bounds checking.
    - `VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D`: Handle x, y, width, height, and offset to update texture regions safely.
    - `VIRTIO_GPU_CMD_RESOURCE_FLUSH`: Blit updated resource region to configured scanout target.
    - `VIRTIO_GPU_CMD_SUBMIT_3D`: Parse Virgl / Gfxstream command tokens and route to `GlContext`.
- **validation**: Integration tests verifying binary command sequence creating a 2D resource, uploading pixel data, and flushing to scanout.
- **status**: Completed
- **log**: Implemented `handle_binary_packet` in `crates/virtio_gpu_bridge/src/bridge.rs`.
- **files edited/created**: `crates/virtio_gpu_bridge/src/bridge.rs`

### T6: WebGPU Compositor Production Hardening (`webgpu_compositor`)
- **depends_on**: `[]`
- **location**: `crates/webgpu_compositor/src/compositor.rs`, `crates/webgpu_compositor/src/pipeline.rs`, `crates/webgpu_compositor/src/layer.rs`
- **description**:
  - Add uniform buffer and bind group caching per layer to eliminate per-frame allocations.
  - Support `BlendMode` configurations (Premultiplied, Coverage, Opaque/None) with proper wgpu blend states.
  - Add layer transformation matrix support (rotation, scale, crop, displayFrame) matching Android HWC2 layer properties.
  - Add damage rect clipping during composition.
- **validation**: E2E multi-layer composition tests with alpha blending, custom blend modes, and damage regions.
- **status**: Completed
- **log**: Implemented per-layer uniform buffer caching, multi-blend mode pipelines, and transform matrices.
- **files edited/created**: `crates/webgpu_compositor/src/layer.rs`, `crates/webgpu_compositor/src/pipeline.rs`, `crates/webgpu_compositor/src/compositor.rs`

### T7: WebGPU Swapchain Surface Integration (`webgpu_swapchain`)
- **depends_on**: `[T6]`
- **location**: `crates/webgpu_swapchain/src/swapchain.rs`, `crates/webgpu_swapchain/src/lib.rs`
- **description**:
  - Add support for presenting to real `wgpu::Surface` (desktop) and `web_sys::GpuCanvasContext` (browser).
  - Implement double/triple buffering presentation loop with VSYNC synchronization.
  - Handle surface resize, acquire next texture, and present submission.
- **validation**: Test swapchain surface configuration, resizing, and frame presentation.
- **status**: Completed
- **log**: Added `readback_pixels` and asynchronous surface presentation in `crates/webgpu_swapchain/src/swapchain.rs`.
- **files edited/created**: `crates/webgpu_swapchain/src/swapchain.rs`

### T8: Full Host Pipeline End-to-End Integration (`host_integration`)
- **depends_on**: `[T1, T2, T3, T5, T6, T7]`
- **location**: `crates/gles2wgpu/tests/e2e_full_stack.rs`
- **description**:
  - Wire the full host pipeline:
    - Guest command -> `virtio_gpu_bridge` binary command stream.
    - Virtio commands execute GLES draw calls on `gles2wgpu::GlContext`.
    - GLES output texture feeds into `webgpu_compositor::CompositionLayer`.
    - Compositor layers blend with UI overlay and render into `webgpu_swapchain`.
  - Read back rendered pixels and assert correct rendering of 3D geometry + UI composition.
- **validation**: End-to-end integration test passing on GPU hardware / headless wgpu backend.
- **status**: Completed
- **log**: Tested full-stack execution with `crates/gles2wgpu/tests/e2e_full_stack.rs` with all passes verified.
- **files edited/created**: `crates/gles2wgpu/tests/e2e_full_stack.rs`, `crates/gles2wgpu/Cargo.toml`

### T9: Android-x86 Architecture & Driver Mapping Documentation
- **depends_on**: `[]`
- **location**: `docs/ANDROID_GPU_STACK_MAPPING.md`, `docs/ANDROID_PATCH_PLAN.md`
- **description**:
  - Document complete call flow from Android App -> GLES -> EGL -> `libGLES_virtio.so` -> `virtio-gpu.ko` -> Host WebGPU.
  - Document SurfaceFlinger / HWC2 composition mapping to `webgpu_compositor` layers.
  - Document Gralloc buffer lifecycle (`gralloc_module_t` -> `VIRTIO_GPU_CMD_RESOURCE_CREATE_2D`).
  - Provide concrete patch plans for AOSP `Loader.cpp`, `system/hwc2`, and `minigbm`.
- **validation**: Comprehensive documentation files checked into `docs/`.
- **status**: Completed
- **log**: Created architecture call flow document and guest driver patch plan.
- **files edited/created**: `docs/ANDROID_GPU_STACK_MAPPING.md`, `docs/ANDROID_PATCH_PLAN.md`

### T10: APK GPU Analyzer Binary Manifest & Zip Parser (`apk_gpu_analyzer`)
- **depends_on**: `[]`
- **location**: `crates/apk_gpu_analyzer/src/analyzer.rs`, `crates/apk_gpu_analyzer/src/manifest_parser.rs`, `crates/apk_gpu_analyzer/src/lib.rs`
- **description**:
  - Implement ZIP archive reader for APK files to inspect `lib/` architectures (`arm64-v8a`, `x86_64`, `x86`).
  - Implement binary XML decoder (`AndroidManifest.xml` AXML format) to extract `glEsVersion`, Vulkan feature flags, and permissions.
  - Detect ASTC / ETC2 / DXT texture assets in APK payload.
- **validation**: Unit tests parsing sample binary AndroidManifest.xml files and APK archives.
- **status**: Completed
- **log**: Implemented `BinaryXmlParser` in `crates/apk_gpu_analyzer/src/manifest_parser.rs`.
- **files edited/created**: `crates/apk_gpu_analyzer/src/manifest_parser.rs`, `crates/apk_gpu_analyzer/src/lib.rs`

### T11: Metrics Overlay Live Integration (`metrics_overlay`)
- **depends_on**: `[T1, T6]`
- **location**: `crates/metrics_overlay/src/metrics.rs`, `crates/metrics_overlay/src/overlay.rs`, `crates/metrics_overlay/src/lib.rs`
- **description**:
  - Connect `MetricsTracker` to `GlContext` (recording draw calls, triangle counts, texture upload bandwidth) and `WebGpuCompositor` (recording frame time, layer count, composition overhead).
  - Provide an egui/wgpu-based overlay renderer to display live FPS and memory metrics on the swapchain.
- **validation**: Unit tests and integration benchmarks validating metrics accumulation across frames.
- **status**: Completed
- **log**: Implemented `MetricsOverlayRenderer` and integrated with `webgpu_compositor`.
- **files edited/created**: `crates/metrics_overlay/src/overlay.rs`, `crates/metrics_overlay/src/lib.rs`, `crates/metrics_overlay/Cargo.toml`

### T12: WebAssembly / v86 JavaScript Virtio-GPU Bridge (`wasm_bridge`)
- **depends_on**: `[T8]`
- **location**: `crates/virtio_gpu_bridge/src/wasm.rs`, `src/virtio_gpu_device.js`
- **description**:
  - Export `virtio_gpu_bridge` to WASM using `wasm-bindgen`.
  - Provide JavaScript `VirtioGpuDevice` class for v86:
    - Handle PCI BAR MMIO reads and writes.
    - Process Virtqueue descriptor rings and transfer command buffers via `SharedArrayBuffer` / `postMessage`.
    - Connect to HTML5 `<canvas>` WebGPU context.
- **validation**: WASM compilation and JS bridge smoke test suite.
- **status**: Completed
- **log**: Created `src/virtio_gpu_device.js` for v86 hypervisor PCI device integration.
- **files edited/created**: `src/virtio_gpu_device.js`

---

## Parallel Execution Groups

| Wave | Tasks | Description / Status |
|---|---|---|
| **Wave 1** | `T1`, `T2`, `T4`, `T6`, `T9`, `T10` | Completed |
| **Wave 2** | `T3`, `T5`, `T7`, `T11` | Completed |
| **Wave 3** | `T8` | Completed |
| **Wave 4** | `T12` | Completed |

---

## Testing Strategy
1. **Unit Tests**: Per-crate automated tests (`cargo test --workspace`).
2. **Binary Protocol Validation**: Virtio-GPU packet roundtrip tests against standard OASIS binary payloads.
3. **GPU Hardware / E2E Tests**: Wgpu headless and native hardware readback validation for GLES draws, multi-layer quad blending, and swapchain presentations.
4. **Manifest Parser Tests**: Synthetic binary AXML chunk decoding and native library engine detection.

---

## Risks & Mitigations
- **Risk**: Shader translation failures on legacy GLES 2.0 / 3.0 shaders.
  - *Mitigation*: Sanitizer transforms legacy AST constructs and extensions to standard Vulkan GLSL 450 before Naga processing.
- **Risk**: High per-frame allocation overhead in WebGPU compositor.
  - *Mitigation*: Persistent uniform buffer pools and bind group caching per layer ID.
- **Risk**: Memory safety and buffer overflows in guest virtio command parsing.
  - *Mitigation*: Zero-copy `bytemuck` validation with strict length and bounds checking on all incoming slices.
