# AndroidWebGPU — Binder Subsystem Plan

## 0. Premise (locked in)

Binder work splits into two independent tracks that must not be conflated:

| | Guest-side Binder | Host-side virtio-binder |
|---|---|---|
| Where | Inside the v86 VM (Android-x86 kernel) | Between guest and browser/WASM host |
| What | Real Linux kernel binder driver, unmodified AMS/WMS/PMS | New paravirtualized transport + new Rust runtime |
| Effort | Zero rewrite — enable `CONFIG_ANDROID_BINDER_IPC`, done | 100% new engineering |
| rsbinder role | Not used | Used **only** for the Parcel / wire-struct layer inside `binder-rt` — not for AIDL, not as a driver |

Everything below assumes this fork. Do not build a Rust binder driver for the guest — that work has no payoff.

**AIDL note:** no custom AIDL parser (not rsbinder-aidl, not hand-written). AIDL frontend is AOSP's official `aidl` compiler binary, shelled out to. See Phase 1.

---

## 1. Goals / non-goals

**Goals**
- Guest Android boots and runs AMS/WMS/PMS/SurfaceFlinger etc. completely unmodified via real kernel binder.
- A defined, minimal set of Binder transactions can be selectively routed out of the guest to a host-side Rust runtime.
- Host-side Rust services (starting with a WebGPU compositor) can serve those transactions and return valid Binder-compatible responses.
- Guest never has to know its call was served off-VM — from its perspective it's a normal Binder reply.

**Non-goals (explicitly out of scope)**
- Reimplementing the Binder kernel driver.
- Reimplementing ServiceManager for the guest (guest keeps its real one).
- Routing *all* Binder traffic through virtio — only the handful of services worth offloading.

---

## 2. Workspace layout

```
androidwebgpu/
├── guest/
│   └── kernel-config/          # CONFIG_ANDROID_BINDER_IPC + virtio-binder guest driver source
│       └── virtio_binder_shim/ # intercept + forward selected transactions
├── virtio-binder/
│   ├── proto/                  # TLV wire format spec + codec (shared guest/host)
│   ├── guest-driver/           # guest-side virtio queue producer
│   └── host-device/            # v86-side emulated virtio device (queue consumer)
├── host-runtime/
│   ├── binder-rt/              # adapted from rsbinder: Parcel, flat_binder_object, wire structs only
│   ├── aidl-compat/            # trait/type shim so AOSP `aidl --lang=rust` output compiles against binder-rt
│   ├── handle-bridge/          # guest handle <-> host object mapping + refcounting
│   └── services/
│       └── surfaceflinger-gpu/ # first offloaded service (see §6)
└── routing/
    └── policy.rs                # which interface/method pairs cross the bridge
```

---

## 3. Phase plan

### Phase 0 — Guest baseline (no new code, just config)
- [ ] Build Android-x86 kernel with `CONFIG_ANDROID_BINDER_IPC=y`, `CONFIG_ANDROID_BINDERFS=y`.
- [ ] Boot in v86, confirm `/dev/binder` exists, confirm `servicemanager`, `system_server` come up.
- [ ] Confirm AMS/WMS/PMS respond to a basic `dumpsys` inside the guest.
- **Exit criteria:** stock Android boots to launcher inside v86 with zero host involvement.

### Phase 1 — TLV wire format + host binder-rt skeleton
- [ ] Define TLV envelope for a Binder transaction crossing the boundary: `{ target_handle, code, flags, parcel_bytes, offsets[] }`.
- [ ] Adapt `binder-rt` from rsbinder's Parcel/wire-struct code: keep Parcel read/write, `flat_binder_object`, transaction structs. Strip anything assuming local `/dev/binder`. This is the *only* place rsbinder is used.
- [ ] AIDL frontend — **no custom parser**. Shell out to AOSP's official `aidl` binary with `--lang=rust` for the one or two interfaces being offloaded first (not the whole AOSP tree).
- [ ] Build `aidl-compat`: a thin shim crate whose public types/traits match AOSP's official `binder` Rust crate surface (`Interface`, `IBinder`, `Strong<T>`, `BpXxx`/`BnXxx` naming) but internally delegate to `binder-rt`. Goal: official `aidl --lang=rust` output compiles unmodified against your runtime.
  - Fallback if full compatibility proves too fiddly: hand-adapt the generated output per-interface instead of chasing 100% shim compatibility — cheaper for a small, fixed interface set.
- [ ] Unit test: encode a known transaction on paper (captured via `strace` from a real device) → decode with `binder-rt` → byte-identical roundtrip.
- **Exit criteria:** `binder-rt` can parse/produce real AOSP-compatible Parcels, and one AIDL interface compiled via the official `aidl` tool builds and runs against `binder-rt` — with no VM involved yet.

