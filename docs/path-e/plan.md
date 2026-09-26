# Android OS in Browser: Completion Plan

Status: proposed execution plan
Owner: platform team
Scope: full Android OS boot and use inside a browser
Depth: tree 1
Mode: solo artifact pass; implementation lanes are parallel after contract freeze

## Execution contract

This plan is the contract for completing the product. It is not a prototype
roadmap.
The only accepted product path is a real x86 Android guest running in the browser
through a VM, real guest I/O, and a WebGPU-backed display path.

- The browser entry point may load only the production runtime module
  `pkg/android_vm.js`; legacy synthetic arcade, pixel, and packet fixtures cannot
  satisfy any OS gate.
- Runtime statuses are exactly `PENDING`, `RUNNING`, `PASSED`, `FAILED`, and
  `BLOCKED`. Missing prerequisites are `BLOCKED`; they are never inferred as
  success.
- `ready === true` is legal only when every required gate `G0` through `G9` is
  `PASSED` in the same run and epoch.
- Every LLD entity below has a named owner, input contract, output contract,
  failure contract, and validation proof. Any missing contract blocks that lane.
- A worker, device, image, or protocol can be stubbed only in an isolated unit
  test. A stub must never be reachable from the browser acceptance path.

## Depth tree and parallel ownership

```text
1 Full Android OS in browser
├── 1.1 browser/runtime contract      W0 + W1 + W9
├── 1.2 VM/virtio and browser I/O     W2 + W3 + W4 + W5
├── 1.3 real guest GPU/display        W6 + W7
├── 1.4 pinned Android guest          W8
└── 1.5 integration and acceptance    W9
```

The current turn owns the root plan, gate ledger, and fail-closed browser entry.
Future workers must use disjoint `OWNS:` paths from section 6. A worker may
consume another lane's frozen contract but may not redefine it.

## 1. Goal and success bar

### HLD goal

Run a bootable Android-x86 guest inside a browser tab. The guest must boot through
the normal Android init path, render SystemUI and apps, accept keyboard/mouse/touch
input, persist its disk, and use browser-backed network, audio, and graphics
devices. WebGPU is the host acceleration path; it is not the guest API.

The target product is therefore:

```text
Browser shell
  -> WebAssembly x86 VM
  -> Android-x86 kernel + initramfs + system/vendor/product images
  -> Linux virtio devices
  -> Android framework / SurfaceFlinger / EGL / GLES
  -> virtio-gpu protocol
  -> Rust host bridge
  -> WebGPU canvas + browser I/O
```

The current arcade page, Rust GPU crates, APK fixtures, and visual gates are
legacy or isolated test assets. They are not evidence of Android OS boot and
must not be imported by the browser acceptance path.

### Definition of done

Done means all of the following are true in a clean checkout:

1. `make web` or an equivalent one-command build creates the complete browser
   bundle, including the WASM package and all boot assets.
2. A fresh browser tab loads the bundle without a 404, uncaught import error, or
   false ready status.
3. A pinned Android guest image boots to SystemUI without a host-side mock.
4. Guest input reaches Android and Android display output reaches the canvas.
5. A reboot preserves a test file in the guest disk.
6. The guest can obtain a network lease or use the declared browser net mode.
7. A GLES app can create an EGL surface, draw, swap, and remain responsive.
8. Browser validation reports each gate as `PENDING`, `RUNNING`, `PASSED`,
   `FAILED`, or `BLOCKED`; no gate can be green from a preset HTML value.
9. CI runs host unit tests, WASM build, browser smoke, VM boot, I/O, and GPU
   contract tests.

## 2. Current code audit

### What exists

- `crates/gles2wgpu`: a partial GLES state machine and Naga shader path.
- `crates/virtio_gpu_bridge`: host-side packet parsing, small 2D backing stores,
  and a custom 3D command stream.
- `crates/webgpu_compositor`: a reusable WebGPU layer compositor.
- `crates/webgpu_swapchain`: offscreen textures and an unconnected surface type.
- `crates/apk_gpu_analyzer`: APK ZIP/AXML inspection only.
- `crates/metrics_overlay`: metrics data and a compositor layer.
- `src/virtio_gpu_device.js`: a direct-call test object, not a v86 device.
- `src/test_suite.js`: synthetic pixel tests; never an OS acceptance source.
- `guest/patches`: incomplete source sketches for old Android HAL seams.
- `fixtures/*.apk`: APK inputs for analyzer tests, not boot media.

### Hard blockers found

1. The generated runtime package `pkg/android_vm.js` is absent in a clean
   checkout. The browser entry now loads it dynamically and reports a visible
   `BLOCKED` state when the HTTP request returns 404; this remains a build gap,
   not a successful runtime.
2. No v86 source, package, VM start path, x86 BIOS, Android kernel, initrd, or
   system/vendor/product image is present.
