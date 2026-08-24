# Project: AndroidWebGPU Binder Subsystem Offloading

## Architecture
Paravirtualized Binder IPC offloading pipeline routing selective Android IPC transactions across the VM boundary from Android-x86 guest into host Rust runtime and WebGPU compositor.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Android-x86 Guest                             │
│                                                                         │
│  [Guest App / HAL] ────► [Guest Interception / Routing Filter]          │
│                                │                                        │
│                 Default: Local │ Match: Offload                         │
│                                ▼                                        │
│                   [Guest /dev/binder Kernel]   [Virtio-Binder Driver]   │
└────────────────────────────────────────────────────────┬────────────────┘
                                                         │ Virtio Ring Descriptors
┌────────────────────────────────────────────────────────┼────────────────┐
│ Host Runtime                                           ▼                │
│                                            [Virtio-Binder Device]       │
│                                                        │                │
│                                              [binder-rt Codec]          │
│                                                        │                │
│                                              [aidl-compat Runtime]      │
│                                                        │                │
│                                              [Handle Bridge]            │
│                                                        │                │
│                                          [SurfaceFlinger GPU Service]   │
│                                                        │                │
│                                             [webgpu_compositor]         │
│                                                        │                │
│                                             [webgpu_swapchain]          │
│                                                        │                │
│                                               [WebGPU Device]           │
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

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | `binder_rt` | Parcel Codec & Wire Protocol Envelopes | none | DONE |
| M2 | `aidl_compat` | AIDL Interface Integration & Traits | M1 | DONE |
| M3 | `virtio_binder` | Paravirtualized Transport (Virtio-Binder Device & Shim) | M1 | DONE |
| M4 | `binder_handle_bridge` | Cross-Boundary Handle Bridge & Lifecycle Management | M1, M2 | DONE |
| M5 | `binder_routing_compositor` | Selective Routing Policy & Offloaded Compositor Service | M1, M2, M4 | DONE |
| M6 | `e2e_verification_hardening` | Full E2E Test Suite (Tiers 1-4) & Adversarial Hardening (Tier 5) | M1, M2, M3, M4, M5 | DONE |

## Interface Contracts

### `binder_rt` ↔ `aidl_compat`
- `binder_rt::Parcel`:
  - `pub fn write_i32(&mut self, val: i32) -> Result<()>`
  - `pub fn read_i32(&self, offset: &mut usize) -> Result<i32>`
  - `pub fn write_utf8(&mut self, val: Option<&str>) -> Result<()>`
  - `pub fn read_utf8(&self, offset: &mut usize) -> Result<Option<String>>`
  - `pub fn write_utf16(&mut self, val: Option<&str>) -> Result<()>`
  - `pub fn read_utf16(&self, offset: &mut usize) -> Result<Option<String>>`
  - `pub fn write_binder(&mut self, handle: u32, cookie: u64) -> Result<()>`
  - `pub fn read_binder(&self, offset: &mut usize) -> Result<flat_binder_object>`
- `binder_rt::Status`:
  - `pub fn new_service_specific_error(err: i32, msg: Option<&str>) -> Self`
  - `pub fn new_exception(code: ExceptionCode, msg: Option<&str>) -> Self`

### `binder_rt` ↔ `virtio_binder`
- `VirtioBinderReqHdr`:
  - `msg_id: u64`, `cmd: u32`, `target_handle: u32`, `code: u32`, `flags: u32`, `cookie: u64`, `data_size: u32`, `offsets_size: u32`
- `VirtioBinderRespHdr`:
  - `msg_id: u64`, `status: i32`, `result_code: i32`, `data_size: u32`, `offsets_size: u32`, `flags: u32`

### `aidl_compat` ↔ `binder_handle_bridge`
- `HandleBridge`:
  - `register_service(client_id: u32, descriptor: &str, service: Arc<dyn IBinder>) -> u32`
  - `get_service(client_id: u32, handle: u32) -> Option<Arc<dyn IBinder>>`
  - `acquire_ref(client_id: u32, handle: u32, count: usize) -> Result<()>`
  - `release_ref(client_id: u32, handle: u32, count: usize) -> Result<bool>`
  - `register_death_recipient(client_id: u32, handle: u32, cookie: u64) -> Result<()>`
  - `on_client_died(client_id: u32) -> Vec<(u32, u64)>`

### `binder_routing` ↔ `surfaceflinger_gpu_service`
- `RoutingPolicy`:
  - `route(descriptor: &str, code: u32) -> RouteAction`
- `SurfaceFlingerGpuService`:
  - Implements `android.gui.ISurfaceComposer` AIDL interface
  - Translates `setTransactionState` into `webgpu_compositor::CompositionLayer` updates
  - Submits composition to `webgpu_swapchain`

## Code Layout
```
crates/
├── binder_rt/                   # M1: Parcel codec and wire protocol
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── parcel.rs
│       ├── wire.rs
│       ├── status.rs
│       └── types.rs
├── aidl_compat/                 # M2: AIDL Rust compatibility crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── traits.rs
│       ├── pointer.rs
│       ├── status.rs
│       ├── proxy.rs
│       ├── stub.rs
│       └── macros.rs
├── virtio_binder/               # M3: Virtio transport device & shim
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── device.rs
│       ├── queue.rs
│       ├── protocol.rs
│       └── guest_shim.rs
├── binder_handle_bridge/        # M4: Cross-boundary handle translation & lifecycle
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── bridge.rs
│       ├── table.rs
│       └── death.rs
├── binder_routing/              # M5: Selective routing policy engine
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── policy.rs
│       └── matcher.rs
├── surfaceflinger_gpu_service/  # M5: Offloaded SurfaceFlinger service
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── service.rs
│       ├── layer_translator.rs
│       └── buffer_queue.rs
└── tests_e2e_binder/            # M6 / E2E Track: Comprehensive test suite
    ├── Cargo.toml
    └── tests/
        ├── tier1_feature_coverage.rs
        ├── tier2_boundary_corner.rs
        ├── tier3_pairwise_combinations.rs
        ├── tier4_realworld_applications.rs
        └── tier5_adversarial_hardening.rs
```
