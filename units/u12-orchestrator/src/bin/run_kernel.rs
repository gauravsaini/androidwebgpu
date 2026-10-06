//! Kernel run entry point for Path N AArch64 emulator.
//!
//! Boots an AArch64 Linux kernel image (default: /mnt/sdb1/aosp/Image) under
//! U12 Orchestrator and executes instructions until halt or max-steps limit.

use std::env;
use std::fs;
use std::path::Path;
use u11_snapshot;
use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};

fn print_help() {
    println!("Usage: run_kernel [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --kernel <PATH>       Path to ARM64 kernel Image (default: /mnt/sdb1/aosp/Image)");
    println!("  --dtb <PATH>          Path to device tree blob (optional; x0 set to DTB address if given)");
    println!("  --initrd <PATH>       Path to initramfs CPIO (optional; loaded at 0x4800_0000 per Track B layout)");
    println!("  --max-steps <N>       Step budget cap (default: 2000000)");
    println!("  --trace <FILE>        Enable per-step execution tracing to file");
    println!("  --console-output <FILE>  Write all captured UART bytes to FILE");
    println!("  --survey              Enable survey mode (discovery only, never progress)");
    println!("  --save-snapshot <PATH>  Write emulator snapshot to PATH after the run");
    println!("  --save-at <STEP>     With --save-snapshot: save when step counter reaches STEP and exit");
    println!("                        (default: save at end of run)");
    println!("  --save-every <N>     With --save-snapshot: auto-save every N steps to PATH.<step>");
    println!("                        (e.g. --save-snapshot snap.bin --save-every 100000)");
    println!("                        Enables restart from pre-divergence snapshot instead of reset.");
    println!("  --load-snapshot <PATH>  Restore emulator state from PATH instead of fresh boot;");
    println!("                        --max-steps then counts from the snapshot's step count.");
    println!("                        Snapshot files store an 8-byte LE step count header.");
    println!("  --help, -h            Print this help text");
}