3. `VirtioGpuDevice` stores fake PCI bytes and accepts a direct `Uint8Array` at
   `processControlQueue`; it has no virtqueue descriptors, DMA, interrupts,
   feature negotiation, BAR behavior, or v86 registration.
4. No virtio block, console, input, network, entropy, RTC, audio, or camera
   device exists. A full Android guest cannot boot or be used with only GPU.
5. The page uses `CanvasRenderingContext2D.putImageData`, not a WebGPU canvas
   surface. `WebGpuSwapchain` is not constructed by the browser entry point.
6. `RESOURCE_ATTACH_BACKING` is decoded but not executed by the bridge. Context
   commands return success without context state. Real guest virtio-gpu traffic
   cannot use this path.
7. `SUBMIT_3D` accepts a custom test opcode stream, not a declared Virgl or
   gfxstream protocol. The draw-elements decoder also reads the index type from
   the count field in `bridge.rs:396`.
8. Gate 4 creates synthetic gradients and Gate 5 creates synthetic system bars;
   neither loads nor runs an APK. These paths cannot certify any OS gate and
   must remain outside the browser acceptance path.
9. The guest C++ files do not form a buildable Android port. `eglMakeCurrent`
   is a no-op, no GLES entry point layer is present, HWC `set()` does not send
   layer data, and the `eglSwapBuffers` packet buffer is too short for its two
   declared wire packets.
10. Several host GPU tests return early when no adapter exists. A passing test
    can therefore mean "GPU unavailable" rather than "GPU verified".

### Current proof baseline

The host workspace currently passes its Rust tests and JS syntax checks. That
only proves host code paths that are reached by those tests. The WASM target is
not installed in the present environment, and the browser bundle is not
complete. This plan treats those as build failures, not optional setup.

## 3. HLD architecture

### 3.1 Runtime layers

```text
UI + validation loop
        |
BrowserHost: canvas, input, storage, net, audio, clock
        |
VmRuntime: v86 CPU/RAM/BIOS + lifecycle
        |
VirtioBus: PCI config + queue + IRQ + DMA
        |
Guest devices: blk, console, input, net, rng, rtc, gpu, audio
        |
Android guest: kernel -> init -> zygote -> system_server -> SystemUI/apps
        |
GPU host path: virtio-gpu -> memory map -> GLES/Gfx -> compositor -> surface
```

### 3.2 Ownership rules

- Guest RAM belongs to the VM and is accessed only through checked DMA views.
- Browser-owned buffers never expose raw host pointers to guest code.
- Every async browser op has an ID, completion, timeout, and error result.
- GPU resources are owned by a resource registry and released on guest teardown.
- Disk writes are ordered through one storage actor; concurrent writes are not
  allowed to race IndexedDB or OPFS transactions.
- A frame is presented only after its fence or explicit completion is observed.
- The validation loop observes runtime events; it must not invent a pass result.

### 3.3 State model

```text
NEW -> ASSET_CHK -> VM_RDY -> BOOTING -> KERN_RDY -> AND_RDY -> RUN
                      |          |          |           |
                      +-------->FAIL <------+-----------+

RUN -> PAUSE -> RUN
RUN -> STOPPED
```

Required state events: `asset_check`, `vm_create`, `boot_start`, `kernel_seen`,
`android_ready`, `frame_presented`, `input_delivered`, `disk_commit`,
`net_ready`, `fatal_error`.

## 4. LLD entities and strict contracts

Every entity below must ship with the listed tests. A contract is strict: a
caller may rely on it, and an implementation must reject bad input rather than
silently accepting it.

### E01. Web build and bundle

**Role:** create a reproducible browser artifact.

**Inputs:** Rust workspace, JS sources, v86 dependency, guest assets, build
configuration, target browser feature set.

**Outputs:** `dist/index.html`, hashed JS, `pkg/*.js`, `pkg/*.wasm`, boot image
manifest, source map, and a machine-readable `build-manifest.json`.

**Contract:**

- The bundle must be self-contained except for explicitly declared remote fonts.
- Every import in `index.html` must exist in `build-manifest.json`.
- WASM must build for `wasm32-unknown-unknown` with the `wasm` feature.
- Asset URLs must be relative to the bundle root and must work under HTTP(S).
- Build must fail on missing target, missing asset, or stale generated package.
- `pkg/` must be generated by the build and must not be a hidden manual step.

**Tests:** clean checkout build; manifest import scan; HTTP 200 for all assets;
WASM instantiate smoke; CSP-compatible module load.

### E02. BrowserHost

**Role:** own browser APIs and provide stable host services.

**Inputs:** canvas, DOM events, IndexedDB/OPFS, Fetch/WebSocket/WebRTC policy,
Web Audio, WebGPU.

