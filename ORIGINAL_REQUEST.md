# Original User Request

## 2026-08-28T16:45:35Z

Pure in-guest authentic Android UI execution eliminating host LayoutInflater fallback by running real Android runtime (boot.art + framework.jar), Zygote process fork, and SurfaceFlinger rendering directly to /dev/dri/card0 and VirtIO-GPU scanout.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Authentic In-Guest Android Runtime (ART) & Framework Staging
Extract and stage genuine Android runtime binaries (boot.art ~18MB, framework.jar ~8MB, core-libart.jar, ext.jar, services.jar, libart.so) into guest/initrd/system/framework/ and guest/initrd/system/lib/. Remove synthetic generation scripts (guest/tools/generate_art_assets.mjs) and update build_initrd.sh to package genuine Android assets into initrd.img.

### R2. Real Zygote Daemon & ServiceManager IPC Lifecycle
Configure guest/initrd/init with authentic BOOTCLASSPATH, launch Zygote daemon (app_process) listening on /dev/socket/zygote, handle real app process forks, and wire IPC communication with servicemanager (Handle 0), ams_rs (Handle 4), and pms_rs (Handle 5).

### R3. Pure In-Guest SurfaceFlinger & VirtIO-GPU DRM Presentation
Link real surfaceflinger ELF with egl_webgpu.cpp, gralloc.virtio_gpu.so, and hwcomposer to drive /dev/dri/card0. Ensure full scanout lock to virtio_gpu_bridge without falling back to host LayoutInflater or mock view hierarchies, executing F-Droid's MainActivity lifecycle (onCreate -> onStart -> onResume).

### R4. Complete End-to-End Verification & Quality Gates
Enforce all bringup and visual quality gates: /dev/ttyS0 serial milestone logging, Logcat tag validation, damage rect propagation, frame entropy H >= 1.0, and passing all test suites (cargo test, pnpm test, tests/run_e2e_tests.mjs, tests/test_v86_guest_boot.mjs).

## Acceptance Criteria

### In-Guest Runtime & Assets
- [ ] guest/tools/generate_art_assets.mjs is removed or replaced by authentic asset extraction.
- [ ] guest/initrd/system/framework/boot.art and framework.jar contain authentic binary ART image and DEX bytecode.
- [ ] build_initrd.sh builds genuine initrd.img containing real framework and shared libraries.

### Process & IPC Lifecycle
- [ ] /dev/socket/zygote socket opens and accepts app fork requests in the guest VM.
- [ ] servicemanager registers activity (ams_rs) and package (pms_rs) native services.
- [ ] F-Droid MainActivity executes onCreate -> onStart -> onResume inside the guest environment.

### SurfaceFlinger & VirtIO-GPU Display
- [ ] SurfaceFlinger acquires /dev/dri/card0 and composites buffers to VirtIO-GPU scanout 0.
- [ ] WebGPU canvas receives real guest DRM framebuffers with Shannon entropy H >= 1.0.
- [ ] Host LayoutInflater mock/fallback is disabled when in-guest mode is active.

### Test & Gate Verification
- [ ] cargo test --workspace passes 100% with 0 failures.
- [ ] node tests/run_e2e_tests.mjs passes all tests.
- [ ] node tests/test_v86_guest_boot.mjs passes all assertions.
