# Original User Request

## Initial Request — 2026-08-24T18:22:10Z

Extend the browser test bench (`index.html`, `src/`) and WASM/v86 runtime to validate end-to-end guest Android Binder offloading to host Rust services and WebGPU rendering inside the browser, validated via `python3 -m http.server` and Chrome DevTools MCP.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: demo

## Requirements

### R1. Browser Test Bench Extension (`index.html`)
Extend the existing test page with a dedicated "Binder Subsystem" verification tab and UI cards covering the 5-phase validation matrix (Phase 0 ISO baseline, Phase 2 TestPing, Phase 3 TestHandles/TestConcurrent, Phase 4 TestInput, Phase 5 Unity/Godot SurfaceFlinger).

### R2. In-Browser WASM & v86 Virtio-Binder Bridge
Expose the `virtio_binder`, `binder_rt`, `aidl_compat`, `binder_handle_bridge`, `binder_routing`, and `surfaceflinger_gpu_service` crates through the WASM boundary (`crates/virtio_gpu_bridge` / `pkg/`), connecting v86 linear memory / virtqueue rings to host dispatch in the browser.

### R3. 5-Phase Guest Payload Test Suite (`src/binder_test_suite.js`)
Implement executable in-browser test fixtures covering:
- **Phase 0 (Baseline)**: Guest kernel `/dev/binder` and `servicemanager` baseline check.
- **Phase 2 (Ping)**: `TestPing` transaction roundtrip (TLV → Parcel → Host Rust → Reply Parcel).
- **Phase 3 (Handles & Concurrency)**: `TestHandles` multi-hop handle translation and `TestConcurrent` thread stress without leaks.
- **Phase 4 (Input)**: `TestInput` bridged Android input subsystem event forwarding.
- **Phase 5 (Compositor)**: Unity/Godot APK frame stream through `ISurfaceComposer`/`IGraphicBufferProducer` rendering to WebGPU canvas.

### R4. WebGPU Canvas Compositing in Browser
Verify that offloaded frame buffers rendered through the host WebGPU pipeline display correctly on the browser HTML5 canvas with pixel color assertions.

### R5. Chrome DevTools MCP Automated Verification
Provide automated test runner execution and badge state validation using Chrome DevTools MCP against `http://localhost:8000`.

## Acceptance Criteria

### Browser Test Bench
- [ ] Test page serves cleanly via `python3 -m http.server` and runs in Chrome with zero unhandled JavaScript errors or WASM panics.

### Phase 2: Ping Roundtrip
- [ ] `TestPing` transaction produces matching return code and byte-identical payload reply back to guest.

### Phase 3: Handles & Concurrency
- [ ] Multi-hop handle transfer and cross-VM reference counting survive concurrent execution without handle leaks or use-after-free.

### Phase 4: Bridged Service
- [ ] `TestInput` transactions route across virtio-binder to host Rust service and return valid status.

### Phase 5: WebGPU Frame Output
- [ ] SurfaceFlinger buffer submission renders non-zero composited frames onto the WebGPU canvas, confirmed by pixel readback assertions.

### Chrome DevTools Validation
- [ ] Automated execution via Chrome DevTools MCP runs all test phases and verifies all gate badges transition to `PASSED`.
