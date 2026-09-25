# Test Readiness Report (TEST_READY.md)

## Status: BLOCKED ⛔ (M1 + M-E2E sign-off withheld)
Host suites pass (E2E 62/62 · 311/311, all production-backed; M1 22/22, stress 13/13, fuzz 17/17, v86-adapter 3/3, Rust 21).
LIVE GUEST MILESTONE (2026-09-25): real Android-x86 9.0-r2 32-bit kernel boots
under the vendored v86 core — `Linux version 4.19.110`, `/init`, `Detecting
Android-x86` on serial (docs/boot-evidence-9.0-r2-serial.log, 18KB).
Sign-off still blocked until: system/vendor/product squashfs media,
SurfaceFlinger/SystemUI frames, and G0–G9 PASSED in one browser epoch.

## Test Summary
- **Total Test Suites**: 62
- **Total Test Cases**: 311
- **Passed**: 311 (100%)
- **Failed**: 0
- **Skipped / Cancelled**: 0
- **Execution Duration**: ~2.3s

## Tier Breakdown

### Tier 1: Feature Coverage (25 suites / 125 tests)
Directory: `tests/e2e/tier1-features/`
- F-VM-01: v86 x86 VM Web Worker runner & lifecycle (`f_vm_01_lifecycle.test.mjs` - 5 tests)
- F-VM-02: Bounds-checked GuestMem DMA view (`f_vm_02_guest_mem.test.mjs` - 5 tests)
- F-VIO-01: Virtio PCI bus & configuration space router (`f_vio_01_pci_bus.test.mjs` - 5 tests)
- F-VIO-02: Virtqueue split ring buffer parser & descriptor validation (`f_vio_02_virtqueue.test.mjs` - 5 tests)
- F-VIO-03: Virtio IRQ assertion & latched status management (`f_vio_03_irq.test.mjs` - 5 tests)
- F-DEV-01: VirtioBlk device with OPFS/IndexedDB persistence (`f_dev_01_blk.test.mjs` - 5 tests)
- F-DEV-02: VirtioConsole device with serial log streaming (`f_dev_02_console.test.mjs` - 5 tests)
- F-DEV-03: VirtioInput device with DOM to evdev translation (`f_dev_03_input.test.mjs` - 5 tests)
- F-DEV-04: VirtioNet user-mode proxy network device (`f_dev_04_net.test.mjs` - 5 tests)
- F-DEV-05: VirtioRng entropy device via crypto.getRandomValues (`f_dev_05_rng.test.mjs` - 5 tests)
- F-DEV-06: CMOS RTC & monotonic clock device (`f_dev_06_rtc.test.mjs` - 5 tests)
- F-DEV-07: VirtioSound audio output device (`f_dev_07_sound.test.mjs` - 5 tests)
- F-GPU-01: Virtio-GPU DMA backing attach/detach (`f_gpu_01_attach_backing.test.mjs` - 5 tests)
- F-GPU-02: Spec TRANSFER_TO_HOST_2D from guest physical RAM (`f_gpu_02_transfer_2d.test.mjs` - 5 tests)
- F-GPU-03: Direct WebGPU canvas surface present (strictly no putImageData) (`f_gpu_03_webgpu_present.test.mjs` - 5 tests)
- F-GPU-04: 3D opcode slice fix & GLES translation hookup (`f_gpu_04_3d_gles.test.mjs` - 5 tests)
- F-GPU-05: Monotonic fence completion & async work synchronization (`f_gpu_05_fence_sync.test.mjs` - 5 tests)
- F-GPU-06: WebGPU device loss and context recovery (`f_gpu_06_device_loss.test.mjs` - 5 tests)
- F-GST-01: Android guest boot media & images/manifest.json (`f_gst_01_boot_media.test.mjs` - 5 tests)
- F-GST-02: Android guest HAL fixes (egl, gralloc, hwcomposer) (`f_gst_02_hal_fixes.test.mjs` - 5 tests)
- F-BLD-01: Runtime artifact packaging (`f_bld_01_packaging.test.mjs` - 5 tests)
- F-BLD-02: Build scripts & tooling (`f_bld_02_tooling.test.mjs` - 5 tests)
- F-VAL-01: Dynamic truth-based validation loop evaluating G0-G9 (`f_val_01_validation_loop.test.mjs` - 5 tests)
- F-VAL-02: Runtime artifact acceptance (`f_val_02_artifact_accept.test.mjs` - 5 tests)
- F-VAL-03: Fail-closed HTTP bootstrap and UI state reporting (`f_val_03_http_bootstrap.test.mjs` - 5 tests)

