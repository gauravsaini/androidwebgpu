# Plan — Path N parallel execution

Date: 2026-09-30. Driver: Muse (autonomous).
Supersedes the 2026-09-26 Wave 0–2 plan (preserved in git history).
Branch: `feat/native-arm-vision`. Language: Rust (preferred).

## 1. Frozen starting point

- Tip: `64210e5` — pushed, tree clean, pre-push gate green
  (`cargo test --workspace` + `wasm32-unknown-unknown` web-host build).
- Design docs frozen: `grandvision.md`, `docs/architecture/HLD.md`,
  `docs/architecture/LLD.md`, `docs/architecture/SWARM.md`,
  `docs/architecture/HLD-browser-host.md`, `docs/architecture/LLD-browser-host.md`.

### Done (measured evidence on file)

- Contracts (`contracts/`), U1 decode, U2 IR lift, U3 wasm-jit, U4 MMU,
  U5 GIC/timer, U6 virtio-transport, U7 gpu-device, U8 gpu-host,
  U10 analyzer-conformance, U11 snapshot, U12 orchestrator, U13 adapters
  (DOM key-event → console RX), U14 metrics, U15 exec backends
  (wasmtime native / wasmi wasm32).
- Browser host (`crates/web-host` + `www/`): real `pathn-sh` guest in a real
  browser. Headless Chromium acceptance 6/6 via real `KeyboardEvent`s
  (1221 vCPU steps). Interactive demo live: `http://100.104.140.2:8124/`
  (gsai box, tailnet-only).

### Honest not-done list (carried forward, no optimism)

- No Android boot. No launcher. No APK. No guest WebGPU path in the
  browser host yet (U7/U8 exist as units; not wired to a canvas).
- ASCII-only console input. Single vCPU. Cooperative WFI.
- Wasmi browser performance not benchmarked (no numbers claimed).

## 2. Objective of this phase

Four parallel tracks that together move Path N from "bare-metal shell in
a browser" toward "AOSP (arm64) in a browser with WebGPU". Each track is
independently shippable; each has measured done-criteria.

## 3. Tracks

### Track A — GPU path in the browser host

- Goal: guest GPU commands render on a real WebGPU `<canvas>` in `www/`.
- Scope: wire U7 `GpuCmd` through `web-host` to WebGPU; reuse Path E
  crates (`virtio_gpu_bridge`, `gles2wgpu`, `webgpu_compositor`,
  `webgpu_swapchain`) where cheap — no rewrites.
- Out of scope: full GLES translation, production shaders.
- Branch: `feat/pathn-gpu-browser` (from `64210e5`).
- Done: golden triangle screenshot test green in real headless Chromium
  (pixel assertion), committed under `www/`; `docs/architecture/` note.

### Track B — Android guest boot (U9, longest pole)

- Goal: boot a real AOSP kernel + minimal rootfs to `init` under the
  Path N vCPU (native first, wasm after).
- Scope: `guest-image/` crate; prebuilt kernel + minimal rootfs first;
  full `system.img` explicitly deferred.
- Out of scope: launcher, system.img, APK install.
- Branch: `feat/pathn-guest-boot` (from `64210e5`).
- Done: `init` reached with committed boot-log evidence.
- Honest rule: a genuinely impossible gate gets `ABANDON: <measured reason>`
  + handoff. Never a fake boot, never silent skip.

### Track C — Platform hardening + performance

- Goal: close the measured gaps in the current platform.
- Scope: UTF-8 console input (tests green); honest benchmark report
  `docs/BENCHMARKS.md` — wasmi vs wasmtime, browser boot time, input
  latency, steps/sec (measured numbers only, no claims without data);
  multi-vCPU design doc (prototype only if cheap).
- Out of scope: rewriting the executor, production perf tuning.
- Branch: `feat/pathn-platform` (from `64210e5`).
- Done: UTF-8 tests green; `docs/BENCHMARKS.md` committed; vCPU doc
  committed.

### Track D — APK / sideload path

- Goal: sideload a test APK into the guest rootfs via a mock-adb path.
- Scope: mock-adb test; `apk_gpu_analyzer` contract conformance; no
  rewrite of the analyzer.
- Out of scope: Play Store, real device adb, APK execution.
- Branch: `feat/pathn-sideload` (from `64210e5`).
- Done: mock-adb test green; conformance note committed.

## 4. Swarm laws (binding, from `docs/architecture/SWARM.md`)

1. **No cross-imports.** A unit imports only `contracts/` (+ its own).
   Impurity (`now()`, threads, I/O, randomness) stays in U12/U13.
2. **One branch per track**, all from `64210e5`. No direct pushes to
   `feat/native-arm-vision`; no force-push anywhere.
3. **Conventional commits**: `type(scope): description`.
4. **Local CI is the source of truth.** Pre-push gate green before any
   push. GitHub Actions is secondary.
5. **Evidence gates.** "Complete" without measured evidence (boot log,
   screenshot, fps number, test output) is not complete.
6. **Contract change protocol.** Schema change → version bump + explicit
   re-ack from affected tracks. Silent change = revert.

## 5. Merge order

Tracks merge into `feat/native-arm-vision` in this order, each only after
its done-criteria are met and the gate is green: **A → C → D → B**
(B merges last — longest pole, biggest blast radius).
Each merge: rebase on current tip, gate green, push.

## 6. Milestones & reporting

- **M1** — all four tracks branched, designs locked.
- **M2** — each track: implementation + tests green (per-track done-criteria).
- **M3** — merged to `feat/native-arm-vision` in order, CI green.
- Status discipline: one line per milestone per track. Milestone updates
  only — no micro-step narration.

## 7. Non-goals (this phase)

Full `system.img`, launcher UI, production/public hosting, x86 guest
paths, app-store or monetization work.
