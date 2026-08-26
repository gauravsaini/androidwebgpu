§7 Endgame: Real Guest Boot → Real App Pixels
Problem
Everything visible today is synthetic — JS-drawn placeholder buffers and HTML DOM simulations. No real x86 CPU executes, no real Linux kernel boots, no real app renders pixels through the pipeline.

The real pipeline should be:


v86 (x86 CPU) → Linux kernel → virtio-gpu driver → virtqueue rings → 
Rust WASM host (VirtioGpuBridge) → SurfaceFlinger compositor → WebGPU canvas
Current State
Component	Status
v86 hypervisor	Downloaded — v86.wasm and libv86.js ready
SeaBIOS / VGA BIOS	Downloaded — seabios.bin and vgabios.bin in bios/
Linux kernel / ISO	Downloaded — linux4.iso in guest/build/
Dev Server & CSP	Updated — serve.py with unsafe-eval, wasm-unsafe-eval, COOP/COEP
Debug Logs	In Progress — structured [v86], [bridge], serial pipe logging added
Virtio-GPU wire parser	✅ Functional — tested in Rust crates
SurfaceFlinger compositor	✅ Functional — composites layers, presents to WebGPU swapchain
Guest userland	Pending — compiling i686 binaries
Logging & Observability Standard
Every phase must implement structured logging:

[v86] Prefix: WASM load, memory allocation, BIOS POST, boot milestones.
[bridge] Prefix: Virtio-GPU commands, buffer swaps, format conversions.
[compositor] Prefix: Frame composition, layer count, swapchain presentation.
[v86-serial] Prefix: Dmesg lines and shell output forwarded to logcat panel.
Proposed Changes
Phase 1: Real Linux Boot on WebGPU Canvas (MVP)
Get a real Linux kernel booting in v86, with framebuffer console output visible on the WebGPU canvas through the existing virtio-gpu pipeline.

IMPORTANT

v86 only supports 32-bit x86. Android-x86 kernels ≥4.4 often need SSE3+ which v86 lacks. Phase 1 uses a proven Buildroot/Alpine kernel first.

[NEW] guest/download_v86_assets.sh
Script to fetch v86 runtime + BIOS + a pre-built minimal Linux kernel:

Download v86.wasm and libv86.js from v86 releases (or pnpm add v86)
Download seabios.bin and vgabios.bin from v86 repo bios/ directory
Download pre-built Buildroot bzImage + rootfs from v86 demo images (or build custom via Buildroot)
Place into bios/ and guest/build/
[MODIFY] 
android.html
Add <script src="./v86/libv86.js"></script> to load v86 runtime
Wire V86Starter initialization in initGraphicsAndVM() with real BIOS + kernel
Connect v86's VGA framebuffer output to virtio-gpu pipeline (or initially just v86's built-in screen adapter for proof-of-life)
[MODIFY] 
v86_guest_manager.js
Remove simulateBootProgression() fallback when V86Starter is available
Wire real serial console output parsing to logcat panel
Add v86 event listeners for boot milestone detection from real dmesg
Phase 2: Virtio-GPU Virtqueue Integration
Connect v86's virtqueue ring buffers to the existing Rust VirtioGpuBridge so guest GPU commands flow through the real pipeline.

[MODIFY] 
virtio_gpu_device.js
Register VirtioGpuDevice as a PCI device on v86's bus (emulator.v86.io.pci)
Implement virtqueue descriptor ring consumer using SharedArrayBuffer from v86's guest memory
Route control queue (queue 0) commands through rustBridge.process_command_packet()
Handle guest→host DMA: map guest physical addresses to SharedArrayBuffer offsets for TransferToHost2d
[MODIFY] 
bridge.rs
Add process_virtqueue_descriptor() method that reads virtio descriptor chains from guest memory
Handle scatter-gather lists for large framebuffer transfers
Phase 3: Cross-Compiled Guest Userland
Build real i686 Linux ELF binaries that run inside the v86 guest.

Guest binaries needed (cross-compile --target i686-unknown-linux-gnu):
servicemanager — binder context manager (handle 0)
pms_rs — package manager service
ams_rs — activity manager service
wms_rs — window manager service
inputflinger_rs — input dispatcher
surfaceflinger_gpu_service — compositor client (talks to host via virtio-gpu)
[MODIFY] guest/initrd/init
Replace stub if [ -x ... ] checks with real binary launches
Add proper service readiness detection via binder ping
[NEW] Cargo.toml workspace member for i686-unknown-linux-gnu target
Conditional compilation: #[cfg(target_arch = "x86")] for guest mode
Guest services communicate via real /dev/binder ioctl, not host-side function calls
Phase 4: Android ART + Real App Rendering (Endgame)
WARNING

This phase has the highest uncertainty. v86 may not support the CPU features Android ART requires.

Real Zygote process forking app processes
Real ART/Dalvik executing DEX bytecode
Real Skia rendering through libhwui → eglSwapBuffers → virtio-gpu SUBMIT_3D
Host-side GLES2→WebGPU translation receives real draw calls
queueAppBufferToSurfaceFlinger() JS function gets deleted — guest renders directly
Alternative approach if v86 can't run ART:
Use Alpine Linux + Xorg + real Firefox (x86 build) as stepping stone:

Guest runs real X11 Firefox rendering to virtio-gpu framebuffer
Not Android, but real app pixels through the real pipeline
Proves the architecture end-to-end
Open Questions
IMPORTANT

Q1: v86 CPU feature limitations — v86 lacks SSE3/SSSE3/SSE4. Can a 32-bit Linux kernel with CONFIG_X86_32=y boot without these? Buildroot kernels work, but Android-x86 kernels may not. Should we start with Alpine/Buildroot as proof-of-life?

IMPORTANT

Q2: Virtio-GPU vs VGA framebuffer — v86 has built-in VGA/SVGA emulation. Should Phase 1 use v86's native screen adapter first (zero integration work) and add virtio-gpu in Phase 2? Or wire virtio-gpu from day 1?

IMPORTANT

Q3: Phase 1 scope — Should Phase 1 deliver just "Linux console booting on screen" (could be done in 1-2 days), or should it include a minimal GUI (fbterm/framebuffer graphics)?

Verification Plan
Phase 1
v86 boots real kernel → dmesg appears in serial console → logcat panel shows real boot messages
Linux login prompt visible on WebGPU canvas (via v86 screen adapter or virtio-gpu)
uname -a returns real kernel version, not simulated text
Phase 2
Guest virtio_gpu DRM driver initializes → fb0: virtio_gpudrmfb in dmesg
Guest writes pixels to framebuffer → pixels appear on WebGPU canvas through SurfaceFlinger compositor
Frame rate ≥15 FPS for static content
Phase 3
Real servicemanager starts inside guest → binder handle 0 responds to ping
Real pms_rs i686 binary registers with servicemanager
adb shell service list (via serial) shows real registered services
Phase 4
Real app renders UI through Skia → pixels visible on WebGPU canvas
queueAppBufferToSurfaceFlinger() deleted from android.html