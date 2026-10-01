//! Kernel run entry point for Path N AArch64 emulator.
//!
//! Boots an AArch64 Linux kernel image (default: /mnt/sdb1/aosp/Image) under
//! U12 Orchestrator and executes instructions until halt or max-steps limit.

use std::env;
use std::fs;
use std::path::Path;
use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};

fn print_help() {
    println!("Usage: run_kernel [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --kernel <PATH>       Path to ARM64 kernel Image (default: /mnt/sdb1/aosp/Image)");
    println!("  --max-steps <N>       Step budget cap (default: 2000000)");
    println!("  --trace <FILE>        Enable per-step execution tracing to file");
    println!("  --survey              Enable survey mode (discovery only, never progress)");
    println!("  --help, -h            Print this help text");
}

fn main() {
    let mut kernel_path = "/mnt/sdb1/aosp/Image".to_string();
    let mut max_steps: u64 = 2_000_000;
    let mut trace_file: Option<String> = None;
    let mut survey = false;

    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--kernel" => {
                i += 1;
                if i < args.len() {
                    kernel_path = args[i].clone();
                }
            }
            "--max-steps" => {
                i += 1;
                if i < args.len() {
                    if let Ok(val) = args[i].parse() {
                        max_steps = val;
                    }
                }
            }
            "--trace" => {
                i += 1;
                if i < args.len() {
                    trace_file = Some(args[i].clone());
                }
            }
            "--survey" => {
                // HARD RULE: Survey mode is discovery only — it must NEVER be used
                // to claim boot progress or move any baseline.
                survey = true;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let path = Path::new(&kernel_path);
    if !path.exists() {
        eprintln!("Error: kernel image not found at {kernel_path}");
        std::process::exit(1);
    }

    let kernel_bytes = fs::read(path).expect("failed to read kernel image");

    let mut orch = Orchestrator::new();
    if let Some(ref tf) = trace_file {
        orch.enable_trace_file(tf)
            .expect("failed to open trace output file");
        println!("[trace] Writing per-step execution trace to {tf}");
    }
    if survey {
        // HARD RULE: Survey mode is discovery only — it must NEVER be used
        // to claim boot progress or move any baseline.
        orch.set_survey_mode(true);
        println!("[survey] Survey mode active: skipping unimplemented opcodes (discovery only)");
    }

    let ram = &mut orch.machine_mut().ram;
    let load_len = kernel_bytes.len().min(ram.len());
    ram[..load_len].copy_from_slice(&kernel_bytes[..load_len]);

    // Linux kernel entry point
    orch.machine_mut().cpu[0].pc = RAM_BASE;
    orch.machine_mut().cpu[0].sp = RAM_BASE + 0x0800_0000;
    orch.machine_mut().cpu[0].regs = [0; 31];

    println!(
        "[boot] Loaded {} bytes at {RAM_BASE:#x}. Running up to {max_steps} steps...",
        load_len
    );

    let mut halt_reason = None;
    for _ in 0..max_steps {
        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            StepOutcome::WfiYield { addr } => {
                halt_reason = Some(HaltReason::Wfi { addr });
                break;
            }
            StepOutcome::Halted(reason) => {
                halt_reason = Some(reason);
                break;
            }
        }
    }

    orch.flush_trace();

    let steps = orch.steps();
    let pc = orch.machine().cpu[0].pc;
    println!("[done] Executed {steps} steps. PC={pc:#018x}");
    match halt_reason {
        Some(reason) => println!("[halt] Reason: {reason:?}"),
        None => println!("[limit] Step budget ({max_steps}) reached without halt"),
    }

    if survey {
        orch.print_survey_summary();
    }
}
