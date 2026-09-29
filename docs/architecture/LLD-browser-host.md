# LLD — Browser host for Path N

Companion to `HLD-browser-host.md`. Interface contracts per entity; Rust 1.98.1;
`$HOME/.cargo/bin` on PATH for every shell.

## 1. `contracts/src/execution.rs` (new; additive contract amendment)

```rust
/// Host operations a U3 module imports. String errors become WASM traps,
/// surfaced as `HaltReason::WasmTrap` — same as today.
pub trait HostOps {
    fn mem_load(&mut self, addr: i64, size: i64) -> Result<i64, String>;
    fn mem_store(&mut self, addr: i64, size: i64, val: i64) -> Result<(), String>;
    /// Notification only; backends track invocation for the `wfi_seen` return.
    fn wfi(&mut self);
}

/// Executes one U3 block: checkpoint regs in, call `run`, read regs back.
/// Returns `(exit_addr, wfi_seen)`. Implementations own a compile cache keyed
/// by module bytes (U3 compile is pure → bytes are a valid, deterministic key).
pub trait BlockExecutor {
    fn run_block(
        &mut self,
        wasm: &WasmModule,
        regs: &mut [u64; 31],
        host: &mut dyn HostOps,
    ) -> Result<(i64, bool), String>;
    fn cache_len(&self) -> usize;
    fn invalidate(&mut self);
}
```

Object-safe (no generics, no `Self` returns). `WasmModule` is
`pathn_contracts::cpu::WasmModule { bytes: Vec<u8> }`.

## 2. `units/u15-exec-wasmtime` (native only)

Moved verbatim from u12: `WasmtimeExecutor { engine, cache:
HashMap<Vec<u8>, wasmtime::Module> }`. `run_block` = old `compile_cached` +
`execute_block`, with `host_mem_load`/`host_mem_store` returning `String` errors
(via `HostOps`) instead of `wasmtime::Error`. `wfi_seen` tracked by a
`TrackingHost { inner: &mut dyn HostOps, wfi_seen: bool }` wrapper.

## 3. `units/u15-exec-wasmi` (portable: native + wasm32)

`WasmiExecutor { engine: wasmi::Engine, cache: HashMap<Vec<u8>, wasmi::Module> }`.
Per `run_block`:
- `Store::new(&engine, host)` where the store data is a `TrackingHost` owning
  `&mut dyn HostOps` (same borrow pattern as wasmtime's `Store<WasmHost>`).
- `Linker<&mut TrackingHost>` with `func_wrap("env", "mem_load" |
  "mem_store" | "wfi", …)` closures of the form
  `|mut caller: Caller<'_, _>, addr: i64, size: i64| -> Result<i64, wasmi::Error>`;
  host errors map via `wasmi::Error::host(HostMsg(String))`
  (`HostMsg: Display + Debug + Send + Sync + 'static` implements
  `wasmi_core::HostError`).
- 31 mutable i64 globals: `Global::new(&mut store, Val::I64(v),
  Mutability::Var)`, defined as `env.r{i}`; read back after the call.
- `instance.get_typed_func::<(), i64>(&store, "run")?.call(&mut store, ())`;
  traps → `Err(format!("trap: {e}"))`.
- wasmi 2.0.0 verified against the local registry source (func_wrap,
  Error::host, Global, get_typed_func all confirmed present).

## 4. u12 refactor (orchestrator)

- Fields `engine: wasmtime::Engine` / `block_cache: HashMap<u64,
  wasmtime::Module>` → `executor: Box<dyn BlockExecutor>`.
- `Orchestrator::new()` → `Self::with_executor(default_executor())`;
  `default_executor()` is `cfg`-selected: `u15_exec_wasmtime` on
  `not(target_arch = "wasm32")`, `u15_exec_wasmi` on wasm32.
- New `pub fn with_executor(Box<dyn BlockExecutor>) -> Self` (injection point
  for the web host and tests).
- `WasmHost` implements `HostOps` (mem/console dispatch unchanged; `wfi()`
  empty — the backend wrapper tracks the flag).
- `block_cache_len()` → `self.executor.cache_len()`; `invalidate_code_cache()`
  → `self.executor.invalidate()`. Public API otherwise unchanged;
  `step_vcpu`/`run_until_halt` flow identical.
- Deps: `wasmtime` removed; `u15-exec-wasmtime` under
  `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, `u15-exec-wasmi`
  unconditional (native parity tests need it too).

## 5. `crates/web-host` (wasm32-unknown-unknown, wasm-bindgen)

```rust
#[wasm_bindgen]
pub struct PathnShell { orch: Orchestrator, kbd: KeyboardInput, tx_drained: usize }
#[wasm_bindgen]
impl PathnShell {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<PathnShell, JsValue>; // fail-closed boot
    pub fn push_key(&mut self, code: &str, key: &str, pressed: bool);
    /// Run up to STEPS_PER_FRAME vCPU blocks; returns NEW tx bytes as String.
    /// Stops early at WFI park (nothing to do until new input) or real halt.
    pub fn step_frame(&mut self) -> String;
    pub fn steps(&self) -> u32;   // diagnostics
    pub fn parked(&self) -> bool;  // true while alive at the WFI prompt
    pub fn tx_text(&self) -> String; // full TX so far (acceptance hook)
}
```

`new()`: `GuestManifest::pathn_sh()` → `build().0` → `load_image` →
`run_until_halt(10_000)`; assert `matches!(halt, HaltReason::Wfi { .. })` and
`console tx == b"pathn-sh> "` (byte-exact, from the Wave-5 measurement), else
`Err(JsValue::from_str("BOOT FAILED: …"))`. TX drain: track `tx_len` over
`console.tx_bytes()` (existing accessor — verify name at impl time), decode the
new slice as UTF-8 lossy (lossy is a display choice, documented; the input
path is UTF-8 clean — bytes are preserved end to end, only the pathn-sh
builtins stay ASCII).

`www/app.js` (no framework): `init()` → `new PathnShell()` → render TX into
`<pre id="term">`; `keydown`/`keyup` listeners call `push_key(e.code, e.key,
true/false)`; `requestAnimationFrame` loop calls `step_frame()` and appends
the returned string; boot failure → terminal shows `BOOT FAILED: <reason>`.
`www/acceptance.html` reuses the same handler: on load, boots, dispatches
synthetic `KeyboardEvent`s for `help\n`, `echo hi\n`, `frob\n` through the REAL
`window` keydown listener path, pumps frames until TX is quiescent, then writes
`ACCEPTANCE: PASS` + the exact TX bytes into `#result` (or `FAIL` + diff).

## 6. Build / test / gates

- `cargo build --target wasm32-unknown-unknown -p web-host`, then
  `wasm-bindgen --target web --out-dir www/pkg`.
- Native: `cargo test --workspace` (pre-push gate) — includes wasmi/native
  parity tests (real guest → prompt, 74 steps, byte-identical TX).
- Browser: serve `www/` over HTTP; headless chromium `--dump-dom
  --virtual-time-budget=20000 acceptance.html`; assert `ACCEPTANCE: PASS` and
  pinned bytes.
- Commits: conventional `type(scope): description`; no force-push; pre-push hook
  must pass.
