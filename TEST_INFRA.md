# Test Infrastructure Specification (TEST_INFRA.md)

## 1. Overview
The AndroidWebGPU E2E Opaque-Box Test Suite validates the complete browser runtime, WebAssembly x86 VM, Linux virtio transport, WebGPU-accelerated graphics scanout, and fail-closed validation gates across four progressive tiers.

Testing operates strictly against documented interface contracts without inspecting internal private state or relying on synthetic production passes.

## 2. Test Harness Architecture (`tests/e2e/harness/`)
The test harness provides authoritative specification oracles and wire codecs located in `tests/e2e/harness/`:

| Module | File | Purpose |
|---|---|---|
| Assertions | `assertions.mjs` | Assertion utilities (`assertEqual`, `assertDeepEqual`, `assertOk`, `assertThrows`, `assertRejects`, `assertContract`, `assertGateStatus`). |
| Guest Memory | `guest_memory.mjs` | `GuestMemOracle`: Bounds-checked 32-bit physical RAM, DMA boundary check, integer wrap-around detection. |
| PCI & Virtqueue | `pci_virtio.mjs` | `PciConfigSpaceOracle` (ports 0xCF8/0xCFC, BDF mapping, ISR status), `VirtqueueSplitRingParser` (descriptor chains, loop detection, used/avail rings). |
| Wire Protocols | `protocol_virtio.mjs` | Spec wire encoders/decoders for Virtio-Blk, Virtio-Input (evdev), Virtio-Net, Virtio-GPU headers/commands. |
| WebGPU & Canvas | `gles_webgpu_mock.mjs` | Headless WebGPU mocks (`MockGPUDevice`, `MockGPUCanvasContext`, `MockCanvasElement`) strictly enforcing zero `putImageData` fallback and device loss simulation. |
| Runtime & Gates | `runtime_mock.mjs` | `MockAndroidRuntime`, truth-based validation loop runner, gate schema validation (`validateGateResult`, `validateRuntimeRunResult`). |
| Event Bus | `event_bus.mjs` | `EventBusSpy`: Lifecycle transitions (`VM_STATES`), runtime event contract validation. |
| Entry Point | `index.mjs` | Aggregated re-exports of all harness modules. |

## 3. Test Runner Design (`tests/e2e/runner.mjs`)
The runner utilizes the native Node.js test runner (`node:test`) and spec reporter (`node:test/reporters`):

- **Command**: `node tests/e2e/runner.mjs`
- **Filtering by Tier**:
  - `node tests/e2e/runner.mjs --tier=1` (Tier 1 Feature Coverage)
  - `node tests/e2e/runner.mjs --tier=2` (Tier 2 Boundary & Corner Cases)
  - `node tests/e2e/runner.mjs --tier=3` (Tier 3 Pairwise Combinations)
  - `node tests/e2e/runner.mjs --tier=4` (Tier 4 Real-World Application Scenarios)
- **Filtering by Feature**:
  - `node tests/e2e/runner.mjs --feature=f-vm-01`
  - `node tests/e2e/runner.mjs --tier=2 --feature=f-gpu-03`
- **Exit Code Contract**:
  - `0`: All matching test suites executed and passed.
  - `1`: One or more tests failed, or no tests matched criteria.

## 4. Test Tiers
1. **Tier 1: Feature Coverage (`tests/e2e/tier1-features/`)**
   - 25 test suites covering all features F-VM-01 through F-VAL-03.
   - At least 5 test cases per feature (125 tests total).
   - Validates happy paths and protocol compliance.
2. **Tier 2: Boundary & Corner Cases (`tests/e2e/tier2-boundaries/`)**
   - 25 test suites covering all features F-VM-01 through F-VAL-03.
   - At least 5 test cases per feature (125 tests total).
   - Validates buffer overflows, negative addresses, cyclic loops, illegal states, and format corruptions.
3. **Tier 3: Pairwise Combinatorial Tests (`tests/e2e/tier3-combinations/`)**
   - 6 test suites covering multi-subsystem interactions (30 tests total):
     - `vm_virtio_pairwise.test.mjs` (VM ↔ Virtio)
     - `virtio_devices_pairwise.test.mjs` (Virtio ↔ Devices)
     - `devices_gpu_pairwise.test.mjs` (Devices ↔ GPU)
     - `gpu_runtime_pairwise.test.mjs` (GPU ↔ Runtime)
     - `vm_runtime_pairwise.test.mjs` (VM ↔ Runtime)
     - `virtio_storage_gpu_pairwise.test.mjs` (Storage ↔ GPU)
4. **Tier 4: Real-World Scenarios (`tests/e2e/tier4-scenarios/`)**
   - 6 scenarios (30 tests total, production-backed):
      - `scenario_os_boot_chain.test.mjs` (OS boot sequence: BIOS -> Kernel -> Init -> Zygote -> SurfaceFlinger -> SystemUI)
      - `scenario_input_gpu_render.test.mjs` (DOM touch -> VirtioInput -> Framework -> GLES -> WebGPU present)
      - `scenario_disk_persistence.test.mjs` (SQLite userdata commit -> OPFS persistence -> VM reboot recovery)
      - `scenario_net_proxy_loop.test.mjs` (DNS & HTTP proxy loop over VirtioNet)
      - `scenario_device_loss_recovery.test.mjs` (WebGPU device crash -> context recreate -> resume without VM panic)
      - `scenario_http_sut.test.mjs` (live COOP/COEP server: entry, pkg, WASM, manifest hashes, fail-closed DOM)

## 5. Verification Commands
```bash
# Execute entire E2E test suite (62 suites, 311 tests)
node tests/e2e/runner.mjs

# Execute individual tiers
node tests/e2e/runner.mjs --tier=1
node tests/e2e/runner.mjs --tier=2
node tests/e2e/runner.mjs --tier=3
node tests/e2e/runner.mjs --tier=4
```