**Outputs:** typed host service traits for VM, devices, and renderer.

**Contract:**

- `open_canvas()` returns a configured `GPUCanvasContext` or a typed error.
- `read_input()` returns ordered events with monotonic sequence numbers.
- `read_disk()`/`write_disk()` are async and CRC-checked.
- `open_net()` exposes only the declared proxy mode; no raw socket assumption.
- `audio_write()` is non-blocking and reports underrun/overrun.
- `now_ns()` is monotonic within one VM session.
- Browser feature absence is surfaced as `UNSUPPORTED`, never as a green state.

**Tests:** no-WebGPU, storage quota, reload, tab visibility, input order,
network denial, audio unlock, canvas resize, and worker/main-thread policy.

### E03. GuestImageStore

**Role:** fetch, verify, cache, and stream Android boot media.

**Inputs:** signed manifest, image URLs or embedded assets, cache key, optional
user-provided image.

**Outputs:** read-only boot streams plus a writable userdata block.

**Contract:**

- Manifest pins SHA-256, byte size, image role, Android version, ABI, and page
  alignment.
- No image is mounted before hash and size checks pass.
- `system`, `vendor`, `product`, `odm`, `ramdisk`, and `kernel` roles are distinct.
- Userdata is copy-on-write or OPFS-backed and survives VM restart.
- Partial download can resume; corrupt chunks are discarded.
- A missing required role blocks boot with the exact role in the error.

**Tests:** hash pass/fail; resume; corrupt chunk; quota full; cold boot from
cache; userdata persistence across reload and VM reboot.

### E04. VmRuntime

**Role:** run the x86 guest and expose lifecycle plus memory access.

**Inputs:** BIOS/firmware, kernel, initrd, disk devices, RAM size, CPU count,
device list, boot args.

**Outputs:** VM state events, guest RAM access, IRQ injection, halt reason,
serial log, reset/pause/resume controls.

**Contract:**

- `start()` is idempotent only before `STOPPED`; a second start is rejected.
- `pause()` quiesces device callbacks before resolving.
- Device IRQs are delivered on the VM event loop, not from arbitrary DOM tasks.
- Guest physical reads/writes are bounds-checked and alignment-aware.
- Halt, triple fault, invalid opcode, and watchdog timeout are distinct errors.
- The VM can be reset without leaking workers, timers, GPU resources, or locks.

**Tests:** BIOS boot; kernel boot; RAM bounds; IRQ order; pause/resume; reset;
watchdog; worker teardown; serial log capture.

### E05. VirtioBus and VirtioQueue

**Role:** implement the transport shared by guest devices.

**Inputs:** PCI config reads/writes, guest RAM, queue setup, feature bits, queue
notify, interrupt status.

**Outputs:** descriptor chains to device handlers and used-ring completions.

**Contract:**

- Support the guest's selected legacy or modern virtio mode; report only features
  actually implemented.
- Validate queue size, descriptor address, chain length, indirect tables, and
  writable/readonly flags.
- Reject loops, overlap, out-of-RAM ranges, and malformed chains.
- Every consumed descriptor chain gets exactly one used entry or a fatal device
  error; no chain is silently dropped.
- IRQ status is latched until acknowledged.
- Queue processing is bounded per tick to protect browser responsiveness.

**Tests:** feature negotiation; split and packed rings as needed; malformed
chains; indirect descriptors; IRQ ack; queue reset; stress with 1M requests.

### E06. VirtioBlk

**Role:** provide Android boot and userdata block I/O.

**Inputs:** read/write/flush requests, sector number, guest buffers.

**Outputs:** virtio status, transferred byte count, flush completion.

**Contract:**

- Sector size is fixed and advertised; unaligned requests are rejected.
- Requests beyond image size return `IOERR` and do not touch storage.
- Reads are immutable for boot roles.
- Writes are journaled, ordered, and flushed before `FLUSH` completes.
- A failed browser storage op completes the request with an error.
- No request may block the VM event loop.

**Tests:** boot read; boundary read/write; partial sector rejection; flush order;
power-loss simulation; quota failure; userdata persistence.

### E07. VirtioConsole and boot log

**Role:** provide early boot and debug output.

**Inputs:** guest TX/RX queue traffic.

**Outputs:** bounded serial log, optional browser terminal input.

**Contract:**

- TX is lossless up to a bounded buffer; overflow is counted and signalled.
- RX input is opt-in and ordered.
- Logs include VM epoch and device timestamp.
- Console teardown wakes all pending reads.

**Tests:** kernel boot log; RX/TX backpressure; teardown; 10 MB burst.

### E08. VirtioInput

**Role:** deliver keyboard, mouse, touch, and button events.

**Inputs:** DOM `KeyboardEvent`, pointer, touch, wheel, focus/blur.

**Outputs:** virtio input events with type/code/value and sequence.

