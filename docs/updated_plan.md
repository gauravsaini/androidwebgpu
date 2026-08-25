# AndroidWebGPU — Binder Subsystem Plan

## 0. Premise (locked in)

Binder work splits into two independent tracks that must not be conflated — plus one thing that was always assumed but never stated: **ART stays real too.**

| | Guest kernel binder driver | ART (Java runtime) | Host-side virtio-binder (§1–5, done) | Guest-side native system services (§7, new) |
|---|---|---|---|---|
| Where | Inside the v86 VM | Inside the v86 VM | Between guest and browser/WASM host | Inside the v86 VM, userspace |
| What | Real Linux kernel binder driver — **stays untouched** | Real AOSP ART (JIT/AOT, GC, `libcore`, JNI) — **stays untouched** | Paravirtualized transport + host Rust runtime | Java `system_server` (AMS/WMS/PMS/InputFlinger) **replaced** by native Rust binaries talking real kernel binder directly |
| Effort | Zero rewrite | Zero rewrite (build-step only, see §0.1) | Done (see §1–5) | 100% new engineering, guest-side |
| rsbinder role | N/A | N/A | Parcel/wire-struct layer inside `binder-rt` | Same `binder-rt` Parcel/wire code, reused, plus a **new** direct-ioctl transport (`binder-sys`) since these run in-guest, not over virtio |

Three things are true at once and must not be conflated:
- The **kernel binder driver** stays real and untouched — nothing below builds a new driver.
- **ART** stays real and untouched — reimplementing a JVM/ART-compatible runtime (compiler, GC, class library, JNI) is out of scope by orders of magnitude; nothing in this plan ever attempts it. See §0.1 for why this still needs a checklist entry despite requiring zero new code.
- **`system_server`** (the Java process hosting AMS/WMS/PMS/InputManagerService) is what's being replaced — by design, everything that talks to it over Binder (Zygote-forked app processes, framework.jar, other native daemons) must see wire-compatible replies, or nothing launches.

### 0.1 — ART checklist (no new code, but not zero work)

"Stays untouched" still means three concrete things need doing/verifying, folded into Phase 0:

- [ ] **Boot image must target the right ISA.** v86 emulates x86, so the AOT-compiled boot classpath (`boot.art`/`boot.oat`) has to be built for x86, not arm64 — this is a build-config step in your Android-x86 build, not code, but it's easy to silently get wrong (e.g. picking up a prebuilt arm64 boot image) and get a guest that boots but can't run any app.
- [ ] **Audit for JNI shortcuts around Binder.** Some framework Java classes call native code that bypasses Binder entirely for performance (the clearest example: parts of `SurfaceControl` talk to SurfaceFlinger via shared memory / native calls, not a clean `IWindowManager` transaction). Anywhere §7's Rust services replace a Java counterpart, check whether real framework classes reach it purely over Binder or partly via a JNI shortcut — if the latter, the Rust replacement needs to satisfy that path too, not just the AIDL interface. Do this audit *before* trusting a "boots one test APK" milestone as representative of anything broader.
- [ ] **Confirm Zygote's preload set matches your framework.jar.** Zygote preloads a fixed set of classes/resources before forking; if your guest image's `framework.jar`/boot classpath doesn't match what Zygote's preload list expects (version skew from mixing AOSP source trees), you'll see per-app-process crashes that look unrelated to Binder at all — worth ruling out early since it'll otherwise get mis-diagnosed as a §7 service bug.

### 0.2 — Verification gate (added after a status-drift incident)

**Rule: no phase gets marked complete based on a standalone Rust unit-test harness.** "Complete" means verified against the *real* guest — real v86, real Android-x86 kernel, real AOSP daemons (`system_server` if still Java at that point, `cameraserver`/`audioserver`/`sensorservice`, real ART) — not an in-memory Rust crate simulating the protocol.

This was written because it already happened once: `pms_rs`/`ams_rs`/`wms_rs`/`inputflinger_rs` and the virtual HALs were built and marked complete as isolated Rust harness crates — AIDL structures and buffer pools simulated in-process — without ever running against a booted v86 guest or registering with a real ServiceManager. Corrected status:

| Layer | Component | Honest status |
|---|---|---|
| L4 | Stock APKs | 🔴 Not executable — manifest/ZIP parsing only, no bytecode or native code runs |
| L3 | ART / Framework | 🔴 Not present — Phase 0 (real v86 boot) not yet verified |
| L2 | `pms_rs`/`ams_rs`/`wms_rs`/`inputflinger_rs` | 🟡 Rust harness only — not running against real kernel binder or real ServiceManager |
| L2 | Virtual HALs (Audio/Camera/Sensors) | 🟡 Rust mock only — stable-AIDL not pulled, VINTF not declared, real daemons never contacted |
| L1 | `binder_sys` / GPU transport | 🟡 Prototype — buffer-only bridging was the plan; a GL-command-stream translator (`gles2wgpu`) is a heavier, undiscussed deviation — confirm which one actually exists before counting it |