### Phase 2 — virtio-binder transport
- [ ] Guest driver: kernel module or userspace shim that hooks the transactions matching the routing policy (§5) *before* they hit `/dev/binder`, and instead writes them to a virtio queue.
- [ ] Host device: v86-side emulated virtio device — descriptor ring reader/writer against guest's linear memory / SharedArrayBuffer.
- [ ] Wire the two together: guest write → host device read → deliver into `binder-rt` dispatch → host service handles it → response written back through the same queue → guest driver injects the reply into the waiting Binder call.
- [ ] Round-trip test: a synthetic AIDL "ping" service, called from a guest test app, answered entirely on the host.
- **Exit criteria:** a guest app can call a host-only Binder interface and get a correct reply, indistinguishable from a local call.

### Phase 3 — Handle bridge (the hard part)
- [ ] Design guest-handle ↔ host-object mapping table (this replaces what the kernel normally does for free).
- [ ] Implement refcounting across the bridge: guest acquire/release must keep the host object alive; host must be able to signal death back to guest (`DeathRecipient` equivalent).
- [ ] Handle the case of a Binder object *returned* from a host call back into the guest (not just handles passed in) — needs its own proxy allocation on the guest side.
- [ ] Stress test: concurrent calls, object passed through 2+ hops, death notification while a call is in flight.
- **Exit criteria:** object lifetime and reference counting survive under concurrent load without leaks or use-after-free across the boundary.

### Phase 4 — Routing policy
- [ ] Write `routing/policy.rs`: static table of `(interface, method) → local | bridged`.
- [ ] Default-deny: everything stays local (guest) unless explicitly listed.
- [ ] Guest shim consults this table before deciding whether to intercept.
- **Exit criteria:** flipping a service between "local" and "bridged" is a one-line config change, no guest app changes needed.

### Phase 5 — First real offloaded service
- [ ] Pick target (see §6) — default recommendation: SurfaceFlinger's buffer-composition path.
- [ ] Implement the host-side Rust service that answers the relevant transaction codes.
- [ ] Wire host service output into the WebGPU compositor / canvas.
- [ ] End-to-end test: guest app renders a frame → composited buffer crosses the bridge → visible via WebGPU in the browser tab.
- **Exit criteria:** one real, visible, end-to-end frame rendered through the whole stack.

---

## 4. Milestone order (condensed)

1. Stock Android boots in v86 (Phase 0)
2. `binder-rt` roundtrips real Parcels off-VM (Phase 1)
3. Synthetic ping service works across virtio-binder (Phase 2)
4. Handle lifetime survives concurrency/death-notification stress test (Phase 3)
5. Routing table flips a real service on/off without guest changes (Phase 4)
6. First visible composited frame via host WebGPU (Phase 5)

Do not start Phase 5 before Phase 3 passes its stress test — this is where silent corruption bugs hide, and they're far cheaper to find before a real service depends on them.

---

## 5. Open decision — which service goes first?

Candidates and what each buys you:

| Service | Payoff | Difficulty |
|---|---|---|
| **SurfaceFlinger buffer path** | Direct tie-in to WebGPU compositor, visible result fast, matches project name | High — touches BufferQueue/gralloc semantics |
| **MediaCodec** | Offload decode/encode to WebCodecs, real perf win | Medium — narrower interface surface than SurfaceFlinger |
| **InputFlinger** | Simplest possible first bridge (small, well-defined interface) | Low — good for validating Phases 2–4 cheaply before committing to SurfaceFlinger |

**Recommendation:** use InputFlinger (or an even simpler synthetic service) as the Phase 2/3 validation target, then move to SurfaceFlinger for Phase 5. This avoids debugging BufferQueue semantics and the bridge transport at the same time.

---

## 6. Risks

- **Handle lifecycle across VM boundary** (Phase 3) is the single highest-risk item — no prior art to lean on, get it stress-tested before building services on top of it.
- **BufferQueue / gralloc semantics** for SurfaceFlinger involve shared-memory buffer handles, not just Binder calls — likely needs its own design doc before Phase 5 starts.
- **v86 CPU emulation overhead** — confirm guest performance is acceptable before investing in the bridge; if base emulation is too slow, the bridge's benefits are moot.
- **AIDL surface creep** — keep the AIDL codegen scope pinned to only the interfaces being offloaded; do not attempt full AOSP interface coverage.
- **`aidl-compat` shim risk** — chasing 100% compatibility with AOSP's official `binder` Rust crate surface can turn into its own project if the interfaces you offload use advanced features (nested parcelables, unions, `@nullable`, oneway edge cases). Timebox this; fall back to per-interface hand-adaptation of the generated code rather than a fully general shim if it stalls.
- **Toolchain dependency** — building/obtaining the official `aidl` binary means pulling it from AOSP source or prebuilt platform tools; pin an exact version so generated output stays stable across the project.