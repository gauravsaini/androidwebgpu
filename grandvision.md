# Grand Vision: Poora Android, Browser Mein

**One line:** Koi bhi Android app ya game — khaas taur par Unity / Unreal / Godot titles —
kisi bhi browser mein khule. Bina install, bina Play Store, GPU-accelerated, URL kholo aur chalao.

**Why this matters**
- Distribution becomes a link. APK bhejo, store approval ka wait nahi.
- Preservation: purane games/devices marte nahi, browser mein zinda rehte hain.
- Local-first alternative to cloud gaming — tumhara compute, tumhara device, koi server rent nahi.

## The two paths

- **Path E (existing, `main` branch):** Android-x86 inside v86. CPU emulation tax +
  ARM APKs ke liye Houdini translation. Prototype ke taur par rakho, reference ke liye.
- **Path N (this doc):** ARM-native. AOSP arm64 guest, AArch64→WASM JIT, host GPU via WebGPU.
  ARM APKs bina kisi translation ke chalte hain — kyunki 99% real APKs arm64 hi hain.

Path N is the balle-balle path. Path E uska stepping stone tha, manjil nahi.

## Principles (law, not suggestions)

1. **Contracts are law.** Swarm parallel mein banayega; units ke beech ek hi coupling hai — frozen contract. Contract tode, build toota.
2. **Pure where possible.** Deterministic units, explicit state, no hidden coupling, no covert side-effects.
3. **Reuse the real.** `gles2wgpu`, `virtio_gpu_bridge`, `webgpu_compositor`, `webgpu_swapchain`,
   `apk_gpu_analyzer`, `metrics_overlay` — already exist, tested hain. Rewrite nahi, wiring karo.
4. **Honest gates.** Gate sirf measured evidence par pass: boot log, rendered frame, fps number.
   "Code exists" gate pass nahi karata. (Lesson from Path E: README ne "full APK support" claim kiya,
   reality mein koi APK kabhi render nahi hua.)
5. **Browser is the OS.** COOP/COEP, WASM threads + SharedArrayBuffer, WebGPU. Koi native plugin nahi.

## Non-goals (v1)

- Play Store / GMS licensing.
- Telephony / RIL / SMS.
- Camera passthrough.
- iOS host (WASM threads + JIT constraints; revisit later).

## Success criteria (balle balle = sab tick)

- AOSP arm64 page mein launcher tak boot: cold < 30s, snapshot se < 5s.
- 2D APK (settings, simple app): stable 30fps.
- Unity demo APK: playable, input + audio ke saath.
- Targets: mid-range ARM laptop browser + flagship phone ka Chrome (Android 12+).

## Swarm execution rules (owner code par strictly)

1. **Independent units:** LLD ka har box ek completely independent, modular unit. Koi unit doosre
   unit ke internals import nahi karega — sirf frozen contract.
2. **Strict explicit contracts:** Har unit ka input/output contract explicit hai —
   defined types/interfaces/schemas, explicit params/returns. Docs mein signature ke bina unit adhura hai.
3. **Pure function behavior:** No hidden internal state, no mutable shared state, no implicit
   dependencies, no covert side-effects. Jahan possible, inputs→outputs deterministic.
   Jahan state zaroori hai (MMU, IRQ, device queues), state explicit struct mein thread hoga —
   function ke andar chhupa nahi.
4. **Controlled scope boundary:** Ye rules hamare owned code par apply hote hain. External components
   (browser engine, WebGPU, WebSocket, OS interfaces, third-party libs) excluded hain —
   **lekin** unke adapters par boundary isolation aur side-effect containment mandatory hai.
   Saari impurity adapters aur orchestrator mein quarantine hogi, kahin aur nahi.

Execution plan: `docs/architecture/SWARM.md`.