**Contract:**

- DOM key mapping is explicit and versioned; unknown keys are reported, not
  guessed.
- Focus loss sends key-up for all pressed keys.
- Pointer coordinates are transformed from CSS pixels to guest display pixels.
- Touch IDs are stable for the life of a contact.
- Browser permission or focus loss cannot deadlock the guest input queue.

**Tests:** key rollover; IME path; focus loss; multi-touch; rotation/resize;
pointer capture; duplicate event rejection.

### E09. VirtioNet

**Role:** expose a safe browser network mode.

**Inputs:** guest Ethernet frames.

**Outputs:** guest RX frames, link state, DNS/proxy status.

**Contract:**

- Pick one mode: user-mode NAT via a service worker/proxy, WebSocket tunnel, or
  WebRTC data channel. The mode is declared in the build manifest.
- The device never promises raw L2 access if the browser cannot provide it.
- MTU, MAC, link state, and checksum behavior are fixed and tested.
- Backpressure is bounded; dropped frames are counted.
- CORS, CSP, and permission errors return link-down, not fake link-up.

**Tests:** DHCP or static setup; DNS; TCP loopback through the selected proxy;
MTU; link flap; offline mode; backpressure.

### E10. VirtioRng, RTC, power, and audio

**Role:** satisfy Android's basic platform dependencies.

**Inputs:** browser crypto, monotonic clock, visibility state, AudioWorklet.

**Outputs:** random bytes, wall/mono time, suspend/resume, PCM stream.

**Contract:**

- RNG uses `crypto.getRandomValues` or a declared fallback with a hard error.
- RTC distinguishes wall time from monotonic time.
- Visibility changes do not change guest time unexpectedly.
- Audio starts only after browser user activation and reports mute/blocked state.
- Device reset closes all browser handles.

**Tests:** entropy health; clock drift; tab hide/show; audio unlock; underrun;
reset and resource release.

### E11. VirtioGpuDevice / guest GPU transport

**Role:** expose a real virtio-gpu PCI device to Android.

**Inputs:** control/cursor queue chains, guest DMA pages, fences, context IDs,
resource IDs, scanout state.

**Outputs:** spec-shaped response chains, used-ring entries, IRQs, display info,
and completed fences.

**Contract:**

- Implement the exact virtio-gpu version and feature set declared to the guest.
- Implement `GET_DISPLAY_INFO`, resource create/unref, attach/detach backing,
  set scanout, transfer, flush, context lifecycle, capsets, fences, and cursor
  behavior required by the chosen Android stack.
- Resource IDs, scanout IDs, context IDs, and fence IDs are validated and never
  reused while referenced.
- Guest backing entries map to checked guest RAM, not packet-local payload only.
- Transfer offsets and row strides are honored.
- Unsupported 3D protocols return a spec error and a telemetry event.
- The device is registered with `VirtioBus`; direct JS calls are test-only.

**Tests:** wire packet golden tests; descriptor/DMA tests; resource lifetime;
subrect stride; attach backing; fence order; invalid IDs; reset; real Linux
virtio-gpu probe; real guest scanout.

### E12. GuestMem and resource registry

**Role:** bridge guest physical pages to host GPU resources.

**Inputs:** guest physical address ranges, resource metadata, transfer boxes.

**Outputs:** checked host views or copied staging buffers.

**Contract:**

- All ranges use checked `u64` arithmetic.
- Page lists may not alias a live resource unless the ownership rule says they
  may; aliasing is tested.
- Host writes never exceed the declared guest range.
- Resource format, width, height, stride, and byte size are immutable after create
  or changed only by an explicit destroy/recreate sequence.
- Unmap occurs before resource destruction.

**Tests:** overflow; page split; alias; detach; destroy while in flight;
read-only guest page; large frame.

### E13. GLES2WGPU

**Role:** translate the guest-supported GLES ABI to WebGPU commands.

**Inputs:** validated guest GLES command stream or a declared virgl/gfxstream
stream, GL state, shader source, resource handles.

**Outputs:** WebGPU render commands, errors, metrics, and fence completion.

**Contract:**

- The supported GLES version and extension list are explicit and returned to the
  guest. Unsupported calls produce the correct GL error.
- Shader compile/link status is real; no source is marked compiled without Naga
  validation and WebGPU module creation.
- GL object lifetimes and bind state follow the declared GLES version.
- Viewport, scissor, blend, depth, stencil, texture, framebuffer, VAO, and
  uniform state affect the issued render pass.
- Readback and presentation use the same resource, not a test-only texture.
- A GPU validation error tears down the affected context with a typed event.

**Tests:** API conformance subset; shader corpus; state leakage; textures;
depth/stencil; FBO; blend; indexed draws; context loss; real guest GLES app.

### E14. WebGpuCompositor

