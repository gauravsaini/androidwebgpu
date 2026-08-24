# Verification Gates: Android WebGPU Stack Hardening

- [x] G1: glDraw dynamic multi-VBO allocation and vertex attribute leakage fix
  CHECK: cargo test -p gles2wgpu --test e2e_gles_pipeline -- --nocapture
  EXPECT: test_gles2wgpu_indexed_mesh_draw_elements ... ok

- [x] G2: GLES uniform buffer state management (glUniform1f, glUniform4fv, glUniformMatrix4fv)
  CHECK: cargo test -p gles2wgpu --test e2e_gles_pipeline test_gles2wgpu_uniforms
  EXPECT: test_gles2wgpu_uniforms ... ok

- [x] G3: Virtio-GPU ResourceFlush subrect blit with destination row stride
  CHECK: cargo test -p virtio_gpu_bridge test_flush_subrect_blit
  EXPECT: test_flush_subrect_blit ... ok

- [x] G4: WebGPU swapchain canvas presentation & surface resize handling
  CHECK: cargo test -p webgpu_swapchain test_swapchain_surface_resize
  EXPECT: test_swapchain_surface_resize ... ok

- [x] G5: Guest patches and AOSP driver implementations exist
  CHECK: test -f guest/patches/gralloc.virtio_gpu.cpp && test -f guest/patches/hwcomposer.virtio_gpu.cpp && test -f guest/patches/egl_webgpu.cpp && echo "GUEST_PATCHES_EXIST"
  EXPECT: GUEST_PATCHES_EXIST

- [x] G6: Virtio-GPU WASM bridge bindings with wasm-bindgen
  CHECK: cargo check --workspace
  EXPECT: Finished

- [x] G7: Full Workspace automated test suite
  CHECK: cargo test --workspace
  EXPECT: test result: ok

- [x] G8: Real APK Flight (Unity Cube & Godot GLES2) through Virtio-GPU Submit3D
  CHECK: cargo test --test apk_real -- --nocapture
  EXPECT: test_apk_real_unity_and_godot_virtio_flight ... ok
