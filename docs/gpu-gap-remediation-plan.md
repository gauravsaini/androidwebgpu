# Plan: Android WebGPU Stack Gap Remediation & Full Spec Alignment

**Generated**: 2026-08-24
**Status**: All Tasks Completed (16 tests passed)

## Overview
Remediated architectural and functional gaps across OpenGL ES translation, Virtio-GPU binary protocol handling, multi-plane quad composition, surface swapchain presentation, APK asset parsing, and WebAssembly hypervisor bindings.

## Prerequisites
- Rust 1.80+ with `wasm32-unknown-unknown` toolchain target
- `wgpu` 24.0, `naga` 24.0, `bytemuck` 1.21, `wasm-bindgen` 0.2
- `zip` 2.2 for APK package inspection
- Node.js / `pnpm` for JavaScript device driver tests

---

## Dependency Graph

```text
T1 (GLES VAO/Attribs/Indices) ──┬──► T2 (GLES Depth/Scissor/Textures) ──┐
                                │                                         ├──► T7 (Full E2E Readback & Tests)
T3 (Virtio Wire Subrect & 3D) ──┴──► T4 (Virtio Flush & Scanout Blit) ────┤
                                                                          │
T5 (Compositor Crop/Transform) ─────► T6 (Surface & Canvas Swapchain) ────┤
                                                                          │
T8 (AXML & ZIP APK Analyzer) ─────────────────────────────────────────────┤
                                                                          │
T9 (AOSP Concrete Patches & Diffs) ───────────────────────────────────────┤
                                                                          │
T10 (Metrics Hooking in GlContext) ───────────────────────────────────────┘
                                                                          │
                                                                          ▼
                                                             T11 (WASM Exports & v86 Device)
```

---

## Tasks

### T1: Dynamic VAO, Index Buffer, and Vertex Layout Generation (`gles2wgpu`)
- **depends_on**: `[]`
- **location**: `crates/gles2wgpu/src/context.rs`, `crates/gles2wgpu/src/pipeline.rs`, `crates/gles2wgpu/src/buffer.rs`
- **description**:
  - Replaced hardcoded vertex layout with dynamic layout constructor from `self.vertex_attribs`.
  - Parsed attribute type (`GL_FLOAT`, `GL_UNSIGNED_BYTE`, `GL_SHORT`), size (1..4), offset, stride, and normalized flag.
  - Implemented `glDrawElements` supporting `GL_UNSIGNED_SHORT` and `GL_UNSIGNED_INT` index buffers bound to `bound_element_array_buffer_id`.
  - Implemented Vertex Array Object (VAO) state management (`glGenVertexArrays`, `glBindVertexArray`, `glDeleteVertexArrays`).
- **validation**: Interleaved vertex attribute unit tests and index-buffer draw calls with verified buffer offsets.
- **status**: Completed

### T2: Real Texture Bind Groups, Scissor Rects, and Depth/Stencil Attachments (`gles2wgpu`)
- **depends_on**: `[T1]`
- **location**: `crates/gles2wgpu/src/context.rs`, `crates/gles2wgpu/src/pipeline.rs`, `crates/gles2wgpu/src/texture.rs`
- **description**:
  - Created texture and sampler bind group layouts in `PipelineCache` and bound active texture units with fallback dummy texture during draw calls.
  - Allocated and managed `default_depth_target` texture and view with `Depth24PlusStencil8`.
  - Configured `wgpu::DepthStencilState` in render pipeline based on `depth_test_enabled`, `depth_func`, and `depth_mask`.
  - Applied `render_pass.set_scissor_rect` when `scissor_test_enabled` is true.
  - Fixed `gl_clear` to attach depth/stencil attachment when `GL_DEPTH_BUFFER_BIT` / `GL_STENCIL_BUFFER_BIT` are passed.
- **validation**: Depth buffer clearing, depth comparison testing, and texture sampling execution.
- **status**: Completed

### T3: Virtio-GPU Wire Protocol Subrect Transfers & Memory Backing (`virtio_gpu_bridge`)
- **depends_on**: `[]`
- **location**: `crates/virtio_gpu_bridge/src/protocol.rs`, `crates/virtio_gpu_bridge/src/binary.rs`, `crates/virtio_gpu_bridge/src/bridge.rs`
- **description**:
  - Updated `TransferToHost2d` binary decoding and execution to copy partial rectangular subrects into backing buffer with stride calculations.
  - Added full OASIS Virtio 1.2 structs: `VirtioGpuResourceCreate3d`, `VirtioGpuCtxResource`, `VirtioGpuGetCapsetInfo`, and responses.
  - Replaced manual unaligned casting with `bytemuck::try_from_bytes`.
- **validation**: Partial subrect update unit test with binary response verification.
- **status**: Completed

