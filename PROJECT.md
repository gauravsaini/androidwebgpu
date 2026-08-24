# Project: AndroidWebGPU Binder Subsystem Offloading & Browser Verification

## Architecture
Paravirtualized Binder IPC offloading pipeline routing selective Android IPC transactions across the VM boundary from Android-x86 guest into host Rust runtime and WebGPU compositor, verified in-browser via WASM, HTML5 WebGPU Test Bench, and Chrome DevTools MCP.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Browser Test Bench (index.html)                      │
│                                                                         │
│  [Tab: Binder Subsystem] ──► [src/binder_test_suite.js (Phases 0,2,3,4,5)]│
│                                           │                             │
│                                           ▼                             │
│                         [pkg/virtio_gpu_bridge_bg.wasm]                 │
│                                           │                             │
│                 ┌─────────────────────────┴────────────────────────┐    │
│                 ▼                                                  ▼    │
│  [Virtio-Binder Device / Bridge]                        [WebGPU Canvas] │
│                 │                                                  ▲    │
│  [binder_rt / aidl_compat / handle_bridge]                         │    │
│                 │                                                  │    │
│  [surfaceflinger_gpu_service] ─────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
```

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Parcel Alignment & Padding | 4-byte alignment and zero-padding formula | M1 | Survey Wire Spec |
| 2 | Parcel Scalar Codec | Little-endian primitives (bool, i8..i64, u8..u64, f32, f64) | M1 | Survey Wire Spec |
| 3 | Parcel String Codec | UTF-16LE and UTF-8 string serialization with null terminators and padding | M1 | Survey Wire Spec |
| 4 | Parcel Vectors & Arrays | Array serialization with count prefixes and nullable handling | M1 | Survey Wire Spec |
| 5 | Binder Object Serialization | `flat_binder_object` (24-byte) packing and offsets array tracking | M1 | Survey Wire Spec |
| 6 | File Descriptor Serialization | `BINDER_TYPE_FD` object packing and index mapping | M1 | Survey Wire Spec |
| 7 | Transaction Envelopes | 64-byte `binder_transaction_data`, `BC_TRANSACTION`, `BR_REPLY`, etc. | M1 | Survey Wire Spec |
| 8 | AIDL Status & Exceptions | Exception code serialization (`EX_NONE`, `EX_SERVICE_SPECIFIC`, etc.) | M1 | Survey Wire Spec |
| 9 | `binder::Interface` & `IBinder` | Base traits for AIDL interface definitions and transaction dispatch | M2 | Survey Wire Spec |
| 10 | `binder::SpIBinder` & `WpIBinder` | Smart pointer types with strong and weak reference count management | M2 | Survey Wire Spec |
| 11 | `binder::Remotable` & `Proxy` | `Bn*` stub and `Bp*` proxy traits for AIDL codegen | M2 | Survey Wire Spec |
| 12 | `binder::Parcelable` & Macros | Trait and macros for custom parcelable struct auto-derivation | M2 | Survey Wire Spec |
| 13 | Official AIDL Stub Compatibility | Ability to compile unmodified AOSP `aidl --lang=rust` stubs | M2 | Survey Wire Spec |
| 14 | Virtio-Binder Device & Protocol | Virtio MMIO/PCI queue device (Device ID 44) with request/response headers | M3 | Survey Wire Spec |
| 15 | Transport Dispatch Loop | Async queue descriptor processing, msg_id tracking, and response delivery | M3 | Survey Wire Spec |
| 16 | Virtio Event Queue | Out-of-band host-to-guest death notification and lifecycle event queue | M3 | Survey Wire Spec |
| 17 | Guest Interception Shim | Guest userspace/driver shim capturing targeted binder transactions | M3 | Survey Wire Spec |
| 18 | Bidirectional Handle Table | Mapping between guest handle IDs and host service trait objects (`Arc<dyn IBinder>`) | M4 | Survey Compositor |
| 19 | Distributed Reference Counting | Synchronized `BC_ACQUIRE` / `BC_RELEASE` tracking across the VM boundary | M4 | Survey Compositor |
| 20 | Multi-Hop Handle Transfer | Safe handle passing across multiple clients with reference preservation | M4 | Survey Compositor |
| 21 | Death Notification Propagation | `DeathRecipient` registration and automatic host resource cleanup on client death | M4 | Survey Compositor |
| 22 | Selective Routing Policy | Configurable routing rules defaulting to local guest kernel execution | M5 | Survey Compositor |
| 23 | Interface & Code Matcher | Routing decision based on interface descriptor and transaction opcode | M5 | Survey Compositor |
| 24 | Offloaded Compositor Service | Host-side `ISurfaceComposer` implementation receiving layer transactions | M5 | Survey Compositor |
| 25 | Layer State Translation | Mapping Android `layer_state_t` to `webgpu_compositor::CompositionLayer` | M5 | Survey Compositor |
| 26 | WebGPU Frame Presentation | Frame compositing and submission to `webgpu_swapchain` Mailbox target | M5 | Survey Compositor |
| 27 | E2E Integration Test Suite | Automated verification of Tiers 1-4 covering all features across VM boundary | M6 | Survey Codebase |
| 28 | Adversarial Coverage Hardening | White-box stress tests, race condition validation, and fuzzing (Tier 5) | M6 | Survey Codebase |
| 29 | WASM Virtio-Binder Bridge | WASM exports for Virtio-Binder request/reply dispatch and buffer extraction in `crates/virtio_gpu_bridge` | M7 | R2 Spec |
| 30 | Rebuilt WASM Artifact | Recompiled `pkg/` containing `WasmVirtioGpuBridge` binder bindings | M7 | R2 Spec |
| 31 | Browser Test Bench Extension | `index.html` `#tab-binder`, `#binder-card`, `#btn-run-binder`, and 5 phase badges | M8 | R1 Spec |
| 32 | Phase 0 Guest Baseline Check | In-browser test fixture verifying `/dev/binder` driver and `servicemanager` | M9 | R3 Spec |
| 33 | Phase 2 TestPing Roundtrip | In-browser test fixture verifying `TestPing` IPC roundtrip and byte equality | M9 | R3 Spec |
| 34 | Phase 3 Handles & Concurrency | In-browser test fixture verifying handle transfer, refcounting, and thread stress | M9 | R3 Spec |
| 35 | Phase 4 TestInput Forwarding | In-browser test fixture verifying Android input subsystem event forwarding | M9 | R3 Spec |
| 36 | Phase 5 SurfaceFlinger Compositor | In-browser test fixture verifying APK frame rendering to WebGPU canvas | M9 | R3, R4 Spec |
| 37 | WebGPU Pixel Color Assertions | Canvas readback asserting non-zero rendered pixels from SurfaceFlinger | M9 | R4 Spec |
| 38 | Chrome DevTools MCP E2E Execution | Automated test runner execution and badge state assertion against `http://localhost:8000` | M10 | R5 Spec |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | `binder_rt` | Parcel Codec & Wire Protocol Envelopes | none | DONE |
| M2 | `aidl_compat` | AIDL Interface Integration & Traits | M1 | DONE |
| M3 | `virtio_binder` | Paravirtualized Transport (Virtio-Binder Device & Shim) | M1 | DONE |
| M4 | `binder_handle_bridge` | Cross-Boundary Handle Bridge & Lifecycle Management | M1, M2 | DONE |
| M5 | `binder_routing_compositor` | Selective Routing Policy & Offloaded Compositor Service | M1, M2, M4 | DONE |
| M6 | `e2e_verification_hardening` | Full E2E Test Suite (Tiers 1-4) & Adversarial Hardening (Tier 5) | M1, M2, M3, M4, M5 | DONE |
| M7 | `wasm_binder_bridge` | Expose Virtio-Binder in WASM bridge and rebuild `pkg/` | M1-M6 | DONE |
| M8 | `browser_test_bench_ui` | Add "Binder Subsystem" tab, card, and badges to `index.html` | none | DONE |
| M9 | `binder_test_suite_js` | Implement 5-phase test suite (`src/binder_test_suite.js`) + pixel assertions | M7, M8 | DONE |
| M10 | `chrome_devtools_e2e` | Automated Chrome DevTools MCP execution against `http://localhost:8000` | M7, M8, M9 | DONE |

