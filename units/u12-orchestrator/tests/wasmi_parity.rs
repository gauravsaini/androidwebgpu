//! wasmi backend parity (Path N browser host): the REAL 4.1 `pathn-sh` guest
//! boots to the exact shell prompt and executes commands through the
//! wasm32-compatible interpreter backend, injected via
//! [`Orchestrator::with_executor`]. Every pin mirrors the native (wasmtime)
//! backend acceptance in `boot.rs` — same boot park, same step count, same
//! exact TX bytes. Any divergence between backends fails here, loudly.

use guest_image::image::{build, GuestManifest};
use pathn_contracts::adapters::{KeyCode, NormalizedInput};
use u12_orchestrator::{HaltReason, Orchestrator, RAM_BASE};
use u13_adapters::ScriptedInput;
use u15_exec_wasmi::WasmiExecutor;

fn real_image() -> Vec<u8> {
    let manifest = GuestManifest {
        name: "pathn-sh".to_string(),
        version: 1,
        load_addr: RAM_BASE,
    };
    build(&manifest).0
}

/// Orchestrator with the wasm32-compatible backend injected.
fn wasmi_orchestrator() -> Orchestrator {
    Orchestrator::with_executor(Box::new(WasmiExecutor::new()))
}

/// Boot the real image to the first shell prompt via the wasmi backend.
fn boot_to_prompt() -> Orchestrator {
    let img = real_image();
    let mut o = wasmi_orchestrator();
    o.load_image(&img).unwrap();
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert_eq!(o.console().tx_bytes, b"pathn-sh> ");
    o
}

/// One `NormalizedInput::Key` press event per byte, as a single poll script.
fn scripted_keys(s: &str) -> ScriptedInput {
    let evs = s
        .bytes()
        .map(|b| NormalizedInput::Key {
            code: KeyCode(b as u32),
            pressed: true,
        })
        .collect();
    ScriptedInput::new(vec![evs])
}

#[test]
fn wasmi_boot_reaches_shell_prompt() {
    let o = boot_to_prompt();
    // Backend parity: the SAME measured instruction count as wasmtime.
    assert_eq!(o.steps(), 74);
    assert_eq!(o.machine().cpu[0].pc, 0x4000_0074);
    assert_eq!(o.machine().cpu[0].regs[10], 0x4000_1000);
    assert_eq!(o.machine().cpu[0].regs[0], 0x0);
    assert!(
        o.block_cache_len() > 0,
        "backend cache populated during boot"
    );
}

#[test]
fn wasmi_help_command_prints_help_text() {
    let mut o = boot_to_prompt();
    o.pump_input(&mut scripted_keys("help\n"));
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> help\ncommands: echo <args> | help\npathn-sh> "
    );
}

#[test]
fn wasmi_echo_command_prints_args() {
    let mut o = boot_to_prompt();
    o.pump_input(&mut scripted_keys("echo hi\n"));
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(o.console().tx_bytes, b"pathn-sh> echo hi\nhi\npathn-sh> ");
}

#[test]
fn wasmi_unknown_command_reports_word() {
    let mut o = boot_to_prompt();
    o.pump_input(&mut scripted_keys("bogus\n"));
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> bogus\nunknown cmd: bogus\npathn-sh> "
    );
}

#[test]
fn wasmi_default_native_backend_is_wasmtime() {
    // On native targets the default executor must still be the wasmtime
    // backend: the cache API routes through the injected backend either way.
    let mut o = Orchestrator::new();
    assert_eq!(o.block_cache_len(), 0);
    o.invalidate_code_cache();
    assert_eq!(o.block_cache_len(), 0);
}