fn main() {
    println!(
        "BINARY_GIT_REV={}",
        option_env!("BINARY_GIT_REV").unwrap_or("unknown")
    );

    let mut kernel_path = "/mnt/sdb1/aosp/Image".to_string();
    let mut dtb_path: Option<String> = None;
    let mut initrd_path: Option<String> = None;
    let mut max_steps: u64 = 2_000_000;
    let mut trace_file: Option<String> = None;
    let mut console_output: Option<String> = None;
    let mut dump_around: Option<usize> = None;
    let mut dump_console = false;
    let mut survey = false;
    let mut save_snapshot: Option<String> = None;
    let mut save_at: Option<u64> = None;
    let mut save_every: Option<u64> = None;
    let mut load_snapshot: Option<String> = None;

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
            "--dtb" => {
                i += 1;
                if i < args.len() {
                    dtb_path = Some(args[i].clone());
                }
            }
            "--initrd" => {
                i += 1;
                if i < args.len() {
                    initrd_path = Some(args[i].clone());
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
            "--console-output" => {
                i += 1;
                if i < args.len() {
                    console_output = Some(args[i].clone());
                }
            }
            "--dump-around" => {
                i += 1;
                if i < args.len() {
                    dump_around = Some(args[i].parse().unwrap_or(8));
                }
            }
            "--dump-console" => {
                dump_console = true;
            }
            "--survey" => {
                // HARD RULE: Survey mode is discovery only — it must NEVER be used
                // to claim boot progress or move any baseline.
                survey = true;
            }
            "--save-snapshot" => {
                i += 1;
                if i < args.len() {
                    save_snapshot = Some(args[i].clone());
                }
            }
            "--save-at" => {
                i += 1;
                if i < args.len() {
                    if let Ok(val) = args[i].parse() {
                        save_at = Some(val);
                    }
                }
            }
            "--save-every" => {
                i += 1;
                if i < args.len() {
                    if let Ok(val) = args[i].parse() {
                        save_every = Some(val);
                    }
                }
            }
            "--load-snapshot" => {
                i += 1;
                if i < args.len() {
                    load_snapshot = Some(args[i].clone());
                }
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

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

    // Snapshot load takes precedence over fresh boot: the restored
    // MachineState already contains kernel, DTB, registers, RAM, etc.
    // File format: u64 LE step-count header followed by the u11-snapshot blob.
    if let Some(ref snap_path) = load_snapshot {
        let bytes = fs::read(snap_path).expect("failed to read snapshot file");
        if bytes.len() < 8 {
            eprintln!("Error: snapshot file too short (missing step-count header)");
            std::process::exit(1);
        }
        let saved_steps = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let state = u11_snapshot::restore(&bytes[8..]).expect("failed to restore snapshot");
        let pc = state.cpu[0].pc;
        *orch.machine_mut() = state;
        orch.steps = saved_steps;
        println!(
            "[snapshot] Loaded {} bytes from {snap_path} at step {saved_steps}, PC={pc:#018x}",
            bytes.len()
        );
        println!(
            "[boot] Resuming from snapshot. Running up to {max_steps} steps from step {}...",
            orch.steps()
        );
    } else {
        let path = Path::new(&kernel_path);
        if !path.exists() {
            eprintln!("Error: kernel image not found at {kernel_path}");
            std::process::exit(1);
        }

        let kernel_bytes = fs::read(path).expect("failed to read kernel image");

        // ARM64 Linux boot protocol (matches QEMU -machine virt):
        // - Kernel Image loaded at RAM_BASE + TEXT_OFFSET (0x80000)
        // - PC set to kernel entry (RAM_BASE + 0x80000)
        // - X0 = DTB physical address (if --dtb given)
        // - SP left as 0 (kernel sets up its own stack)
        const TEXT_OFFSET: u64 = 0x80000;
        const KERNEL_LOAD_ADDR: u64 = RAM_BASE + TEXT_OFFSET;
        let ram = &mut orch.machine_mut().ram;
        let load_len = kernel_bytes.len().min(ram.len() - TEXT_OFFSET as usize);
        let k_off = TEXT_OFFSET as usize;
        ram[k_off..k_off + load_len].copy_from_slice(&kernel_bytes[..load_len]);

        // Linux kernel entry point
        orch.machine_mut().cpu[0].pc = KERNEL_LOAD_ADDR;
        orch.machine_mut().cpu[0].sp = 0;
        orch.machine_mut().cpu[0].regs = [0; 31];
        // QEMU sets X4 to kernel load address and Z flag in PSTATE at boot.
        orch.machine_mut().cpu[0].regs[4] = KERNEL_LOAD_ADDR;
        orch.machine_mut().cpu[0].pstate = 0x4000_0000; // Z flag (bit 30)

        // Initrd (Shelf Job 2, 2026-10-05): Track B QEMU layout.
        // - Initramfs CPIO at 0x4800_0000 (INITRD_LOAD_ADDR)
        // - DTB at 0x4820_0000 (DTB_LOAD_ADDR, moved from 0x4800_0000)
        // - Kernel Image at 0x4008_0000 (unchanged)
        // Non-overlapping: kernel ends at 0x4168_1200, initrd is 6656 bytes,
        // DTB is 1501 bytes. All clear of each other and framebuffer (0x7F00_0000).
        const INITRD_LOAD_ADDR: u64 = RAM_BASE + 0x0800_0000; // 0x4800_0000
        const DTB_LOAD_ADDR: u64 = RAM_BASE + 0x0820_0000; // 0x4820_0000
        if let Some(ref initrd_p) = initrd_path {
            let initrd_bytes = fs::read(initrd_p).expect("failed to read initrd");
            let off = (INITRD_LOAD_ADDR - RAM_BASE) as usize;
            let ram = &mut orch.machine_mut().ram;
            assert!(
                off + initrd_bytes.len() <= ram.len(),
                "initrd does not fit in RAM"
            );
            // Verify no overlap with DTB region (DTB loads after initrd)
            assert!(
                INITRD_LOAD_ADDR + initrd_bytes.len() as u64 <= DTB_LOAD_ADDR,
                "initrd overlaps DTB region"
            );
            ram[off..off + initrd_bytes.len()].copy_from_slice(&initrd_bytes);
            println!(
                "[boot] Loaded initrd ({} bytes) at {INITRD_LOAD_ADDR:#x}",
                initrd_bytes.len()
            );
        }
        if let Some(ref dtb_p) = dtb_path {
            let dtb_bytes = fs::read(dtb_p).expect("failed to read DTB");
            let off = (DTB_LOAD_ADDR - RAM_BASE) as usize;
            let ram = &mut orch.machine_mut().ram;
            assert!(off + dtb_bytes.len() <= ram.len(), "DTB does not fit in RAM");
            ram[off..off + dtb_bytes.len()].copy_from_slice(&dtb_bytes);
            orch.machine_mut().cpu[0].regs[0] = DTB_LOAD_ADDR;
            println!(
                "[boot] Loaded DTB ({} bytes) at {DTB_LOAD_ADDR:#x}, x0 set",
                dtb_bytes.len()
            );
        }

        println!(
            "[boot] Loaded {} bytes at {KERNEL_LOAD_ADDR:#x}. Running up to {max_steps} steps...",
            load_len
        );
    }

    let mut halt_reason = None;
    let mut saved_at_step: Option<u64> = None;
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
        let s = orch.steps();
        if s > 0 && s % 10_000_000 == 0 {
            let pc = orch.machine().cpu[0].pc;
            println!("[progress] step {s} ({}M), PC={pc:#018x}", s / 1_000_000);
        }
        // --save-at: snapshot as soon as the step counter reaches the target.
        if let Some(target) = save_at {
            if orch.steps() >= target {
                saved_at_step = Some(target);
                break;
            }
        }
        // --save-every: periodic auto-save for fast restart from pre-divergence.
        if let (Some(ref base), Some(every)) = (&save_snapshot, save_every) {
            if every > 0 && orch.steps() > 0 && orch.steps() % every == 0 {
                let path = format!("{}.{}", base, orch.steps());
                let blob = u11_snapshot::snapshot(orch.machine()).0;
                let mut out = Vec::with_capacity(8 + blob.len());
                out.extend_from_slice(&orch.steps().to_le_bytes());
                out.extend_from_slice(&blob);
                if fs::write(&path, &out).is_ok() {
                    println!("[snapshot] Auto-saved {} bytes to {} at step {}",
                             out.len(), path, orch.steps());
                }
            }
        }
    }

    orch.flush_trace();

    if let Some(path) = console_output {
        fs::write(&path, &orch.console().tx_bytes)
            .unwrap_or_else(|error| panic!("failed to write UART output to {path}: {error}"));
    }

    // Snapshot save: u64 LE step-count header + u11-snapshot blob.
    if let Some(ref snap_path) = save_snapshot {
        let blob = u11_snapshot::snapshot(orch.machine()).0;
        let mut out = Vec::with_capacity(8 + blob.len());
        out.extend_from_slice(&orch.steps().to_le_bytes());
        out.extend_from_slice(&blob);
        fs::write(snap_path, &out).expect("failed to write snapshot file");
        println!(
            "[snapshot] Saved {} bytes to {snap_path} at step {}",
            out.len(),
            orch.steps()
        );
    }

    let steps = orch.steps();
    let pc = orch.machine().cpu[0].pc;
    if let Some(n) = dump_around {
        println!("[dump] {n} instructions around PC={pc:#018x}:");
        for (addr, word) in orch.debug_fetch_around(pc.wrapping_sub(16), n + 4) {
            match word {
                Ok(w) => println!("  {addr:#018x}: {w:#010x}"),
                Err(e) => println!("  {addr:#018x}: <fetch failed: {e}>"),
            }
        }
    }
    if dump_console {
        let bytes = &orch.console().tx_bytes;
        println!("[console] {} bytes:", bytes.len());
        if let Ok(s) = std::str::from_utf8(bytes) {
            // Print last 2KB to avoid flooding
            let start = s.len().saturating_sub(2048);
            println!("{}", &s[start..]);
        } else {
            println!("<non-UTF8 output>");
        }
    }
    println!("[done] Executed {steps} steps. PC={pc:#018x}");
    match halt_reason {
        Some(ref reason) => {
            println!("[halt] Reason: {reason:?}");
            println!("[cpu] Registers at halt:");
            for r in 0..31 {
                print!("  x{:02}={:#018x}", r, orch.machine().cpu[0].regs[r]);
                if (r + 1) % 4 == 0 {
                    println!();
                }
            }
            println!();
            println!("  sp={:#018x} pstate={:#010x}", orch.machine().cpu[0].sp, orch.machine().cpu[0].pstate);
            let sys = &orch.machine().cpu[0].sysregs;
            println!("  ttbr0_el1={:#018x} ttbr1_el1={:#018x} tcr_el1={:#018x} sctlr_el1={:#010x}",
                     sys.ttbr0_el1, sys.ttbr1_el1, sys.tcr_el1, sys.sctlr_el1);
        }
        None if saved_at_step.is_some() => println!(
            "[snapshot] Reached --save-at step {} (saved, exiting)",
            saved_at_step.unwrap()
        ),
        None => println!("[limit] Step budget ({max_steps}) reached without halt"),
    }

    // Mandatory RAM search per Panic Verification Rule
    let ram = &orch.machine().ram;
    let needles: &[(&str, &[u8])] = &[
        ("Failed to allocate", b"Failed to allocate"),
        ("Kernel panic - not syncing", b"Kernel panic - not syncing"),
    ];
    println!("[ram-search] Scanning 1 GiB guest RAM for panic strings...");
    for (name, needle) in needles {
        let mut count = 0;
        let mut idx = 0;
        while let Some(pos) = ram[idx..].windows(needle.len()).position(|w| w == *needle) {
            let pa = RAM_BASE + (idx + pos) as u64;
            let is_rodata = (0x4008_0000..0x4168_0000).contains(&pa);
            println!("  Found '{}' at PA {pa:#010x} (rodata={is_rodata})", name);
            if !is_rodata {
                count += 1;
            }
            idx += pos + needle.len();
        }
        if count == 0 {
            println!("  [CLEAN] No dynamic occurrences of '{}'", name);
        } else {
            println!("  [PANIC DETECTED] {} dynamic occurrences of '{}'", count, name);
        }
    }

    if survey {
        orch.print_survey_summary();
    }
}
