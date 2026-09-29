# Track A: guest-driven WebGPU triangle — architecture note

## Problem

The `pathn-sh` guest is real, but it emitted no GPU traffic: `VIRTIO_GPU_CMD_SUBMIT_3D`
existed in the contracts and a full `gles2wgpu` → WebGPU stack sat in the repo,
yet nothing in the guest ever produced a GPU command. Constructing `GpuCmd`s
in browser JS would have rendered *something* — but it would have been a demo,
not a guest-driven path. Track A exists to close the loop honestly: **every
byte that reaches the GPU must originate in the guest's own execution.**

## Pipeline (all real, no mocks)

```
guest AArch64 (triangle_submit_stream)          guest-image/src/guest.rs
  |  104 STRBs: VIRTIO_GPU_CMD_SUBMIT_3D packet
  v  (GPU_DATA 0x0A00_0000, submit via GPU_SUBMIT 0x0A00_0008)
U12 GPU MMIO port (real WasmHost::mem_store)    units/u12-orchestrator
  |  drain_gpu_submit() -> GpuSubmit
  v
U7 gpu-device: typed decode                     units/u7-gpu-device
  |  GpuCmd::Submit3D { ctx_id: 42, commands: 72-byte opcode stream }
  v
U13 webgpu_canvas (wasm32): U8 dispatch         units/u13-adapters/src/webgpu_canvas.rs
  |  Submit3DWire -> re-encoded 0x0207 binary packet
  v  (typed execute_command no-ops Submit3D; process_binary_wire_command
      is the only route to the private execute_submit_3d)
virtio_gpu_bridge -> gles2wgpu GlContext        crates/virtio_gpu_bridge, crates/gles2wgpu
  |  VIEWPORT / CLEAR / DRAW_ARRAYS on default_render_target (Rgba8UnormSrgb)
  v
U13 present(): copy_texture_to_texture          (same file)
  |  render target -> canvas wgpu::Surface texture (Rgba8UnormSrgb)
  v
real <canvas> in the browser                    www/gpu-acceptance.html
```

The browser host (`crates/web-host`) exposes this as `GpuCanvas`:
`create(canvas_id)` boots the bridge, `execute_pending(&mut PathnShell)`
drains one pending submit and executes it through U8 dispatch,
`readback()` returns the render target as 640×480 RGBA8 bytes, and
`present_canvas()` presents (headed browsers only — see below).
`pump()` is the headed convenience (execute + present). JS owns the
DOM and the pixel assertion; Rust owns the emulator and the GPU stack.

## Headless-Chromium ground truth (verified 2026-09-30)

Four environment facts, each verified empirically, that shape the
acceptance — none is worked around by faking pixels:

1. **`--virtual-time-budget` freezes the GPU.** `queue.submit` returns but
   Dawn never executes under virtual time, so `mapAsync`/readback hangs
   forever. Acceptance runs in real time via Playwright
   (`www/gpu-accept-driver.py`).
2. **Headless screenshots do not composite WebGPU canvases.** A pure-JS
   triangle submits cleanly yet `--screenshot` and `canvas.toDataURL()`
   return black. `readback()` is therefore the ground-truth pixel path:
   the real render target is copied to a staging buffer, mapped, and the
   bytes are asserted (center white, corner red, both colors present) and
   blitted to a 2D canvas for the screenshot.
3. **`getCurrentTexture()` loses the device headless.** Dawn reports
   `"A valid external Instance reference no longer exists"`; every later
   `mapAsync` fails. Headless flows must use `execute_pending` +
   `readback` and never present. `present()`/`pump()` remain for headed
   browsers, where the surface path is correct.
4. **`--use-vulkan=swiftshader` hangs `requestDevice()`.** Without it, the
   SwiftShader fallback device creates fine with
   `--enable-unsafe-webgpu --enable-features=Vulkan`.

## Deliberate design choices

- **Adapter-owned GL bootstrap.** The mini opcode stream (VIEWPORT, CLEAR,
  DRAW_ARRAYS) carries no shader, program, buffer, or attribute state, and the
  bridge errors (`"Program {} not found"`) without a bound program. So
  `WebGpuCanvas::new` uploads a fixed GLSL triangle pipeline (white fragment
  shader, one VBO with (-0.5,-0.5),(0.5,-0.5),(0.0,0.5)) through the public
  GLES2 entry points. This is stated openly: the guest defines *what* is
  drawn (viewport, clear color, draw call); the adapter provides the
  *how* (shaders) that the opcode vocabulary cannot express.
- **Surface from a fresh instance, configured with the bridge's device.**
  `VirtioGpuBridge::new` owns its WebGPU device internally (via
  `GlContext::new`); there is no API to inject an externally created one.
  The web backend's `surface.configure` passes the device straight to the
  canvas context, so any device from the same `navigator.gpu` works —
  verified in wgpu 24.0.5's `backend/webgpu.rs` (`acquire` uses the passed
  device's `context`, never re-derives it from the surface's instance).
- **Readback, not present, is the headless pixel path.** `present()` is a
  pure GPU texture copy and remains the headed-browser display path, but
  headless Chromium cannot composite or screenshot a WebGPU canvas, and
  `getCurrentTexture()` loses the device. So the wasm side exposes a
  blocking `readback()` (staging buffer + `map_async`) and the page
  asserts on those bytes, then blits them to a 2D canvas purely so the
  headless screenshot shows what the GPU actually rendered. No pixel is
  fabricated: the 2D canvas is a display of the readback bytes.
- **Failure is loud.** Missing canvas / WebGPU / `Rgba8UnormSrgb` format,
  U7 decode errors, `SignalFence` (Track A never issues fences), and
  `Unsupported` commands all return errors. Nothing is silently dropped.
- **wasm32 quarantine preserved.** The real adapter lives in its own module
  (`webgpu_canvas.rs`, `#[cfg(target_arch = "wasm32")]`); `lib.rs` and the
  native mocks are untouched, and the `conform_quarantine_no_os_io`
  source-scan still passes.

## Current scope (honest bounds)

- Single display, fixed 640×480, single `Rgba8UnormSrgb` format.
- One builtin (`triangle`); the guest's command vocabulary is the
  4 mini opcodes (VIEWPORT, CLEAR, DRAW_ARRAYS, DRAW_ELEMENTS).
- No fence tracking, no 2D scanout interplay, no resize.
- Acceptance: `www/gpu-acceptance.html` + `www/gpu-accept-driver.py` +
  `www/run-gpu-acceptance.sh` (distinct box dir `/mnt/sdb1/pathn-gpu-www/`,
  free port 8125, Playwright driving full Chrome-for-Testing in real time)
  — real headless Chromium, screenshot of the readback blit, in-page pixel
  assertion (canvas center white, corner red, both colors present). The
  script applies a strict gate: title exactly `ACCEPT-PASS gpu`, every
  result JSON entry passing, screenshot non-empty, SHA-256 recorded.
