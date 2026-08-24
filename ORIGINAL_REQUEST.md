# Original User Request

## Initial Request — 2026-08-24T11:32:44Z

Implement the complete Binder subsystem offloading pipeline for AndroidWebGPU, enabling selective routing of Android Binder IPC transactions across the VM boundary from an Android-x86 guest into a host Rust runtime and WebGPU compositor.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: demo

## Requirements

### R1. Parcel Codec & Wire Protocol
Implement a host-side `binder-rt` crate that encodes and decodes AOSP-compatible `Parcel` structures and transaction envelopes across the VM boundary without relying on guest `/dev/binder`.

### R2. AIDL Interface Integration
Provide an `aidl-compat` shim crate allowing official AOSP `aidl --lang=rust` generated stubs to build and execute against `binder-rt`.

### R3. Paravirtualized Transport (Virtio-Binder)
Implement a guest interception mechanism and a host v86-compatible virtio queue device to exchange transaction requests and responses across the guest/host memory boundary.

### R4. Cross-Boundary Handle Bridge & Lifecycle
Implement a handle translation table that maps guest handles to host service objects, maintains distributed reference counts (`acquire`/`release`), and propagates death notifications across the VM boundary.

### R5. Selective Routing Policy & Offloaded Compositor Service
Implement a configurable routing policy table defaulting to local guest execution, and provide a host-side Rust service implementing the offloaded transaction path connected to the host WebGPU compositor.

## Acceptance Criteria

### Codec & AIDL Compatibility
- [ ] `binder-rt` decodes and reproduces captured AOSP `Parcel` payloads byte-for-byte in automated unit tests.
- [ ] Generated Rust code from official AOSP `aidl` compiler builds cleanly against `aidl-compat` without manual stub edits.

### Transport & Round-Trip Dispatch
- [ ] Synthetic test transaction sent from guest driver across virtio queue is answered by host runtime and delivered back to the guest with matching return code and payload.

### Lifecycle & Concurrency Resilience
- [ ] Cross-VM handle table preserves correct reference counts across multi-hop transfers and releases host resources on simulated client termination without leaks or use-after-free.

### End-to-End Compositing
- [ ] Routed rendering transactions produce a rendered frame submitted and displayed through the host WebGPU pipeline.
