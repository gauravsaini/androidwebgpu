# Android-x86 GPU Stack Architecture & WebGPU Mapping

## 1. Overview
This document specifies the end-to-end graphics pipeline mapping from an Android-x86 guest OS running inside a browser hypervisor (such as v86) to host-side WebGPU acceleration.

```text
┌─────────────────────────────────────────────────────────────┐
│                    Android-x86 (Guest)                      │
│                                                             │
│  [APK / Unity / Godot]                                      │
│        │                                                    │
│        ▼                                                    │
│  [libGLESv2 / EGL] (goldfish-opengl GLES2Encoder)           │
│        │                                                    │
│        ▼                                                    │
│  [Gralloc HAL] (minigbm / DRM GEM)                          │
│        │                                                    │
│        ▼                                                    │
│  [virtio-gpu.ko] (Linux Kernel DRM Driver)                  │
└────────┼────────────────────────────────────────────────────┘
         │ Virtio-GPU MMIO / Virtqueue Command Stream (PCI)
┌────────▼────────────────────────────────────────────────────┐
│                    Browser Host Engine                      │
│                                                             │
│  [v86 JS Glue / SharedArrayBuffer]                          │
│        │                                                    │
│        ▼                                                    │
│  [virtio_gpu_bridge] (Binary Packet Deserializer)           │
│        │                                                    │
│        ├─► 2D Flush / Scanout ──► [webgpu_compositor]       │
│        │                                │                   │
│        └─► 3D / GLES Commands           ▼                   │
│                 │              [webgpu_swapchain]           │
│                 ▼                       │                   │
│           [gles2wgpu] ──────────────────┘                   │
│                 │                                           │
│                 ▼                                           │
│           [WebGPU / Dawn / Metal / Vulkan Hardware]         │
└─────────────────────────────────────────────────────────────┘
```

## 2. Component Call Flow Mappings

### A. SurfaceFlinger & HWC2 Composition
- **AOSP Path**: `frameworks/native/services/surfaceflinger/SurfaceFlinger.cpp`
- **Role**: Collects graphic buffers from active application windows and status bars, sorts them by Z-order, and submits composition requests to the Hardware Composer (`HWC2`).
- **WebGPU Target**: `webgpu_compositor::WebGpuCompositor`. Each Android layer (Window, Wallpaper, NavigationBar) maps directly to a `CompositionLayer` with specific `bounds`, `transform`, `z_order`, and `BlendMode` (Premultiplied / Coverage).

### B. Gralloc (Graphic Buffer Allocator)
- **AOSP Path**: `hardware/libhardware/include/hardware/gralloc.h`, `minigbm`
- **Role**: Allocates linear or tiled GPU memory for application framebuffers and video decoders.
- **WebGPU Target**: `VIRTIO_GPU_CMD_RESOURCE_CREATE_2D` / `VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING`. Allocates a host WebGPU texture and binds guest page descriptors.

### C. GLES & EGL Stream Encoder
- **AOSP Path**: `device/generic/goldfish-opengl/system/GLESv2_enc/GL2Encoder.cpp`
- **Role**: Serializes OpenGL ES API calls into stream buffers forwarded to the hypervisor channel.
- **WebGPU Target**: `gles2wgpu::GlContext`. Translates GL state changes, compiles GLSL shaders to WGSL via Naga, and executes render passes on native WebGPU queues.
