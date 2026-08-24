# Android-x86 Guest HAL Drivers for WebGPU

This directory contains guest-side C++ Hardware Abstraction Layer (HAL) modules for Android-x86:

1. `gralloc.virtio_gpu.cpp`: Implements `gralloc.default` module allocating guest framebuffers using shared memfds and associating them with host Virtio-GPU resource IDs.
2. `hwcomposer.virtio_gpu.cpp`: Implements `hwcomposer` HAL forwarding SurfaceFlinger multi-plane layer composition metadata directly to the host `webgpu_compositor`.
3. `egl_webgpu.cpp`: Implements EGL 1.4 API marshaling OpenGL ES 2.0/3.0 commands into Virtio-GPU Submit3D wire packets.

## Build Instructions (AOSP Tree)
Place these files in `hardware/libhardware/modules/` or `device/generic/goldfish-opengl/` and compile with Android `Android.bp` / `Android.mk`:
```bash
m gralloc.virtio_gpu hwcomposer.virtio_gpu egl.virtio_gpu
```