**Role:** compose Android display layers.

**Inputs:** layer ID, buffer resource, crop, transform, alpha, blend mode, z,
damage region, fence.

**Outputs:** one target texture per display frame and frame fence.

**Contract:**

- Layer order is deterministic by z then stable ID.
- Crop and transform use one documented coordinate convention.
- Premultiplied and straight alpha are not mixed silently.
- Damage rects cannot cause stale pixels outside the declared clear policy.
- A layer cannot sample a resource before its acquire fence completes.
- Removing a layer releases its bind resources after the frame fence.

**Tests:** crop; rotation; alpha; z order; damage; fence wait; layer destroy;
multi-display rejection/handling.

### E15. WebGpuSwapchain and canvas output

**Role:** present the composed frame to the browser.

**Inputs:** `HTMLCanvasElement`, `GPUAdapter`, `GPUDevice`, target size, frame
texture, resize and visibility events.

**Outputs:** configured `GPUCanvasContext`, present completion, surface errors.

**Contract:**

- The browser canvas is configured with a supported format and alpha mode.
- Canvas size uses CSS size times device pixel ratio, clamped to device limits.
- Zero-size or hidden tabs pause rendering and do not submit invalid textures.
- `OUTDATED`, `LOST`, and `TIMEOUT` are distinct errors with recovery policy.
- A frame is not counted as presented until the target texture is submitted.
- Headless readback is test-only; it is not the production display path.

**Tests:** WebGPU absent; device lost; resize; DPR; hidden tab; 60 Hz loop;
present error recovery; screenshot pixel probe.

### E16. Android guest integration

**Role:** make the selected Android build use the devices.

**Inputs:** Android source branch, kernel config, device tree/PCI config, HAL
modules, EGL/GLES path, system properties, init rc, guest image manifest.

**Outputs:** bootable Android-x86 image and a documented device ABI.

**Contract:**

- Android version, ABI, kernel, HAL API level, and graphics stack are pinned.
- Kernel has the needed virtio PCI/MMIO, block, input, net, console, RNG, and
  GPU drivers enabled.
- HAL module names, symbols, permissions, SELinux labels, and init services are
  installed in the correct image.
- The guest driver uses the same virtio feature/protocol version as the host.
- EGL/GLES/HWC calls either work or return a declared unsupported result; no
  success stubs are allowed on the boot path.
- SurfaceFlinger can acquire, compose, and release a frame buffer.

**Tests:** kernel device probe; init service; `dumpsys SurfaceFlinger`; EGL
create/make-current/swap; GLES clear/draw; SystemUI screenshot; app launch.

### E17. ValidationLoop

**Role:** run truth-based gates in browser and CI.

**Inputs:** runtime event bus, device handles, test vectors, expected pixels,
boot milestone predicates.

**Outputs:** immutable run record with gate ID, status, start/end time, evidence,
error code, and runtime epoch.

**Contract:**

- Initial status is `PENDING`, never `PASSED`.
- A gate can pass only from an observed assertion in the current run epoch.
- Missing prerequisites yield `BLOCKED`; they do not yield `FAILED` or `PASSED`.
- Isolated synthetic tests never count toward OS readiness or browser evidence.
- Gates run in dependency order; independent checks may run in parallel.
- The result is published as `window.__VALIDATION_RESULTS__` and as JSON in the
  log stream.
- A stale result cannot overwrite a newer run.

**Tests:** missing WASM; adapter loss; gate timeout; rerun; stale result; one
failed gate with later independent gate; browser refresh.

### E18. Observability and fault policy

**Role:** make a failed boot diagnosable.

**Inputs:** all state changes, queue events, browser errors, guest serial, GPU
validation messages.

**Outputs:** structured event ring, human log, counters, and crash bundle.

**Contract:**

- Every event has `epoch`, `seq`, `ts`, `src`, `kind`, and typed payload.
- Logs are bounded and redact guest data not needed for diagnosis.
- Errors have stable codes and a recovery hint.
- A green UI state requires a matching event, not a default DOM value.

**Tests:** event order; ring wrap; error serialization; redaction; export.

## 5. Stitch and I/O contracts

### 5.1 Boot stitch

```text
ValidationLoop.asset_check
  -> GuestImageStore.verify
  -> VmRuntime.create
  -> VirtioBus.register_all
  -> VirtioBlk.open(system/vendor/product/userdata)
  -> VmRuntime.start(kernel, initrd, bootargs)
  -> VirtioConsole.kernel_log
  -> Android init
  -> SurfaceFlinger + SystemUI ready event
  -> WebGpuSwapchain.present
```

Boot must stop at the first missing contract. The UI must show the exact stage
and stable error code.

### 5.2 Frame stitch