**Immediate corrective action, before any further Layer 2/3 work:** stop extending harness crates. Go back to Phase 0 and actually get it to its stated exit criteria — a stock Android-x86 image booting to launcher inside real v86, zero host involvement — before `pms_rs` etc. are tested against anything other than that.

**AIDL note:** no custom AIDL parser (not rsbinder-aidl, not hand-written). AIDL frontend is AOSP's official `aidl` compiler binary, shelled out to. This applies to §7 as much as §1 — the Rust AMS/WMS/PMS/InputFlinger must implement the exact same `IActivityManager`/`IWindowManager`/`IPackageManager`/`IInputManager` AIDL surface (matching your target API level) that stock app processes already expect, since those apps are not being modified.

---

## 1. Goals / non-goals

**Goals**
- §1–5 (done): SurfaceFlinger's buffer-composition path offloaded to a host Rust runtime over virtio-binder, indistinguishable from a local call.
- §7 (new): AMS, WMS, PMS, InputFlinger reimplemented as native Rust binaries running **inside the guest**, registering with the real (unmodified) ServiceManager, talking the real kernel binder driver directly.
- An unmodified stock APK (unmodified Zygote-forked app process, unmodified framework.jar) can launch, draw a window, receive input, and load resources — served entirely by the Rust services, with the app never knowing.

**Non-goals (explicitly out of scope)**
- Reimplementing the Binder kernel driver — stays real, in both tracks.
- Reimplementing ServiceManager for the guest (guest keeps its real one) — Rust AMS/WMS/PMS/InputFlinger register with it like any other service.
- Routing all Binder traffic through virtio — that track is scoped to SurfaceFlinger only.
- Full AOSP feature parity for AMS/WMS/PMS on the first pass — see §7 MVP scoping. This is the single biggest scope risk in the whole project; treat "boots one test APK" as the actual milestone, not "reimplements Android."

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
│       └── surfaceflinger-gpu/ # first offloaded service (see §6) — DONE
├── routing/
│   └── policy.rs                # which interface/method pairs cross the bridge (virtio track only)
└── guest/system-services/       # NEW — native Rust replacements for system_server (§7)
    ├── binder-sys/               # raw ioctl bindings to the real /dev/binder (direct, no virtio)
    ├── zygote-client/            # speaks Zygote's abstract-socket fork protocol
    ├── input-channel/            # replicates InputChannel shared-mem/socketpair protocol
    ├── pms-rs/                   # PackageManagerService — manifest/APK parsing, resolution
    ├── ams-rs/                   # ActivityManagerService — lifecycle, process mgmt
    ├── wms-rs/                   # WindowManagerService — window hierarchy, focus
    └── inputflinger-rs/          # InputFlinger — evdev read + dispatch

guest/virtual-hal/                # NEW (§8) — fake HAL modules; real cameraserver/audioserver/
├── camera-hal-virtual/           #   sensorservice stay UNMODIFIED, talk to these as if real hardware
├── audio-hal-virtual/            # AIDL HAL services — built with binder-sys (Phase 6) + aidl-compat/binder-rt,
└── sensors-hal-virtual/          # same tooling as everything else, no separate hwbinder transport needed

host-runtime/services/            # (extends existing host-runtime/services/ from §1-5)
├── surfaceflinger-gpu/           # DONE
├── camera-host-rs/               # getUserMedia bridge, feeds camera-hal-virtual
├── audio-host-rs/                # WebAudio bridge, feeds audio-hal-virtual
├── sensor-host-rs/                # browser Sensor API bridge, feeds sensors-hal-virtual
└── media-host-rs/                # WebCodecs — bridges IMediaCodecService directly (framework-level, not HAL)
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

---

## 7. Guest-side native system services (NEW — replaces §5/§6 scope for AMS/WMS/PMS/InputFlinger)

This track is **separate from virtio-binder**. These services run inside the guest, talk the real kernel binder driver directly, and register with the real ServiceManager under the standard names (`"activity"`, `"window"`, `"package"`, `"input"`). Stock Zygote-forked app processes must not need any modification — they resolve these services exactly as they would on real Android.

### 7.0 — MVP scoping (read before writing code)