## Interface Contracts

### WASM Bridge ↔ JavaScript Runtime
- `WasmVirtioGpuBridge`:
  - `process_binder_packet(&self, packet: &[u8]) -> Vec<u8>`
  - `compose_and_present(&mut self) -> bool`
  - `get_scanout_framebuffer(&self, scanout_id: u32) -> Vec<u8>`
  - `get_scanout_damage(&self, scanout_id: u32) -> Vec<u32>`
  - `clear_scanout_damage(&mut self, scanout_id: u32)`

### JavaScript Test Suite ↔ DOM UI
- Container: `#binder-card`
- Tab Button: `#tab-binder`
- Trigger Button: `#btn-run-binder`
- Badges: `#badge-phase0`, `#badge-phase2`, `#badge-phase3`, `#badge-phase4`, `#badge-phase5`
- Global Result: `window.__BINDER_TEST_RESULTS__`

## Code Layout
```
crates/
├── binder_rt/                   # M1: Parcel codec and wire protocol
├── aidl_compat/                 # M2: AIDL Rust compatibility crate
├── virtio_binder/               # M3: Virtio transport device & shim
├── binder_handle_bridge/        # M4: Cross-boundary handle translation & lifecycle
├── binder_routing/              # M5: Selective routing policy engine
├── surfaceflinger_gpu_service/  # M5: Offloaded SurfaceFlinger service
├── tests_e2e_binder/            # M6: Rust native E2E test suite
└── virtio_gpu_bridge/           # M7: WASM bridge cdylib
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── bridge.rs
        └── wasm.rs
pkg/                             # M7: Compiled WebAssembly artifacts
index.html                       # M8: HTML5 test bench UI
src/
├── binder_test_suite.js         # M9: 5-Phase Guest Payload Test Suite
├── test_suite.js                # Existing Virtio-GPU / Vulkan suites
├── arcade_demo.js               # 3D arcade demo
└── virtio_gpu_device.js         # Virtqueue device emulation
```