```text
Guest GLES/EGL/HWC
  -> guest virtio-gpu queue
  -> VirtioBus DMA view
  -> E11 resource/ctx/fence checks
  -> E13 GLES2WGPU or declared 3D backend
  -> E14 compositor
  -> E15 canvas surface
  -> frame_presented(epoch, frame_id)
```

The production path must not pass through `putImageData`. A CPU pixel injector
is not a fallback for this product and cannot be exposed by the browser entry.

### 5.3 Input stitch

```text
DOM event
  -> BrowserHost sequence + coordinate map
  -> VirtioInput queue
  -> guest evdev/input subsystem
  -> Android InputReader/InputDispatcher
  -> focused app
```

Every event must be traceable by sequence ID from browser log to guest ack or
queue completion.

### 5.4 Disk stitch

```text
Guest virtio-blk request
  -> VirtioQueue validation
  -> GuestImageStore block actor
  -> OPFS/IndexedDB transaction
  -> used ring + IRQ
```

The disk actor owns ordering. The VM must never read a half-committed userdata
transaction.

### 5.5 Net stitch

```text
Guest virtio-net TX
  -> VirtioNet policy/proxy
  -> browser fetch/WebSocket/WebRTC
  -> RX queue
  -> guest network stack
```

The chosen mode and its limits must be shown in the build manifest and in the
validation result.

## 6. Parallel execution plan

### Work lanes

| Lane | Work | Owns | Depends on | Exit proof |
|---|---|---|---|---|
| W0 | Truth + build | `plan.md`, build, CI, error codes | none | clean bundle + no false UI |
| W1 | VM shell | v86 adapter, worker, lifecycle | W0 | kernel reaches serial |
| W2 | virtio core | PCI, queues, DMA, IRQ | W1 interface only | queue fuzz + used ring |
| W3 | block + boot media | image manifest, OPFS, blk | W0, W2 | Android kernel reads disk |
| W4 | console/input/RTC/RNG | browser I/O devices | W2 | boot log + input echo |
| W5 | net/audio | proxy net + sound | W2, W4 | net/audio smoke |
| W6 | GPU transport | real virtio-gpu + fences | W2, W3 | Linux GPU probe |
| W7 | GPU render | GLES, resource map, comp | W6 | guest draw to canvas |
| W8 | Android guest | kernel/HAL/AOSP image | W3, W4, W6, W7 | SystemUI ready |
| W9 | browser loop | validation UI + browser tests | W0, W1, W6 | truth gate JSON |

### Disjoint ownership map

| Lane | `OWNS:` paths | May consume |
|---|---|---|
| W0 | `plan.md`, `GATES.md`, `scripts/**`, build metadata | none |
| W1 | `src/vm/**`, `src/boot/**`, worker adapter | W0 contracts |
| W2 | `src/virtio/**`, queue/DMA/IRQ core | W0, W1 interfaces |
| W3 | `src/io/block/**`, `src/storage/**`, `images/**` | W0, W2 interfaces |
| W4 | `src/io/input/**`, `src/io/console/**`, `src/io/clock/**` | W0, W2 interfaces |
| W5 | `src/io/net/**`, `src/io/audio/**` | W0, W2, W4 interfaces |
| W6 | `src/gpu_transport/**`, `crates/virtio_gpu_bridge/**` | W0, W2, W3 interfaces |
| W7 | `crates/gles2wgpu/**`, `crates/webgpu_compositor/**`, `crates/webgpu_swapchain/**` | W6 interface |
| W8 | `guest/**`, pinned boot/image manifests | W3, W4, W6, W7 interfaces |
| W9 | `index.html`, `src/validation/**`, `tests/browser/**` | all frozen runtime contracts |

Root files not listed above are integration-owned and require a branch gate.

### Parallel rules

- W0 starts first and publishes types, error codes, event schema, and build
  manifest. All lanes consume those files.
- W1, W3, W4, and W9 can work in parallel after W0 interfaces are frozen.
- W2 is the shared critical lane; do not duplicate queue logic in each device.
- W5 can proceed with fake loopback transport while W3 builds disk.
- W6 can use a host-only Linux wire replay until W8 has a guest image.
- W7 can use recorded guest packets, but its exit proof must include a real guest
  packet replay and a browser screenshot.
- W8 is the integration lane. It must not redefine host contracts.
- W9 may expose only the real gate states. Synthetic tests may be reported in
  unit-test output, but they cannot be surfaced as browser OS evidence.

### Suggested task split

1. W0: freeze `types`, `errors`, `events`, manifest, and `ValidationLoop` JSON.
2. W1: integrate v86 in a worker and add a serial-only boot fixture.
3. W2: land queue/DMA fuzz harness and one minimal device adapter.
4. W3: land image verify/cache and a block replay device.
5. W4: land console plus input; use a tiny Linux guest before Android.
6. W5: land the chosen net mode and audio policy.
7. W6: replace direct JS GPU calls with queue-backed virtio-gpu.
8. W7: select and implement one real 3D protocol; remove custom-only claims.
9. W8: build/pin Android-x86 and install the HAL/kernel changes.
10. W9: make browser UI consume only runtime events and run the gate DAG.

