# GOAL: Android WebGPU Stack

## Objective
Back Android-x86 OpenGL ES / EGL / SurfaceFlinger rendering pipeline with WebGPU in Rust for browser-based emulation in v86 hypervisor with full Unity/Unreal/Godot APK compatibility.

## Core Modules
1. `gles2wgpu`: OpenGL ES 2.0/3.0 to WebGPU translation (VAO, dynamic VBO slots, uniforms, depth/stencil, scissor, texture sampling).
2. `virtio_gpu_bridge`: OASIS Virtio 1.2 GPU protocol parser, 2D/3D command processor, scanout framebuffer blitting with subrect stride, and WASM FFI bindings.
3. `webgpu_compositor`: Multi-plane quad compositor (SurfaceFlinger equivalent) with source UV cropping, 4x4 matrix transformation, damage rects, and blend modes.
4. `webgpu_swapchain`: WebGPU surface/canvas presentation, VSync timing, and headless readback.
5. `apk_gpu_analyzer`: Android APK manifest and native library scanner.
6. `metrics_overlay`: Frame time and GPU performance HUD overlay.
7. `guest/patches`: Concrete AOSP C++ patches for gralloc, hwcomposer, and egl over virtio-gpu.