### Tier 2: Boundary & Corner Cases (25 suites / 125 tests)
Directory: `tests/e2e/tier2-boundaries/`
- F-VM-01-B: Illegal state transitions, rapid oscillation, reset (`f_vm_01_boundary.test.mjs` - 5 tests)
- F-VM-02-B: Out-of-bounds RAM reads, 32-bit wrap overflow, zero-length DMA (`f_vm_02_boundary.test.mjs` - 5 tests)
- F-VIO-01-B: Unpopulated BDF abort, unaligned config offset, disabled config write (`f_vio_01_boundary.test.mjs` - 5 tests)
- F-VIO-02-B: Cyclic descriptor loop, non-power-of-two queue, index wrap (`f_vio_02_boundary.test.mjs` - 5 tests)
- F-VIO-03-B: Spurious ISR read, duplicate assertion, multi-interrupt latch (`f_vio_03_boundary.test.mjs` - 5 tests)
- F-DEV-01-B: Negative block sectors, corrupt status bytes, oversize request (`f_dev_01_boundary.test.mjs` - 5 tests)
- F-DEV-02-B: Null console byte, ANSI escape sequences, buffer saturation (`f_dev_02_boundary.test.mjs` - 5 tests)
- F-DEV-03-B: Negative coordinates, undefined key codes, out-of-order SYN (`f_dev_03_boundary.test.mjs` - 5 tests)
- F-DEV-04-B: Giant MTU frame, zero-byte packet, checksum offload corner cases (`f_dev_04_boundary.test.mjs` - 5 tests)
- F-DEV-05-B: Zero-length entropy request, large entropy buffer chunking (`f_dev_05_boundary.test.mjs` - 5 tests)
- F-DEV-06-B: Leap year calculation, rollover, epoch boundaries (`f_dev_06_boundary.test.mjs` - 5 tests)
- F-DEV-07-B: Sample rate clamps, underrun, zero-length PCM write (`f_dev_07_boundary.test.mjs` - 5 tests)
- F-GPU-01-B: Duplicate resource ID, zero dimensions, empty backing list (`f_gpu_01_boundary.test.mjs` - 5 tests)
- F-GPU-02-B: Transfer without backing, rect out-of-bounds, negative coords (`f_gpu_02_boundary.test.mjs` - 5 tests)
- F-GPU-03-B: Unsupported surface format, present unconfigured context (`f_gpu_03_boundary.test.mjs` - 5 tests)
- F-GPU-04-B: Zero count draw elements, invalid GLES mode/index types (`f_gpu_04_boundary.test.mjs` - 5 tests)
- F-GPU-05-B: Fence ID zero, non-monotonic fence IDs, 64-bit max fence (`f_gpu_05_boundary.test.mjs` - 5 tests)
- F-GPU-06-B: Operations on destroyed device, multiple loss triggers (`f_gpu_06_boundary.test.mjs` - 5 tests)
- F-GST-01-B: Unsupported guest ABI, missing image partition, bad sha256 (`f_gst_01_boundary.test.mjs` - 5 tests)
- F-GST-02-B: Undersized swap buffer packet, egl null context/surface (`f_gst_02_boundary.test.mjs` - 5 tests)
- F-BLD-01-B: Missing initWasm/createAndroidRuntime, corrupt WASM magic (`f_bld_01_boundary.test.mjs` - 5 tests)
- F-BLD-02-B: Missing headers in GATES.md, duplicate gate IDs, bad badge counts (`f_bld_02_boundary.test.mjs` - 5 tests)
- F-VAL-01-B: Unknown gate IDs, invalid statuses, false-green detection (`f_val_01_boundary.test.mjs` - 5 tests)
- F-VAL-02-B: Partial artifacts missing, empty directory, manifest validation (`f_val_02_boundary.test.mjs` - 5 tests)
- F-VAL-03-B: Premature ONLINE badge, missing fail-closed paths (`f_val_03_boundary.test.mjs` - 5 tests)

### Tier 3: Pairwise Combinatorial Tests (6 suites / 30 tests)
Directory: `tests/e2e/tier3-combinations/`
- VM ↔ Virtio: `vm_virtio_pairwise.test.mjs` (5 tests)
- Virtio ↔ Devices: `virtio_devices_pairwise.test.mjs` (5 tests)
- Devices ↔ GPU: `devices_gpu_pairwise.test.mjs` (5 tests)
- GPU ↔ Runtime: `gpu_runtime_pairwise.test.mjs` (5 tests)
- VM ↔ Runtime: `vm_runtime_pairwise.test.mjs` (5 tests)
- Storage ↔ GPU: `virtio_storage_gpu_pairwise.test.mjs` (5 tests)

### Tier 4: Real-World Scenarios (6 suites / 30 tests)
Directory: `tests/e2e/tier4-scenarios/`
- Scenario 1: OS Boot Chain (`scenario_os_boot_chain.test.mjs` - 5 tests)
- Scenario 2: Input-to-GPU Render Cycle (`scenario_input_gpu_render.test.mjs` - 5 tests)
- Scenario 3: Disk Commit & Persistence (`scenario_disk_persistence.test.mjs` - 5 tests)
- Scenario 4: Network Proxy Loop (`scenario_net_proxy_loop.test.mjs` - 5 tests)
- Scenario 5: WebGPU Device Loss & Context Recovery (`scenario_device_loss_recovery.test.mjs` - 5 tests)
- SUT: HTTP production server & bundle (`scenario_http_sut.test.mjs` - 5 tests, production imports)

## Invocation Commands

```bash
# Run all 62 test suites (311 tests)
node tests/e2e/runner.mjs

# Run specific tier
node tests/e2e/runner.mjs --tier=1
node tests/e2e/runner.mjs --tier=2
node tests/e2e/runner.mjs --tier=3
node tests/e2e/runner.mjs --tier=4

# Run specific feature across tiers
node tests/e2e/runner.mjs --feature=f-vm-01
node tests/e2e/runner.mjs --feature=f-gpu-03
```