## 7. Validation matrix

### G0: bundle

- Build from a clean checkout.
- Assert every module and asset returns HTTP 200.
- Instantiate WASM and call a no-op exported function.
- Fail if `pkg/` is absent or browser console has an import error.

### G1: VM

- Start v86 with a tiny known boot image.
- Assert BIOS/boot marker in serial.
- Pause, resume, reset, and assert state events.
- Kill the VM and assert no worker/timer remains.

### G2: virtio core

- Probe PCI and negotiate only implemented features.
- Submit valid and malformed descriptor chains.
- Assert used-ring count, IRQ, and error status.
- Fuzz guest addresses and descriptor loops.

### G3: block and persistence

- Read boot sectors.
- Write a userdata marker, flush, reset VM, and read it back.
- Reload the tab and repeat.
- Fill quota and assert a typed storage error.

### G4: input and console

- Boot serial log reaches UI.
- Key, pointer, wheel, and touch events reach a guest test app.
- Blur sends key-up for all pressed keys.

### G5: network/audio

- Guest gets the selected link mode.
- Run DNS/TCP or the declared proxy smoke.
- Start audio after user activation and report underrun state.

### G6: GPU wire

- Replay Linux virtio-gpu probe packets.
- Create/attach/transfer/flush/unref a real resource.
- Validate fence order and invalid ID errors.
- Assert scanout pixels come from guest DMA, not injected JS data.

### G7: Android boot

- Kernel sees all required devices.
- `init` reaches `zygote` and `system_server`.
- SurfaceFlinger reports a connected display.
- SystemUI screenshot is non-empty and stable across two frames.

### G8: Android app

- Launch a GLES test APK inside the guest.
- Exercise EGL create/make-current/swap.
- Exercise texture, shader, FBO, depth, blend, and input paths.
- Keep a frame counter and assert no GPU protocol error.

### G9: resilience

- WebGPU device loss and recovery.
- Tab hide/show.
- VM reset.
- Network loss.
- Storage failure.
- Guest GPU reset.

### Required test forms

- Rust unit tests for parsers, queue rules, resource ownership, and state.
- Rust property/fuzz tests for guest memory and descriptor chains.
- JS tests for browser host and event sequencing.
- Browser tests for import, WASM, canvas, input, storage, and validation JSON.
- Golden wire tests from captured Linux virtio-gpu traffic.
- Full VM tests with a pinned Android image.

## 8. Browser validation loop contract

The page must expose one run API:

```js
window.runValidationLoop = async () => {
  // returns the immutable run record
};
```

### Production runtime module contract

The generated package at `pkg/android_vm.js` is the sole browser runtime entry.
It must export these exact capabilities:

```js
export default function initWasm(): Promise<void>;
export function createAndroidRuntime(input: {
  canvas: HTMLCanvasElement,
  onEvent: (event: RuntimeEvent) => void
}): Promise<AndroidRuntime>;

interface AndroidRuntime {
  runValidationLoop(input: {
    runId: string,
    epoch: number,
    onGate: (gate: GateResult) => void
  }): Promise<RuntimeRunResult>;
  destroy(): Promise<void>;
}
```

`RuntimeEvent` must contain `epoch`, `seq`, `ts`, `src`, `kind`, and a typed
payload. `GateResult` must contain one of `g0` through `g9`, a terminal status
(`PASSED`, `FAILED`, or `BLOCKED`), evidence, and either `error: null` or a
stable error code. `RuntimeRunResult` must echo `runId` and `epoch`, include all
ten terminal gates, and set `ready` only when all ten are `PASSED`. The module
must not export a synthetic fallback or silently substitute a CPU frame path.

The record shape is:

```js
{
  runId: "uuid",
  epoch: 7,
  startedAt: "iso-8601",
  endedAt: "iso-8601",
  ready: false,
  gates: {
    g0: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g1: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g2: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g3: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g4: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g5: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g6: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g7: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g8: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" },
    g9: { status: "BLOCKED", evidence: [], error: "RUNTIME_NOT_BUILT" }
  }
}
```

Until the VM and guest lanes land, the page must remain blocked. It must not run
the direct bridge pixel suite, synthesize Android frames, or turn any legacy
fixture into OS evidence. The loop must dynamically load the production WASM
module inside its `try/catch`; a missing `pkg` must update the UI to `BLOCKED`
rather than aborting module evaluation before the error handler exists.

## 9. Index refactor requirements

The `index.html` change is intentionally small but strict:

