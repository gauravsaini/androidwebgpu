//! Track C platform benchmark — MEASURED numbers for `docs/BENCHMARKS.md`.
//!
//! Run (native, release):
//!   cargo run -p u12-orchestrator --example bench_platform --release
//!
//! What it measures, per execution backend (wasmi interpreter, wasmtime JIT):
//! 1. Cold boot: executor construction + image load + run to the `pathn-sh> `
//!    WFI park (median of 5).
//! 2. Scripted workload: boot, then `help\n`, `echo hi\n`, `bogus\n` typed
//!    through the real [`KeyboardInput`] adapter path; total wall time and
//!    vCPU steps -> steps/sec.
//! 3. Input latency: time from adapter `push_event` to the echoed bytes
//!    appearing in console TX (median of 5).
//!
//! Every number printed here is measured on this machine in this run.
//! Nothing is estimated.

use std::time::Instant;

use guest_image::image::{build, GuestManifest};
use pathn_contracts::execution::BlockExecutor;
use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};
use u13_adapters::KeyboardInput;
use u15_exec_wasmi::WasmiExecutor;
use u15_exec_wasmtime::WasmtimeExecutor;

const BOOT_WFI_ADDR: u64 = 0x4000_0070;
const BOOT_PROMPT: &[u8] = b"pathn-sh> ";
const COMMANDS: &[&str] = &["help\n", "echo hi\n", "bogus\n"];

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

/// Type text the browser way: DOM `code` + resolved `key` per keystroke.
fn type_text(kbd: &mut KeyboardInput, text: &str) {
    for ch in text.chars() {
        match ch {
            '\n' => kbd.push_event("Enter", "Enter", true),
            ' ' => kbd.push_event("Space", " ", true),
            'a'..='z' => {
                let code = format!("Key{}", ch.to_ascii_uppercase());
                kbd.push_event(&code, &ch.to_string(), true);
            }
            _ => panic!("bench type_text: no mapping for {ch:?}"),
        }
    }
}

/// Boot one orchestrator with the given executor factory. Returns
/// (wall_ms, steps_to_park). Panics if boot evidence diverges.
fn boot_with<E>(make: impl Fn() -> E) -> (f64, u64)
where
    E: BlockExecutor + 'static,
{
    let manifest = GuestManifest {
        name: "pathn-sh".to_string(),
        version: 1,
        load_addr: RAM_BASE,
    };
    let (img, _sbom) = build(&manifest);
    let t0 = Instant::now();
    let mut orch = Orchestrator::with_executor(Box::new(make()));
    orch.load_image(&img).expect("load_image");
    match orch.run_until_halt(10_000) {
        HaltReason::Wfi { addr } if addr == BOOT_WFI_ADDR => {}
        other => panic!("boot did not park at WFI: {other:?}"),
    }
    assert_eq!(
        &orch.console().tx_bytes,
        BOOT_PROMPT,
        "boot prompt mismatch"
    );
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    (ms, orch.steps())
}

/// Step until the guest parks at WFI (or a real halt), capped.
fn settle(orch: &mut Orchestrator) {
    for _ in 0..2_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(_) => break,
            StepOutcome::WfiYield { .. } => break,
            StepOutcome::Continue => {}
        }
    }
}

fn bench_backend<E>(name: &str, make: impl Fn() -> E + Copy)
where
    E: BlockExecutor + 'static,
{
    // 1. Cold boot (median of 5).
    let boots: Vec<f64> = (0..5).map(|_| boot_with(make).0).collect();
    let boot_ms = median(boots);

    // 2. Scripted workload through the real adapter path.
    let manifest = GuestManifest {
        name: "pathn-sh".to_string(),
        version: 1,
        load_addr: RAM_BASE,
    };
    let (img, _sbom) = build(&manifest);
    let t0 = Instant::now();
    let mut orch = Orchestrator::with_executor(Box::new(make()));
    orch.load_image(&img).expect("load_image");
    assert!(matches!(
        orch.run_until_halt(10_000),
        HaltReason::Wfi { .. }
    ));
    let mut kbd = KeyboardInput::new();
    for cmd in COMMANDS {
        type_text(&mut kbd, cmd);
        orch.pump_input(&mut kbd);
        settle(&mut orch);
    }
    let workload_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let steps = orch.steps();
    let steps_per_sec = steps as f64 / (workload_ms / 1000.0);
    let tx = String::from_utf8_lossy(&orch.console().tx_bytes).into_owned();
    assert!(tx.contains("commands: echo <args> | help"), "help output");
    assert!(tx.contains("\nhi\n"), "echo output");
    assert!(tx.contains("unknown cmd: bogus"), "bogus output");

    // 3. Input latency: push_event -> echoed bytes in TX (median of 5).
    let mut lats = Vec::new();
    for _ in 0..5 {
        let (img2, _) = build(&manifest);
        let mut o = Orchestrator::with_executor(Box::new(make()));
        o.load_image(&img2).expect("load_image");
        assert!(matches!(o.run_until_halt(10_000), HaltReason::Wfi { .. }));
        let mut k = KeyboardInput::new();
        let t = Instant::now();
        type_text(&mut k, "z");
        o.pump_input(&mut k);
        for _ in 0..2_000_000 {
            match o.step_vcpu() {
                StepOutcome::Halted(_) | StepOutcome::WfiYield { .. } => break,
                StepOutcome::Continue => {}
            }
            if o.console().tx_bytes.ends_with(b"z") {
                break;
            }
        }
        lats.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(o.console().tx_bytes.ends_with(b"z"), "echo did not arrive");
    }

    println!("backend: {name}");
    println!("  boot_to_prompt_ms (median of 5): {boot_ms:.1}");
    println!("  workload_ms (boot + 3 commands): {workload_ms:.1}");
    println!("  workload_steps: {steps}");
    println!("  steps_per_sec: {steps_per_sec:.0}");
    println!("  input_latency_ms (median of 5): {:.2}", median(lats));
}

fn main() {
    println!(
        "== Path N platform bench (native, {}) ==",
        std::env::consts::ARCH
    );
    bench_backend("wasmi", WasmiExecutor::new);
    bench_backend("wasmtime", WasmtimeExecutor::new);
}
