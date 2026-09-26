# HLD — Native ARM Android-in-Browser (Path N)

Companion: `../../grandvision.md` (vision), `LLD.md` (per-unit contracts), `SWARM.md` (parallel build plan).

## 1. Context

Path E (v86 + Android-x86) proved the GPU translation half is real code, but left the two
load-bearing artifacts as stubs: the production WASM runtime (8 bytes) and a bootable system
image. Path N replaces the CPU half with an ARM-native design and wires the 3D path that
Path E never connected.

Honest definition: browser kabhi KVM/hardware virtualization nahi dega. "Native" ka matlab
**same-ISA JIT** hai — AArch64 guest instructions ka WASM mein JIT, jo browser phir ARM
machine code mein lower karta hai. True native execution nahi; emulation tax kaafi kam.

Why ARM guest instead of x86 guest:
- APK compat: ~99% real APKs arm64 native libs ship karte hain. x86 Android ko Houdini
  (binary translation) chahiye hota hai — slow aur games mein aksar toota hua.
- Simpler JIT: AArch64 RISC hai, fixed-width instructions. x86 JIT (variable-length, complex
  flags) se likhna aur optimize karna aasaan.
- No double translation: ARM lib → ARM guest → WASM → ARM host. x86 path mein ek extra
  semantic hop tha.

## 2. Architecture

```
BROWSER (host — ARM laptop/desktop ya Android Chrome)
=====================================================
│
├─ armjit ───────────────────────────── [NEW]
│   AArch64 decode → IR lift → WASM JIT
│   Pehle Unicorn-WASM spike, phir custom JIT
│
├─ device model ─────────────────────── [PARTIAL — Path E se]
│   virtio-blk (system.img) · virtio-net (WS NAT proxy)
│   virtio-gpu (3D cmd stream) · virtio-input/console
│
├─ GPU half ─────────────────────────── [REAL — as-is reuse]
│   virtio-gpu → virtio_gpu_bridge → gles2wgpu
│   → webgpu_compositor → webgpu_swapchain → canvas
│   (Path E mein 3D path kabhi wire nahi hua — yahan hoga)
│
├─ boot snapshot (JIT-warmed state) ─── [NEW]
├─ wasm-pack production build ───────── [NEW — aaj 8 bytes hai]
├─ browser adapters ─────────────────── [NEW]
│   WebGPU surface · WebSocket net · input · storage
│   (externals boundary — side-effect containment)
└─ metrics_overlay ──────────────────── [REAL — reuse]

GUEST
=====
AOSP arm64, cut-down build (no telephony, minimal system apps)
  App → GLES → patched HAL (gralloc/hwcomposer/egl) → virtio-gpu → host
  [HAL patches PARTIAL — C++ hai, compile nahi hua]
  [real system.img MISSING — gap #1]
```

## 3. Data flows

**Boot:** page load → WASM runtime init → snapshot restore (ya cold boot: kernel → init →
SurfaceFlinger) → launcher frame composited to canvas. Target: cold < 30s, snapshot < 5s.

**Frame render:** app GLES call → guest HAL → virtio-gpu command → bridge decode → gles2wgpu
translate (GLSL→WGSL via Naga) → compositor layers → swapchain present → canvas. Har step ka
contract LLD mein frozen hai; koi step doosre ke internals nahi jaanta.

**Input:** browser touch/keyboard → input adapter (sanitize + normalize) → virtio-input →
guest. Adapter boundary par saari impurity quarantine.

**APK sideload:** APK bytes → apk_gpu_analyzer (engine detect: Unity/Unreal/Godot) →
adb-protocol-over-websocket → guest package manager → install → launch intent.

## 4. Key decisions

- **Full-system emulation, not user-mode syscall translation.** "Poora Android" chahiye —
  real kernel, real drivers, real SurfaceFlinger. User-mode (WSL1-style) faster hota par
  games ke liye driver stack nakli hota. (Tradeoff: JIT overhead har instruction par.)
- **Unicorn-WASM pehle, custom JIT baad mein.** M0/M1 Unicorn se prove karo (weeks),
  parallel mein custom AArch64→WASM JIT likho (months). Interpreter 20–50x slow hai —
  games ke liye nahi, boot prove karne ke liye haan.
- **virtio-gpu + GLES→WebGPU, not Venus/Vulkan.** Path E ka tested code reuse hota hai.
  Guest Vulkan (Venus protocol) baad ka optimization hai.
- **Snapshot boot.** Har baar 60s+ cold boot unacceptable hai. JIT-warmed machine state
  serialize karke rakho, restore karo.

Rejected: AOSP→WASM recompile (moonshot), x86 guest retain karna (Houdini tax),
cloud-streaming (local-first principle ke khilaaf).

## 5. Risks

- **JIT engineering cost** — sabse bada risk. AArch64→WASM JIT months ka kaam hai.
  Mitigation: Unicorn spike se value jaldi, JIT parallel track par.
- **WASM 4GB linear memory ceiling** — guest RAM 2GB budget, baaki JIT code cache. Tight.
  Mitigation: aggressive ballooning, memory64 proposal par nazar.
- **WASM threads requirement** — COOP/COEP headers mandatory, warna multi-vCPU nahi.
  Mitigation: hosting contract mein likha hoga; single-vCPU fallback.
- **GLES→WebGPU feature loss** — kuch GLES 3.x features girenge. Mitigation: conformance
  test suite per extension, gap list public.
- **AOSP build time** — full AOSP arm64 build hours leta hai, iteration slow.
  Mitigation: prebuilt cut-down system.img artifact, CI cache.

## 6. Phases and gates

- **M0 — Spike:** ARM Linux kernel, Unicorn-WASM par, page mein shell tak boot.
  Gate: serial boot log in-page. (Weeks.)
- **M1 — Device model:** AOSP arm64 launcher tak boot, software rendering.
  Gate: launcher ka pehla composited frame, screenshot evidence.
- **M2 — GPU path:** virtio-gpu + gles2wgpu wired, 2D APK install + run.
  Gate: APK launch ka 30fps video capture.
- **M3 — JIT:** custom JIT interpreter ko replace kare; Unity demo APK playable.
  Gate: playable session, input latency measured < 100ms.
- **M4 — Product:** snapshot boot < 5s, sideload UX, HUD.
  Gate: cold user — URL kholo, 5s mein game.

Har gate measured evidence mangta hai. "Code exists" gate pass nahi karata.

## 7. Metrics

- Boot: cold < 30s, snapshot < 5s.
- UI: 30fps sustained, frame p95 < 50ms.
- JIT: Dhrystone-ish microbench par native ka max 5x slow.
- Correctness: GLES conformance subset, APK install success rate.