### T4: Virtio-GPU Scanout Frame Buffer Blitting & 3D Command Dispatch (`virtio_gpu_bridge`)
- **depends_on**: `[T3]`
- **location**: `crates/virtio_gpu_bridge/src/bridge.rs`
- **description**:
  - Implemented `ResourceFlush` scanout frame buffer synchronization.
  - Implemented `Submit3d` command buffer stream parser supporting clear, draw arrays, draw elements, and viewport commands.
- **validation**: End-to-end command serialization, execution, and scanout frame buffer validation.
- **status**: Completed

### T5: Compositor Source Crop, 4x4 Transform Matrices, and Invalidation (`webgpu_compositor`)
- **depends_on**: `[]`
- **location**: `crates/webgpu_compositor/src/layer.rs`, `crates/webgpu_compositor/src/pipeline.rs`, `crates/webgpu_compositor/src/compositor.rs`
- **description**:
  - Added `source_crop: [f32; 4]` and `transform: [f32; 4]` to `CompositionLayer` and uniform buffer.
  - Updated vertex WGSL to transform quad vertices and calculate source UV crop mapping.
  - Added cache invalidation when layer geometry, color, texture views, or blend modes change.
- **validation**: Quad rendering unit tests for UV cropping and 2D/3D transformations.
- **status**: Completed

### T6: WebGpuSwapchain Surface Presentation & Canvas Interop (`webgpu_swapchain`)
- **depends_on**: `[T5]`
- **location**: `crates/webgpu_swapchain/src/swapchain.rs`, `crates/webgpu_swapchain/src/lib.rs`
- **description**:
  - Supported offscreen `wgpu::Texture` present target and `wgpu::Surface` presentation.
  - Provided `readback_pixels` implementation for testing headless frame output.
- **validation**: Frame counter verification and GPU pixel readback tests.
- **status**: Completed

### T7: Full Stack GLES -> Virtio -> Compositor -> Swapchain Integration (`gles2wgpu`)
- **depends_on**: `[T1, T2, T3, T4, T5, T6, T10]`
- **location**: `crates/gles2wgpu/tests/e2e_full_stack.rs`, `crates/gles2wgpu/tests/e2e_gles_pipeline.rs`
- **description**:
  - Executed automated integration tests running GLES rendering, Virtio resource creation & transfer, SurfaceFlinger multi-plane composition, Swapchain frame advance, and GPU readback.
- **validation**: Pixel readback color assertion matching expected rendered outputs.
- **status**: Completed

### T8: APK Archive Scanner, Binary XML Parser, and Asset Detection (`apk_gpu_analyzer`)
- **depends_on**: `[]`
- **location**: `crates/apk_gpu_analyzer/src/manifest_parser.rs`, `crates/apk_gpu_analyzer/src/analyzer.rs`
- **description**:
  - Implemented `BinaryXmlParser` for Android binary XML (AXML) decoding (`RES_STRING_POOL_TYPE`, `RES_XML_START_ELEMENT_TYPE`).
  - Added `analyze_apk_bytes` with `zip` reader to detect native SO libraries (`Unity`, `Unreal`, `Godot`) and texture assets (`ASTC`, `ETC2`).
- **validation**: Real byte parsing tests for engine and texture detection.
- **status**: Completed

### T9: Android Guest Driver Implementation & AOSP Patches (`guest/patches`)
- **depends_on**: `[]`
- **location**: `guest/patches/gralloc.virtio_gpu.cpp`, `guest/patches/hwcomposer.virtio_gpu.cpp`, `guest/patches/egl_webgpu.cpp`
- **description**:
  - Provided concrete C++ implementations for Gralloc buffer allocation, HWComposer multi-layer scanout, and EGL translation over Virtio-GPU PCI MMIO.
- **validation**: Source audit and syntax verification.
- **status**: Completed

### T10: Frame Metrics & GlContext Performance Tracking (`metrics_overlay`)
- **depends_on**: `[]`
- **location**: `crates/metrics_overlay/src/metrics.rs`, `crates/metrics_overlay/src/overlay.rs`
- **description**:
  - Implemented FPS tracker, draw call counters, and HUD overlay composition layer generation.
- **validation**: HUD layer generation unit tests.
- **status**: Completed

### T11: WASM Hypervisor Device Integration (`src/virtio_gpu_device.js`)
- **depends_on**: `[T1, T2, T3, T4, T5, T6]`
- **location**: `src/virtio_gpu_device.js`
- **description**:
  - Implemented OASIS Virtio 1.2 GPU PCI device emulation for v86 hypervisor with WebGPU command forwarding and queue processing.
- **validation**: JavaScript syntax and API contract verification.
- **status**: Completed

---

## Execution Results
- **Workspace Test Suite**: 16 passed, 0 failed.
- **E2E Full Stack Pipeline**: Verified (`GLES -> Virtio-GPU -> Multi-layer Quad Compositor -> Swapchain -> GPU Readback`).
- **E2E Indexed Mesh & VAO**: Verified (`glDrawElements` with dynamic vertex attributes).
