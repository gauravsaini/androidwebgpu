# HLD — Browser host for Path N (`pathn-sh` in the browser)

Status: design locked 2026-09-28. Scope: run the REAL 4.1 `pathn-sh` guest in a
real browser. Waves 0–5 + console-RX are done (wasmtime harness); this doc covers
only what is new for the browser.

## 1. Problem

The emulator core is pure Rust and portable, but execution is pinned to native:
u12 drives U3-emitted WASM modules with **wasmtime**, whose Cranelift JIT cannot
target `wasm32`. There is no browser host: no DOM listeners, no wasm build of the
emulator, no console rendering. Goal: page load → real guest boots → `pathn-sh>`
prompt in the DOM, interactive via the keyboard.

## 2. Key decisions

**D1 — Execution backend: `wasmi` (v2).** wasmtime cannot compile for wasm32
(native codegen). Candidates: wasmer (Rust embedding is native-engine only; the
browser build is a JS package, not embeddable in our Rust wasm), wasm3 (C library
— needs a C toolchain and Emscripten shims), wazero (Go). wasmi is pure-Rust,
Parity-maintained, `no_std`+alloc capable, runs on `wasm32-unknown-unknown`, and
supports the full MVP-core feature set U3 emits (i64 arith, mutable globals,
host calls, `unreachable` traps). Honest cost: it is an interpreter, so it
is slower than the wasmtime JIT — performance not yet measured; irrelevant
for a 74-step boot and an interactive
shell; documented as a limitation, not hidden.

**D2 — Backend abstraction, not a fork.** New contract trait
`pathn_contracts::execution::BlockExecutor` (`run_block(&mut self, wasm:
&WasmModule, regs: &mut [u64; 31], host: &mut dyn HostOps) -> Result<(i64,
bool), String>` + `cache_len` + `invalidate`). Two unit crates implement it:
`u15-exec-wasmtime` (native; moved verbatim out of u12) and `u15-exec-wasmi`
(portable). u12 holds `Box<dyn BlockExecutor>`; `Orchestrator::new()` keeps its
signature and picks the default backend by `cfg` (wasmtime on native, wasmi on
wasm32). The compile cache moves INTO the backend, keyed by module **bytes**
(U3 compile is pure/deterministic — bytes are a valid key, and strictly more
correct than the old pc-key for self-modifying code). `block_cache_len()` and
`invalidate_code_cache()` semantics are preserved through the trait.

**D3 — Reuse, don't reimplement.** The browser reuses every pure unit unchanged
(u1, u2, u3, u4, u5, u6, u7, u8, u11, u13, contracts, guest-image). Only the
execution backend swaps. The guest image is built **inside the wasm** at boot
(`GuestManifest::pathn_sh()` + `build()` are pure — no fetch, no I/O).

**D4 — Thin wasm-bindgen seam.** New crate `crates/web-host` exposes one class,
`PathnShell`: `new()` (build image → load → `run_until_halt` → assert WFI +
`TX == b"pathn-sh> "`, else throw — fail-closed), `pump_keys(code, key,
pressed)` (feeds u13 `KeyboardInput`), `step_frames(n)` (pump_input →
step_vcpu × n → drain TX). All DOM work (terminal `<pre>`, keydown listeners,
rAF loop) lives in plain JS (`www/app.js`); Rust never touches the DOM.

## 3. Boot flow (page load → prompt)

1. `app.js`: `init()` the wasm module → `new PathnShell()`.
2. In-wasm: build guest image → `load_image` → `run_until_halt(10_000)` →
   guest prints `pathn-sh> ` (74 steps, measured) → parks at `WFI` (resumable).
3. Constructor asserts halt reason is `Wfi` and TX is exactly `pathn-sh> `;
   otherwise it throws and the page shows `BLOCKED` (fail-closed, no fake prompt).
4. rAF loop: drain DOM key queue → `pump_input` → `step_vcpu` × budget →
   append new TX bytes to the terminal.

## 4. What is NOT real (honest bounds)

- wasmi is an interpreter, not a JIT — boot/interaction latency is higher than
  native; no performance claims are made.
- Bare-metal `pathn-sh` shell only — no Android boot, no launcher, no APK, no
  GPU/WebGPU path (u7/u8 exist but the guest does not use them).
- ASCII-only input (u13 `KeyboardInput` MVP); WFI park is cooperative (no real
  interrupt injection); single vCPU; injected cycle counter is the only clock.
- The acceptance page drives input via synthetic `KeyboardEvent`s through the
  SAME JS handler as real typing — the wasm/guest path is real; only the finger
  is synthetic.

## 5. Test strategy

- Native parity: `u15-exec-wasmi` boots the real guest to `pathn-sh> ` in 74
  steps with byte-identical TX vs the wasmtime backend (`cargo test`).
- Browser acceptance: `www/acceptance.html` self-drives `help`, `echo hi`, and
  an unknown command through the real key handler + wasm + guest, then writes
  `ACCEPTANCE: PASS` and the pinned TX bytes into the DOM; verified with
  headless chromium `--dump-dom`.
- Gate: `cargo test --workspace` (pre-push hook) stays green on native; the
  wasm32 build is checked with `cargo build --target wasm32-unknown-unknown`.