AMS/PMS/WMS/InputFlinger together are one of the largest subsystems in AOSP. Full parity is not a real target for a from-scratch Rust port. Lock in an explicit minimal target before starting:

- **One fixed test APK** (simple single-Activity app), not general app support.
- **PMS:** resolve/query that one APK only — no install flow, no multi-APK resolution, permissions = grant-all.
- **AMS:** single foreground Activity, no back stack, no services/broadcasts beyond what's required to boot the app.
- **WMS:** single fullscreen window, no multi-window, no animations, no focus-routing logic (only one window exists).
- **InputFlinger:** all input goes to the one window — no dispatch/focus logic needed yet.

**MVP Definition of Done:** the unmodified test APK launches, draws through the existing WebGPU-backed SurfaceFlinger, receives touch/key input, and exits cleanly — served entirely by these four Rust services.

Do not add scope (multi-app, back stack, permissions model, animations) until this MVP is solid — every one of those is its own multi-week effort on real AOSP.

### Phase 6 — `binder-sys`: direct kernel transport (the guest-native counterpart to §1's virtio path)
- [ ] Raw ioctl bindings (`bindgen`) to `/dev/binder`: `BINDER_WRITE_READ`, `mmap` region, `BINDER_SET_MAX_THREADS`, `BINDER_SET_CONTEXT_MGR` (not used — guest keeps real servicemanager), thread-exit handling.
- [ ] Reuse `binder-rt`'s Parcel/wire code from §1 against this transport instead of the virtio TLV path — same Parcel format, different transport underneath.
- [ ] Threadpool: implement the real binder looper spawn-before-block model (a service thread must spawn its replacement *before* blocking on a call that might re-enter, or you deadlock under real concurrent app load).
- **Exit criteria:** a trivial Rust service calls `addService("test", ...)` against the real servicemanager and answers a call from a stock Java test client — no virtio involved.

### Phase 7 — PMS (minimal)
- [ ] Binary AndroidManifest.xml (AXML) parser + `resources.arsc` parser, scoped to the one test APK.
- [ ] Implement enough of `IPackageManager` to answer `resolveActivity` / `getPackageInfo` / `getApplicationInfo` for that APK.
- [ ] Register as the `"package"` service.
- **Exit criteria:** a `dumpsys package`-equivalent query against the test APK returns correct info from the Rust PMS.

### Phase 8 — AMS (minimal)
- [ ] `zygote-client`: replicate the abstract-socket protocol Zygote expects (the argument wire format used by `Process.start()`) to fork the test app's process — this protocol is undocumented outside AOSP source and version-sensitive; pin your target AOSP version.
- [ ] Minimal lifecycle: `startActivity` → resolve via PMS → fork via Zygote → `bindApplication` → `onCreate`/`onResume` round-trip.
- [ ] No back stack, no broadcasts/services beyond boot requirements.
- **Exit criteria:** test APK's process is forked and its Activity reaches `onResume`, confirmed via logcat from the unmodified app.

### Phase 9 — WMS (minimal)
- [ ] Single-window model: allocate one `Surface`, hand it to the already-working SurfaceFlinger/WebGPU path from §5.
- [ ] No focus-routing logic needed — one window gets everything.
- **Exit criteria:** the test app's window is visible end-to-end through the WebGPU compositor.