1. Remove every static WASM, bridge, arcade, and synthetic-test import.
2. Use a dynamic import of `./pkg/android_vm.js` from `bootstrap()` so missing
   build output is visible.
3. Keep all ten initial gate labels `PENDING`; never ship preset success.
4. Keep initial stack text `BOOT WAIT` until the real runtime loads.
5. Expose only `PENDING`, `RUNNING`, `BLOCKED`, `PASSED`, and `FAILED`.
6. Require WebGPU, WASM init, runtime factory, and validation-loop contracts.
7. Delegate all gate work to the runtime's `runValidationLoop()`.
8. Reject unknown gate IDs, non-terminal gate results, stale epochs, and partial
   result records.
9. Publish `window.__VALIDATION_RESULTS__` after every run.
10. Keep stable errors in both the log and the immutable result record.

### Legacy code disposition

`src/arcade_demo.js`, `src/test_suite.js`, and `src/virtio_gpu_device.js` may be
retained only as isolated unit-test fixtures while the real runtime is built.
They must never be imported by `index.html`, never feed `window.runValidationLoop`,
and never contribute to `ready` or any `g0`–`g9` evidence. The integration lane
must delete or archive them before release if they are no longer needed.

## 10. Risk and decision points

### Must decide before W6

- Guest target: Android-x86 version and ABI.
- VM core: v86 fork/version and worker model.
- Virtio mode: legacy, modern, or both.
- 3D protocol: Virgl, gfxstream, or a deliberately smaller GLES stream with a
  matching guest driver.
- Disk backend: OPFS, IndexedDB, or streamed read-only images plus OPFS userdata.
- Net mode: proxy, WebSocket, or WebRTC.
- Browser support floor: Chrome/Edge versions and WebGPU requirement.

### Main risks

- Full Android boot is CPU-heavy; a worker and frame budget are mandatory.
- Android graphics ABI changes by release; pin one release first.
- Browser networking cannot provide unrestricted Ethernet; the guest contract
  must reflect the proxy model.
- A fake GPU protocol can pass local tests while Linux guest traffic fails;
  golden wire tests and a real guest probe are mandatory.
- Large images exceed browser cache/quota; streaming and resumable cache are
  required.

## 11. Exit checklist

- [ ] Build creates and serves all browser assets.
- [ ] v86 boots a pinned Android image.
- [ ] Guest disk persists.
- [ ] Console and input work.
- [ ] Net mode is declared and tested.
- [ ] Audio policy is declared and tested.
- [ ] Real virtio-gpu queue traffic works.
- [ ] Real guest scanout reaches WebGPU canvas.
- [ ] SurfaceFlinger and SystemUI are visible.
- [ ] A GLES APK renders and accepts input.
- [ ] Browser validation has no preset green state.
- [ ] CI runs every required gate.

## 12. Unlazy execution log

### Current step

Verdict response: M1 + M-E2E sign-off is BLOCKED (not done for a full Android
OS in the browser). Landed: P6 false-green reverted, shared-RAM gate +
transfer list, worker device registry, observable IRQ delivery log, real v86
load path, PCI bounds clamp, monotonic watchdog ack, watchdog stop on
`EVT_ERROR`, `reset()` rejection on `DESTROYED`, `setSize` integer check,
stable codes inside messages, fail-closed start gate, default boot device
map, worker-side V86WorkerBackend against the real {V86} starter API
(autostart + emulator-loaded protocol, ArrayBuffer normalization),
browser G0/G1/G3 live-boot/provenance gates, and E2E prod coverage 62/62
(blocking CI).
Live-guest milestone (2026-09-25): vendored v86 core boots the authentic
32-bit Android-x86 9.0-r2 kernel — serial shows `Linux version 4.19.110`,
`Run /init`, `Detecting Android-x86` (docs/boot-evidence-9.0-r2-serial.log,
plus live browser-tab G1 PASSED: v86-live-boot, backend:v86, serial:9761B).
Still required: system/vendor/product squashfs media (init stalls probing
for it), SurfaceFlinger/SystemUI frames, and G0–G9 PASSED in one epoch.

### Checks to run before any gate is marked complete

- Parse `GATES.md` and verify every runnable gate has an indented `CHECK:` and
  `EXPECT:` line.
- Run the plan contract checker and confirm all LLD entities, stitching, I/O,
  parallel lanes, and validation gates are present.
- Run the index contract checker and confirm no synthetic path is imported.
- Run JS syntax checks and `cargo test --workspace`.
- Serve the entry over HTTP and confirm missing `pkg/android_vm.js` produces a
  visible blocked state, never a green state.

### Evidence policy

Evidence is recorded beside the exact gate only after the command was run and
its expected marker was observed. A planned check, a passing host unit test, or
an isolated fixture is not evidence of Android boot. No gate in the root ledger
may be marked complete until its acceptance proof exists.
