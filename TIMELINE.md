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
- **Pass 4 (S9 Real APK Flight & Polish)**:
  - Created checked-in real APK fixtures (`fixtures/unity_cube.apk`, `fixtures/godot_gles2.apk`).
  - Implemented end-to-end APK GPU analyzer & Virtio-GPU Submit3D execution flight test (`crates/apk_gpu_analyzer/tests/apk_real.rs`).
  - Implemented DRM GEM buffer creation/mmap and command stream execbuffer in `guest/patches/egl_webgpu.cpp`.
  - Implemented canvas surface VSync presentation in `crates/webgpu_swapchain/src/swapchain.rs`.
  - Replaced heap attribute allocation with stack buffer in `pipeline.rs`.
- **Pass 5 (Visual Browser Test Bench & Live Chrome Validation)**:
  - Created interactive visual HTML5 test bench (`index.html`) with WebGPU canvas, dark-mode HUD, and 60 FPS Virtio animation loop.
  - Implemented JavaScript Virtio-GPU binary packet builder (`src/virtio_packet_builder.js`) conforming to OASIS 1.2 specification.
  - Implemented automated visual test suite runner (`src/test_suite.js`) with pixel-level RGB assertions.
  - Resolved `std::time::Instant` panic on `wasm32-unknown-unknown` by implementing platform-safe `PlatformInstant` via `js-sys::Date::now()` in `metrics_overlay`.
  - Configured WebGPU adapter limits and memory hints in `gles2wgpu/src/context.rs` to match browser WebAssembly environments.
  - Integrated `console_error_panic_hook` in `virtio_gpu_bridge/src/wasm.rs`.
  - Validated all 4 End-to-End Gates live in Chrome via Chrome DevTools Protocol (100% passed: 4/4):
    - Gate 1: 2D Scanout & Flush (PASSED)
    - Gate 2: 3D Submit GLES (PASSED)
    - Gate 3: Compositor & HUD (PASSED)
    - Gate 4: Real APK Flight Stream (PASSED)
  - Captured verified visual artifact screenshot and generated `walkthrough.md`.

## UPDATED ON : 2026-10-05

### fix (2026-10-05) — Track A1 Kernel Critical Path Halt-Chasing Beyond 150M to 200M Steps

1. **Kernel Halt-Chasing & Emulator Fixes**: Fixed CCMP register decode bit (bit 11 vs 10) resolving runaway `strchr` loop; added UDIV/SDIV decoding to `u1-decode` and `u12-orchestrator`; resolved LDP/LDPSW base register clobber (`rn == rt`) in `u2-ir-lift`; supported 64-bit ADD/SUB extended UXTB, 32/64-bit CLZ, 64-bit MADD/MSUB, and LDXP/LDAXP/STXP/STLXP atomic pairs; booted AOSP kernel to 200M steps halt-free with clean 1 GiB guest RAM panic search.
2. **Tests** (before → after): u1-decode 104 → 105 passed, u2-ir-lift 126 → 127 passed, u12-orchestrator 127 → 133 passed.
3. **Files changed**: `units/u1-decode/src/dp_reg.rs`, `units/u1-decode/src/lib.rs`, `units/u2-ir-lift/src/lib.rs`, `units/u12-orchestrator/src/lib.rs`, `units/u12-orchestrator/src/bin/run_kernel.rs`.

## T499 matched-oracle — 2026-10-15

- Started the matched-oracle lane and verified the actual `guest-inputs` hashes; pinned cortex-a53, 1024 MiB, exact DTB bootargs, input hashes, QEMU 11.1.2, and Rust source revision in `MANIFEST.json`.
- Matched the QEMU GIC version to the provided DTB's `arm,cortex-a15-gic` controller (GICv2). The first GICv3 run faulted before init because that machine setting contradicted the DTB; it was retained as raw setup evidence, not tagged as a guest wall.
- Native QEMU's guest-only UART reached `Run /init as init process`; the kernel unpacked the initramfs and PID 1 printed its rootfs and PASS markers.
- Captured the dynamic CSINV at `0xffffff8008354018` in both engines. The live second hit differs before the instruction (QEMU X8/NZCV `0xffffffffbfffffff`/`0xA`; Rust X8/NZCV `0x40000000`/`0x6`) and produces QEMU X24 `0x40000000` versus Rust X24 `0xffffffffbfffffff`.
- Matched-state control: QEMU and Rust execute the exact Image word `0xda818058` with identical relevant registers and PSTATE and both produce X24 `0x40000000`. The live divergence is in emulator state before CSINV, not the instruction's conditional-select result.
- Gated the memblock store rewrite and vmemmap-on-fault handler behind `--enable-guest-quirks`; default is off. Raw captures are in `evidence/t499/`; curated branch is `okf/branches/t499-matchedoracle.md`.

## T503 ORN fast-path fix — 2026-10-15

- Located the live T499 state divergence one instruction before CSINV: `ORN X8, XZR, X1` was executed as ORR because the U12 fast path ignored N.
- Fixed ORR/ORN for 32-bit and 64-bit forms; the focused test covers both ORR and ORN at each width and an LSL-shifted ORN form. Audited ANDS/BIC and the EOR/EON slow path.
- Rebuilt the release runner. With guest quirks disabled, Rust now matches pinned QEMU X8 and architectural PSTATE before CSINV and matches PC, X24, and PSTATE after it.
- Completed ten consecutive accepted QEMU/Rust live-state pairs with no mismatch. Raw logs and checksums are under `/Users/Shared/bundles/t503-ornfix-20261015/`.
- Recorded gates G503-1/2/3 as MET in `GATES-T503.md`; T503 is a separate OKF branch. Full workspace testing also reports the unrelated vmemmap LDRB/MMU translation failure.

## T506 fresh Rust PID-1 boot — 2026-10-16

- Opened a separate full-boot branch from T503's `ec0a017` fixed revision; all three T499 manifest input hashes match.
- Gate protocol requires guest quirks disabled, a guest-originating exact PID-1 UART marker, and ten consecutive fresh runs for any reproducibility claim. Boot outcome is pending.
- Carried forward T503's 349/350 workspace test result and its unrelated LDRB/MMU translation failure.