### Phase 10 — InputFlinger (minimal)
- [ ] `input-channel`: replicate the real `InputChannel` protocol (socketpair + shared memory — **not** standard Binder) between InputFlinger and the app's `ViewRootImpl`.
- [ ] Read input events (evdev, or v86's virtual input device) and dispatch unconditionally to the single window from Phase 9.
- **Exit criteria:** touch/key events flow from host input → InputFlinger → the test app's `onTouchEvent`/`onKeyDown`.

**Dependency order:** Phase 6 blocks everything. 7 → 8 are sequential (AMS needs PMS to resolve the app). 9 and 10 can be built in parallel once 8 exists, but both need a running app process to test against meaningfully.

### 7.x — Additional risks specific to this track

- **Unbounded scope** is the dominant risk. AMS/PMS/WMS/InputFlinger collectively are among the largest subsystems in AOSP; without the §7.0 MVP discipline this track has no natural stopping point. Re-read §7.0 before adding any feature.
- **Wire-format exactness is not optional here.** Unlike §1–5 (where you control both ends), these services face *unmodified* stock app processes. Any transaction-code or Parcel-layout mismatch against the AIDL version your target Android API level expects doesn't degrade gracefully — the app crashes or ANRs.
- **Zygote protocol fragility** — the fork-request wire format is internal AOSP plumbing, not a stable public API. Pin an exact AOSP/API-level version and expect to re-verify it if you change Android versions later.
- **`InputChannel` is not Binder** — it's a separate socketpair/shared-ring-buffer protocol layered on top of a Binder handshake for setup. Budget for it as its own mini-protocol, not "more AIDL."
- **Threadpool correctness under real concurrency** (Phase 6) — with real apps hitting these services, the spawn-before-block looper rule is a correctness requirement, not an optimization; get it right before Phase 7 depends on it.

---

## 8. HAL bridge — Camera / Audio / Sensors / Media (NEW)

### 8.0 — Architecture decision: HAL-level, not framework-level (for Camera/Audio/Sensors)

`cameraserver`, `audioserver`, `sensorservice` are native C++ daemons **outside** `system_server` — §7 never touched them, they're still real, unmodified AOSP binaries. That means the correct place to bridge is **under** them (the HAL), not by replacing `ICameraService`/`IAudioFlinger`/`ISensorService` themselves:

| Approach | What you reimplement | Risk |
|---|---|---|
| Framework-level (original draft) | All of CameraService/AudioFlinger/SensorService's internal logic — client arbitration, session state machines, permission plumbing | High — reinventing logic that already works correctly in the real daemon |
| **HAL-level (recommended for Camera/Audio/Sensors)** | Only the hardware-facing HAL interface — daemon logic stays real | Lower — daemon's existing behavior is preserved for free |

**Exception: Media/Codec2.** The Codec2 HAL (buffer pools, `IGraphicBufferProducer` ties) is disproportionately complex to virtualize for the payoff. Keep Media at **framework level** — bridge `IMediaCodecService` directly, as originally drafted.

**Transport decision — RESOLVED (target: Android 13+):** AIDL HALs use the same kernel binder driver and the same real ServiceManager as regular framework Binder — no separate `hwbinder`/`hwservicemanager` domain. This means **no new `hwbinder-sys` crate and no new `virtio-hwbinder` device** — `binder-sys` (Phase 6) and the existing `virtio-binder` transport (§1–5) are reused directly for the virtual HALs below. Two new requirements this introduces instead:
- **Exact stable-AIDL match:** pull the frozen `.aidl` HAL interface definitions for your target API level from AOSP (`hardware/interfaces/camera/provider/aidl/`, `hardware/interfaces/audio/aidl/`, `hardware/interfaces/sensors/aidl/`) — same "exact version or the real daemon's client stub breaks" rule as §7's framework AIDL.
- **VINTF manifest declaration:** ServiceManager refuses AIDL HAL registration unless it's declared in the device/framework compatibility matrix (`isDeclared()` check, added specifically to prevent interface spoofing). Your virtual HAL instances must be added to the guest image's `device_manifest.xml` (or v86-image equivalent), or the real daemon's `waitForVintfService()` call fails before your HAL is ever reached.
- **Don't assume uniform migration:** verify Camera/Audio/Sensors are actually AIDL (not still HIDL) at your specific target API level — rollout wasn't simultaneous across all HAL categories. Confirm per-component against the AOSP source tree for your pinned version before starting Phase 11.

### 8.1 — Cross-cutting concerns (apply to all four)

- **Permission bridging:** browser permission prompts (camera/mic/sensor via `getUserMedia`/Generic Sensor API) are a *separate* grant from Android's own runtime permission model. §7's PMS MVP is grant-all on the Android side — but the browser will still prompt the user once, out-of-band from the guest. Design an explicit host-side "ensure browser permission" step before the HAL call succeeds, and decide what the guest sees while that prompt is pending (block the HAL call, or return a transient error the daemon already knows how to handle).
- **Streaming buffer cost:** camera preview frames and decoded video frames are large per-frame payloads. Routing every frame through a generic Parcel/TLV copy (like a normal Binder call) will not perform. Reuse the same shared-buffer mechanism already required for SurfaceFlinger's BufferQueue (see §6 risks) — don't build a one-off copy path per HAL.
- **Lifecycle/visibility events:** tab backgrounding, browser-side permission revocation, or a real device losing the camera/mic mid-stream must map to a Binder-side signal the guest daemon already knows how to handle (disconnect callback, audio focus loss) — otherwise the guest app hangs waiting on a stream that silently died on the host.
- **Browser API availability:** the Generic Sensor API is not implemented in all browsers (notably absent in Firefox/Safari at time of writing) — verify current support before committing sensors-hal-virtual to it, and have a fallback (synthetic/zero data) rather than a hard failure.

### Phase 11 — Sensors HAL (first, simplest)
- [ ] Pull the exact `ISensors` stable-AIDL definition for your pinned Android version from `hardware/interfaces/sensors/aidl/`.
- [ ] `sensors-hal-virtual`: implement `ISensors` via `binder-sys` + `aidl-compat`, presenting a fixed sensor list (accelerometer, gyroscope) to real `sensorservice`.
- [ ] Declare the instance (`android.hardware.sensors.ISensors/default`) in the guest image's VINTF manifest so ServiceManager's `isDeclared()` check passes.
- [ ] `sensor-host-rs`: subscribe to browser Generic Sensor API, forward samples over virtio-binder at the rate `sensorservice` requested.
- [ ] Test APK: reads accelerometer, displays live values.
- **Exit criteria:** unmodified test APK shows sensor values that change with real (or simulated) host device motion, with zero changes to `sensorservice` itself.

### Phase 12 — Audio HAL (playback first, recording second)
- [ ] Pull the exact audio core AIDL HAL (`IModule` and related, `hardware/interfaces/audio/aidl/`) for your pinned version.
- [ ] `audio-hal-virtual`: implement the audio HAL device interface presenting one virtual output (and later input) device to real `audioserver`. Declare the instance in the VINTF manifest.
- [ ] `audio-host-rs`: map HAL PCM buffer writes to a WebAudio graph (`AudioBufferSourceNode`/`AudioWorklet`); mic path maps WebAudio input node back into HAL buffer reads.
- [ ] Test APK: plays a tone (playback), then records mic input (capture) — split into two milestones, don't do both at once.
- **Exit criteria:** unmodified test APK produces audible output via WebAudio; separately, mic capture round-trips correctly.

### Phase 13 — Camera HAL
- [ ] Pull the exact `ICameraProvider`/`ICameraDevice` stable-AIDL definitions (`hardware/interfaces/camera/provider/aidl/`) for your pinned version.
- [ ] `camera-hal-virtual`: implement the provider/device interfaces presenting one fixed-capability virtual camera to real `cameraserver`. Declare the instance in the VINTF manifest.
- [ ] `camera-host-rs`: bridge `getUserMedia` video frames into the HAL's buffer-delivery path — reuse the §6 shared-buffer mechanism, do not copy raw frame bytes through Parcels.
- [ ] Scope the capability set deliberately: preview-only, fixed resolution, no manual capture controls (exposure/focus/RAW) for MVP — Camera2's full capture-request/result metadata model is large.
- [ ] Test APK: `CameraPreviewActivity` shows a live feed.
- **Exit criteria:** unmodified test APK's camera preview shows live host webcam video via `getUserMedia`, through the real `cameraserver` unmodified.

### Phase 14 — Media (framework-level, not HAL)
- [ ] `media-host-rs`: implement enough of `IMediaCodecService`'s AIDL surface to answer decode requests, backed by WebCodecs.
- [ ] Codec capability negotiation: WebCodecs' supported codec/profile set varies by browser and is narrower than Android's. Define an explicit supported-codec list for the test file and fail cleanly (not silently) outside it.
- [ ] AV sync: WebAudio (Phase 12) and WebCodecs run on independent browser clocks. If the test APK plays video with audio, establish a shared timestamp reference bridged back to the guest — accept skew as a known MVP limitation if this proves too costly to fix now.
- [ ] Test APK: plays one H.264 test clip.
- **Exit criteria:** unmodified test APK decodes and renders one video file via WebCodecs, visible through the already-working SurfaceFlinger/WebGPU path.

**Order of attack:** Sensors → Audio (playback) → Camera (preview) → Media (decode). Matches the original draft's ordering — validate the HAL-bridge pattern cheaply on Sensors before committing to Camera's harder buffer-sharing problem.

### 8.x — Additional risks specific to this track

- **VINTF declaration is a hard gate, not paperwork** — miss it and the real daemon's `waitForVintfService()` simply never sees your virtual HAL; this fails silently from your side (looks like the daemon isn't calling you) unless you know to check the manifest first.
- **Per-HAL AIDL migration isn't uniform** — confirm Camera/Audio/Sensors are actually on AIDL (not HIDL) at your specific pinned API level before Phase 11; don't assume "Android 13+" implies all three migrated simultaneously.
- **Stable-AIDL exactness** — like §7's framework AIDL, these HAL interfaces are frozen/versioned; a mismatch against what your pinned daemon binary expects fails hard, not gracefully.
- **Codec2 avoidance is a deliberate scope cut**, not an oversight — revisit only if `IMediaCodecService`-level bridging proves insufficient for real app compatibility later.
- **Permission-prompt UX mismatch** between browser and Android models is easy to underestimate — a guest app that already "has" the Android permission but stalls on an unresolved browser prompt looks like a hang, not a permission error, unless explicitly handled.