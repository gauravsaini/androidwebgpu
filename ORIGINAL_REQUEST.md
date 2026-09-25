# Original User Request

## 2026-09-25T04:34:11Z

Boot a real Android-x86 guest OS inside a browser tab using a WebAssembly x86 VM, Linux virtio devices, WebGPU-accelerated display scanout, and fail-closed validation gates.

Working directory: /Users/admin/Desktop/androidwebgpu
Integrity mode: development

References:
- Architecture & lane contract: plan.md
- Gate ledger: GATES.md

## Requirements

### R1. Browser Runtime Bundle & Packaging
Build a self-contained browser distribution producing `pkg/android_vm.js` and WASM artifacts via standard build commands. The production bundle entry must not rely on synthetic test mocks or preset ready states.

### R2. WebAssembly x86 VM & Virtio Subsystem
Integrate an x86 VM core in a web worker with a complete virtio transport (PCI configuration, queues, IRQ, checked DMA) supporting block, console, input, network, RNG, RTC, and virtio-gpu devices.

### R3. WebGPU-Backed Virtio-GPU Display Pipeline
Expose a compliant virtio-gpu device that validates descriptor chains, attaches guest DMA backing memory, handles command submission with fence completion, and presents rendered frames to a WebGPU canvas surface.

### R4. Android Guest Boot & Userland Execution
Mount verified, pinned Android-x86 images (kernel, initrd, system, vendor, userdata) and execute the guest OS through init, zygote, and SurfaceFlinger until SystemUI renders and interactive applications function.

### R5. Fail-Closed Validation Loop
Implement a truth-based validation engine in the browser exposing `window.runValidationLoop()` that evaluates gates G0 through G9 dynamically, reporting an immutable run record without false-positive or synthetic passes.

## Acceptance Criteria

### Build & VM Bootstrap
- [ ] Clean build generates `pkg/android_vm.js` and all WASM assets without missing-file errors
- [ ] Browser loads entry point under HTTP with dynamic runtime import, reporting real status
- [ ] x86 VM initializes, boots through BIOS/kernel, emits serial console logs, and shuts down without resource leaks

### Virtio Transport & Guest I/O
- [ ] VirtioBus negotiates features and correctly validates descriptor chains, indirect tables, and boundaries
- [ ] Virtio block driver performs read/write operations and preserves userdata across VM reboots and browser page reloads
- [ ] Virtio console captures guest kernel logs; virtio input delivers DOM keyboard, mouse, and touch events to guest evdev
- [ ] Virtio network provides functional TCP/DNS connectivity over the declared browser proxy mode, and audio output starts cleanly

### GPU Pipeline & Display
- [ ] Virtio-gpu handles guest driver probes, attaches DMA backing pages, and signals fence completions
- [ ] Guest render and compositor passes output to WebGPU canvas scanout without fallback to 2D canvas `putImageData`
- [ ] WebGPU device loss and context restoration recover cleanly without corrupting guest state

### Android OS & System Acceptance
- [ ] Pinned Android-x86 guest reaches SurfaceFlinger and renders visible SystemUI frames
- [ ] An interactive GLES Android application launches, renders graphics, and processes input events
- [ ] Browser `window.runValidationLoop()` records all ten gates G0–G9 as PASSED in a single run epoch
- [ ] Verification script `node scripts/verify-runtime-artifacts.mjs` exits successfully with `ANDROID_RUNTIME_ACCEPTED`
- [ ] Gate verification `node scripts/verify-gates.mjs` and plan check `node scripts/verify-plan.mjs` pass cleanly

## 2026-09-25T06:11:23Z

User audit findings & gate requirements:

- 🔴 [vm_worker.js:241-252] no x86 CPU executes; loop only increments instructionsExecuted. Wire real v86/CPU before claiming VM boot.
- 🔴 [vm_worker.js:114-118] IRQ callback is empty; guest interrupts never reach CPU/PIC.
- 🔴 [vm_worker.js:222-224] input handler is empty; guest input is dropped.
- 🔴 [vm_runtime.js:255-280] non-SharedArrayBuffer mode gives worker a new RAM buffer; boot data staged by main thread is invisible to worker. Transfer/shared-memory contract must be fixed.
- 🔴 [m1_vm_virtio_test.mjs:801-825] watchdog test fails: RUNNING !== ERROR. Fix worker abstraction or watchdog path before M1 is green.
- 🔴 [virtqueue.js:332-342] EVENT_IDX notify logic lacks the old used index and does not implement vring_need_event; interrupts can be missed.
- 🟡 [virtio_pci_device.js:94-96] advertises indirect/event-index features unconditionally while event-index behavior is incomplete. Advertise only proven features.
- 🔴 [f_val_01_validation_loop.test.mjs:3-8] validates MockAndroidRuntime, not production runtime.
- 🔴 [f_vio_01_pci_bus.test.mjs:4-18] uses PciConfigSpaceOracle; no production PCI code is invoked.
- 🔴 [f_vm_01_lifecycle.test.mjs:7-63] tests local strings, not VmRuntime.
- 🔴 [f_gpu_03_webgpu_present.test.mjs:4-48] tests mock GPU/canvas objects, not real WebGPU or swapchain code.
- 🔴 [f_bld_01_packaging.test.mjs:23-53] validates fake module exports and fake WASM bytes; it never loads pkg/android_vm.js.
- 🔴 [f_val_02_artifact_accept.test.mjs:10-31] requires missing artifacts, so it will fail once M4/M5 correctly produce them. Split blocked-state and success-state tests.
- 🔴 [tests/e2e/] has no Tier 3 or Tier 4 suites. The runner supports them, but both commands return “No test files matched”.
- 🟡 [f_val_03_http_bootstrap.test.mjs:6-29] does static string scans only; it does not open HTTP, import the module, or inspect live DOM state.

Handoff instructions:
M2–M5 can proceed, but keep these gates open:
1. Real CPU/v86 execution.
2. Shared guest RAM and worker I/O.
3. IRQ/input wiring.
4. Production-runtime E2E adapter.
5. T3/T4 test tracks.
6. Green M1 unit suite.

