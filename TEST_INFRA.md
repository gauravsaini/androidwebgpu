# E2E Test Infra: AndroidWebGPU Binder Subsystem Offloading

## Test Philosophy
- Opaque-box, requirement-driven. No dependency on implementation internals.
- Methodology: Category-Partition + Boundary Value Analysis + Pairwise Combinatorial + Real-World Workload Testing.

## Feature Inventory & Test Mapping
| # | Feature | Requirement Source | Tier 1 (Coverage) | Tier 2 (Boundary) | Tier 3 (Cross) | Tier 4 (Real-World) |
|---|---------|-------------------|:-----------------:|:-----------------:|:--------------:|:-------------------:|
| 1 | Parcel Alignment & Padding | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 2 | Parcel Scalar Codec | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 3 | Parcel String Codec (UTF-8/UTF-16) | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 4 | Parcel Vectors & Arrays | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 5 | Binder Object (`flat_binder_object`) | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 6 | File Descriptor Serialization | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 7 | Transaction Envelopes (`BC_*`/`BR_*`) | ORIGINAL_REQUEST §R1 | 5 | 5 | ✓ | ✓ |
| 8 | AIDL Status & Exceptions | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 9 | `binder::Interface` & `IBinder` | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 10 | `SpIBinder` & `WpIBinder` Pointers | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 11 | `Remotable` & `Proxy` Traits | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 12 | `Parcelable` Trait & Macros | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 13 | Official AOSP AIDL Stub Compatibility | ORIGINAL_REQUEST §R2 | 5 | 5 | ✓ | ✓ |
| 14 | Virtio-Binder Device & Headers | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 15 | Transport Dispatch Loop & `msg_id` | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 16 | Virtio Event Queue & Death Notification | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 17 | Guest Interception Shim | ORIGINAL_REQUEST §R3 | 5 | 5 | ✓ | ✓ |
| 18 | Bidirectional Handle Table | ORIGINAL_REQUEST §R4 | 5 | 5 | ✓ | ✓ |
| 19 | Distributed `acquire`/`release` Refcounts | ORIGINAL_REQUEST §R4 | 5 | 5 | ✓ | ✓ |
| 20 | Multi-Hop Handle Passing | ORIGINAL_REQUEST §R4 | 5 | 5 | ✓ | ✓ |
| 21 | Death Recipient & Process Cleanup | ORIGINAL_REQUEST §R4 | 5 | 5 | ✓ | ✓ |
| 22 | Selective Routing Policy (Default Local) | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 23 | Interface & Code Matching Rules | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 24 | Offloaded Compositor Service | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 25 | Layer State Translation | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |
| 26 | WebGPU Frame Presentation & Readback | ORIGINAL_REQUEST §R5 | 5 | 5 | ✓ | ✓ |

## Test Architecture
- Test runner: `cargo test --workspace` and `cargo test -p tests_e2e_binder`
- Test case format: Rust integration test crates and modules testing through public crate interfaces and synthetic virtio queues.
- Directory layout: `crates/tests_e2e_binder/tests/`

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Full Lifecycle Ping & Status Round-Trip | F1, F2, F7, F8, F9, F14, F15 | Medium |
| 2 | Multi-Client Surface Allocation & Concurrent Render | F4, F5, F9, F10, F18, F19, F24, F26 | High |
| 3 | Multi-Hop Handle Pass & Cross-Client Layer Update | F5, F18, F19, F20, F24, F25 | High |
| 4 | Abrupt Client Crash & Host Resource Teardown | F16, F18, F19, F21, F24 | High |
| 5 | Full Android HWC Multi-Layer SurfaceFlinger Frame Submission | F1..F7, F14..F16, F22..F26 | Very High |
| 6 | Selective Routing Mixed Workload (Local Pass-Through + Offloaded 3D) | F7, F22, F23, F24, F26 | High |

## Coverage Thresholds
- Tier 1: ≥ 130 tests (5 per feature across 26 features)
- Tier 2: ≥ 130 tests (5 boundary/corner cases per feature)
- Tier 3: ≥ 26 pairwise combination tests
- Tier 4: ≥ 6 realistic application scenarios
- Tier 5: Adversarial stress testing & race validation
- Total: ≥ 292 comprehensive tests
