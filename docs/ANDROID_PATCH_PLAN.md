# Android-x86 Guest Driver Patch Plan

## 1. Objective
Enable Android-x86 to redirect all OpenGL ES and composition traffic to the `virtio-gpu` device exposed by the WebAssembly hypervisor (v86), allowing host-side WebGPU acceleration.

---

## 2. Target Repositories and Components

### 1. `device/generic/common` (Android-x86 init & HAL configs)
- **Target**: `init.sh`, `ueventd.android_x86.rc`
- **Action**:
  - Configure `setprop ro.hardware.gralloc minigbm_gbm_mesa` or `goldfish`.
  - Configure `setprop ro.hardware.hwcomposer drm` or `goldfish`.
  - Ensure `/dev/dri/card0` permissions are `0666` for `surfaceflinger` access.

### 2. `frameworks/native/opengl/libs/EGL/Loader.cpp`
- **Target**: EGL Driver Selection Mechanism
- **Action**:
  - Add `/vendor/lib64/egl/libGLES_virtio.so` to the prioritized EGL driver search list.
  - Intercept `eglGetProcAddress` to forward GLES 2.0 / 3.0 functions directly to the `virtio-gpu` stream encoder.

### 3. `minigbm` / DRM GEM Gralloc
- **Target**: `cros_gralloc/gralloc0/gralloc0.cc`
- **Action**:
  - Use `DRM_IOCTL_VIRTGPU_RESOURCE_CREATE` to allocate guest-host synchronized buffers.
  - On `gralloc_unlock`, trigger `DRM_IOCTL_VIRTGPU_TRANSFER_TO_HOST` and `DRM_IOCTL_VIRTGPU_EXECBUFFER` with `VIRTIO_GPU_CMD_RESOURCE_FLUSH`.

### 4. `system/hwc2/EmuHWC2.cpp` (Hardware Composer)
- **Target**: `EmuHWC2::validateDisplay` and `EmuHWC2::presentDisplay`
- **Action**:
  - Package active layers into a lightweight IPC struct: `{ layer_id, z_order, transform, crop, display_frame, blend_mode, resource_id }`.
  - Submit the composition frame packet to the host `webgpu_compositor` via `VIRTIO_GPU_CMD_SUBMIT_3D`.
