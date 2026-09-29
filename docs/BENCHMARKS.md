# Path N Benchmarks

Measured 2026-09-30. Every number below was produced by running the named
harness on the named machine — nothing is estimated, interpolated, or
carried over from other runs. Re-run the harness to reproduce; single-day,
single-machine numbers, compare only against fresh runs on the same rig.

## 1. Native: wasmi vs wasmtime

Harness: `units/u12-orchestrator/examples/bench_platform.rs`

```sh
cargo run -p u12-orchestrator --example bench_platform --release
```

- **Cold boot**: executor construction + image load + run to the
  `pathn-sh> ` WFI park. Median of 5.
- **Workload**: boot, then `help\n`, `echo hi\n`, `bogus\n` typed through
  the real `KeyboardInput` adapter path (`push_event` per keystroke, the
  same path the browser uses). Wall time + total vCPU steps → steps/sec.
- **Input latency**: `push_event` → echoed byte visible in console TX.
  Median of 5.
- Boot evidence is asserted per run (WFI at `0x4000_0070`, exact prompt
  bytes); workload output is asserted (`commands:`, `hi`, `unknown cmd`).

Machine: Linux x86_64, AMD EPYC 9D25 (2 vCPU), rustc/cargo 1.98.1,
`--release`.

| backend  | boot_to_prompt_ms | workload_ms | workload_steps | steps_per_sec | input_latency_ms |
|----------|-------------------|-------------|----------------|---------------|------------------|
| wasmi    | 2.6               | 30.0        | 1193           | 39,788        | 0.53             |
| wasmtime | 52.6              | 336.6       | 1193           | 3,544         | 8.18             |

Notes:

- Both backends execute the identical 1,193 guest steps (deterministic
  guest) — the gap is host-side cost, not guest work.
- wasmtime cold boot includes Engine creation + JIT compile of the block
  modules; wasmi has no compile step. A warm, reused wasmtime executor
  would boot faster than 52.6 ms — not measured here.
- wasmi is the interpreter backend; wasmtime is the JIT. On this tiny
  guest the JIT's compile + per-block call overhead dominates, so the
  interpreter wins by ~11x on steps/sec. Do not generalize to large
  guests.

## 2. Browser: boot to `pathn-sh> `

Harness: `www/bench.html` (committed; served copy synced to the box).

```sh
# on the box, page already served at http://127.0.0.1:8124/bench.html
python3 /tmp/cdp_bench.py 'http://127.0.0.1:8124/bench.html'
```

Method: headless Chromium, page measures with `performance.now()` and
writes results into `document.title`; a CDP client polls the title until
it flips. No `--virtual-time-budget`, so the deltas are wall clock.
`web_host_bg.wasm` is the committed 2,873,241-byte build
(`application/wasm`); the wasm guest runs the wasmi backend.

Machine: gsai box, Intel i7-7700 @ 3.60GHz (8 CPU), Chromium 153.0.8010.47
(snap, headless), page served over localhost.

| run | wasm_load_compile_ms | boot_construct_ms | boot_to_prompt_ms | steps | input_latency_ms |
|-----|----------------------|-------------------|-------------------|-------|------------------|
| 1   | 154.0                | 96.0              | 250.4             | 112   | 6.9              |
| 2   | 300.7                | 97.1              | 398.1             | 112   | 5.2              |
| 3   | 236.7                | 119.6             | 356.9             | 112   | 10.2             |
| **median** | **236.7**       | **97.1**          | **356.9**         | 112   | **6.9**          |

Definitions:

- **wasm_load_compile_ms**: fetch + compile of `web_host_bg.wasm`.
  Varies run to run (154–301 ms observed) — localhost fetch + compiler.
- **boot_construct_ms**: `new PathnShell()` — includes the synchronous
  boot run to the WFI park and the prompt assertion.
- **boot_to_prompt_ms**: script start → prompt bytes confirmed.
- **input_latency_ms**: `push_key('h')` → echoed `h` visible in TX,
  stepping synchronously. No rAF involved; real key-to-photon latency
  adds compositor/frame time on top — not measured.
- **steps**: vCPU blocks executed during boot (stable at 112 across runs).

## 3. What is NOT measured

- Steady-state wasmtime (warm executor) boot.
- Frame-time / rAF pacing in the browser (the bench steps synchronously).
- Multi-vCPU throughput (single vCPU is the current platform; see
  `docs/architecture/HLD-vcpu-smp.md`).
- Android/guest workloads beyond `pathn-sh`.
