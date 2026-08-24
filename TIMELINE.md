# TIMELINE: Android WebGPU Stack Milestones

- **Pass 1 (Architecture & POC)**: Setup core crates, Naga GLSL->WGSL compiler, basic buffer/texture resources, and initial E2E tests.
- **Pass 2 (Pipeline & Protocol)**: Implemented OASIS Virtio 1.2 commands, compositor matrix pipeline, dynamic VAO layout, depth/stencil attachments, scissor clipping, and APK analysis.
- **Pass 3 (Hardening & Complete Gap Remediation)**:
  - Fixed vertex layout memory leak and supported multi-buffer VBO bindings per attribute slot.
  - Implemented full uniform state machine (`glUniform1f`, `glUniform4fv`, `glUniformMatrix4fv`) and shader uniform buffer bind groups.
  - Fixed Virtio-GPU `ResourceFlush` subrect stride blit to scanout buffer.
  - Implemented Virtio-GPU WASM exports (`virtio_gpu_bridge/src/wasm.rs`) and v86 device driver.
  - Implemented concrete C++ guest drivers in `guest/patches/` (`gralloc.virtio_gpu.cpp`, `hwcomposer.virtio_gpu.cpp`, `egl_webgpu.cpp`, `Android.bp`).
  - Added full uniform reflection into WGSL `layout(std140, set=0, binding=2) uniform UniformBlock` struct.
  - Differentiated `GL_SHORT` normalized (`Snorm16`) vs integer (`Sint16`) attribute vertex formats.
  - Added `VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D` protocol decoding and dispatch.
  - Added HWC rotation/reflection matrix transformations (90°, 180°, 270°, flip-h, flip-v) and damage scissor clipping in `webgpu_compositor`.
  - Added `wgpu::Surface` target mode to `webgpu_swapchain`.
  - Wired live metrics tracking assertions into full-stack E2E tests.
