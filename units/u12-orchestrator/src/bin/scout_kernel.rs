//! Automated Scout Runner for Path N AArch64 Emulator (Track A2).
//!
//! Maps next 20-50 boot halts by recording PC, opcode, reason, and force-skipping
//! blockers (NOP faulting instruction, stub device/memory, force return).
//! Outputs markdown report to /Users/Shared/track-a2-scout.md.

use std::env;
use std::fs;
use std::path::Path;
use pathn_contracts::cpu::InsnKind;
use u12_orchestrator::{
    best_effort_classify, HaltReason, Orchestrator, StepOutcome, RAM_BASE,
    TIMER_CYCLES_PER_STEP,
};

#[derive(Debug, Clone)]
pub struct HaltRecord {
    pub id: usize,
    pub step: u64,
    pub pc: u64,
    pub word: Option<u32>,
    pub mnemonic: String,
    pub kind: String,
    pub reason: String,
    pub skip_action: String,
    pub regs: [u64; 31],
    pub sp: u64,
    pub pstate: u64,
    pub context_before: Vec<(u64, u32)>,
    pub context_after: Vec<(u64, u32)>,
    pub console_snapshot: String,
}

fn print_help() {
    println!("Usage: scout_kernel [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --kernel <PATH>       Path to ARM64 kernel Image");
    println!("  --dtb <PATH>          Path to device tree blob");
    println!("  --max-steps <N>       Step budget cap (default: 200000000)");
    println!("  --target-halts <N>    Stop after discovering N halts (default: 30)");
    println!("  --output <PATH>       Output markdown path (default: /Users/Shared/track-a2-scout.md)");
    println!("  --help, -h            Print this help text");
}

fn main() {
    let mut kernel_path = "/Users/Shared/androidwebgpu/androidwebgpu/guest-image/Image".to_string();
    let mut dtb_path = "guest-image/minimal-virt.dtb".to_string();
    let mut max_steps: u64 = 200_000_000;
    let mut target_halts: usize = 30;
    let mut output_path = "/Users/Shared/track-a2-scout.md".to_string();

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
                    dtb_path = args[i].clone();
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
            "--target-halts" => {
                i += 1;
                if i < args.len() {
                    if let Ok(val) = args[i].parse() {
                        target_halts = val;
                    }
                }
            }
            "--output" => {
                i += 1;
                if i < args.len() {
                    output_path = args[i].clone();
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

    if !Path::new(&kernel_path).exists() {
        eprintln!("Error: kernel image not found at {kernel_path}");
        std::process::exit(1);
    }
    if !Path::new(&dtb_path).exists() {
        eprintln!("Error: DTB not found at {dtb_path}");
        std::process::exit(1);
    }

    let kernel_bytes = fs::read(&kernel_path).expect("failed to read kernel image");
    let dtb_bytes = fs::read(&dtb_path).expect("failed to read DTB");

    let mut orch = Orchestrator::new();

    // Setup memory and CPU
    const TEXT_OFFSET: u64 = 0x80000;
    const KERNEL_LOAD_ADDR: u64 = RAM_BASE + TEXT_OFFSET;
    {
        let ram = &mut orch.machine_mut().ram;
        let load_len = kernel_bytes.len().min(ram.len() - TEXT_OFFSET as usize);
        let k_off = TEXT_OFFSET as usize;
        ram[k_off..k_off + load_len].copy_from_slice(&kernel_bytes[..load_len]);

        const DTB_LOAD_ADDR: u64 = RAM_BASE + 0x0800_0000;
        let d_off = (DTB_LOAD_ADDR - RAM_BASE) as usize;
        assert!(d_off + dtb_bytes.len() <= ram.len(), "DTB does not fit in RAM");
        ram[d_off..d_off + dtb_bytes.len()].copy_from_slice(&dtb_bytes);

        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = KERNEL_LOAD_ADDR;
        cpu.sp = 0;
        cpu.regs = [0; 31];
        cpu.regs[0] = DTB_LOAD_ADDR;
        cpu.regs[4] = KERNEL_LOAD_ADDR;
        cpu.pstate = 0x4000_0000; // Z flag
    }

    println!("[scout] Loaded kernel ({} bytes) and DTB ({} bytes)", kernel_bytes.len(), dtb_bytes.len());
    println!("[scout] Running discovery scout up to {max_steps} steps or {target_halts} halts...");
    println!("[scout] Output target: {output_path}");

    let mut halts: Vec<HaltRecord> = Vec::new();
    let mut last_halt_pc: u64 = 0;
    let mut consecutive_same_pc: usize = 0;
    let mut spin_count: usize = 0;
    let mut last_step_pc: u64 = 0;

    while orch.steps < max_steps && halts.len() < target_halts {
        let current_pc = orch.machine().cpu[0].pc;

        // Detect single-instruction spin loop (e.g., b ., wfe loop, or spinning on flag)
        if current_pc == last_step_pc {
            spin_count += 1;
        } else {
            spin_count = 0;
            last_step_pc = current_pc;
        }

        let mut triggered_halt = None;

        if spin_count >= 500 {
            // Spin loop detected!
            let reason = format!("SpinLoop: PC stuck at {current_pc:#018x} for 500 consecutive steps");
            triggered_halt = Some(HaltReason::Unsupported {
                addr: current_pc,
                reason: "SpinLoop detected (stuck loop)",
            });
            spin_count = 0;
        } else {
            match orch.step_vcpu() {
                StepOutcome::Continue => {}
                StepOutcome::WfiYield { addr } => {
                    triggered_halt = Some(HaltReason::Wfi { addr });
                }
                StepOutcome::Halted(reason) => {
                    triggered_halt = Some(reason);
                }
            }
        }

        if orch.steps > 0 && orch.steps % 10_000_000 == 0 {
            let pc = orch.machine().cpu[0].pc;
            println!("[progress] step {} ({}M), PC={:#018x}, halts discovered: {}",
                     orch.steps, orch.steps / 1_000_000, pc, halts.len());
        }

        if let Some(reason) = triggered_halt {
            let pc = orch.machine().cpu[0].pc;
            let step = orch.steps;

            if pc == last_halt_pc {
                consecutive_same_pc += 1;
            } else {
                consecutive_same_pc = 1;
                last_halt_pc = pc;
            }

            // Fetch instruction word and context instructions
            let mut word_opt = None;
            let mut context_before = Vec::new();
            let mut context_after = Vec::new();

            for (addr, res) in orch.debug_fetch_around(pc.wrapping_sub(16), 9) {
                if let Ok(w) = res {
                    if addr < pc {
                        context_before.push((addr, w));
                    } else if addr == pc {
                        word_opt = Some(w);
                    } else {
                        context_after.push((addr, w));
                    }
                }
            }

            let (kind_enum, mnemonic) = if let Some(w) = word_opt {
                let (k, m) = best_effort_classify(w);
                (format!("{k:?}"), m.to_string())
            } else {
                ("Unknown".to_string(), "Unfetchable".to_string())
            };

            let cpu = &orch.machine().cpu[0];
            let regs = cpu.regs;
            let sp = cpu.sp;
            let pstate = cpu.pstate;
            let lr = cpu.regs[30];

            let console_bytes = &orch.console().tx_bytes;
            let console_str = if let Ok(s) = std::str::from_utf8(console_bytes) {
                let start = s.len().saturating_sub(512);
                s[start..].to_string()
            } else {
                String::new()
            };

            // Determine and apply force-skip action
            let skip_action: String;

            if consecutive_same_pc >= 2 {
                // If stuck on the exact same PC for 2 halts, escape immediately via LR or advance PC + 8
                if lr >= 0xffffff8008000000 && lr != pc {
                    orch.machine_mut().cpu[0].pc = lr;
                    skip_action = format!("Repeated halt at {pc:#018x} (halted {consecutive_same_pc}x): force-return via LR={lr:#018x}");
                } else {
                    orch.machine_mut().cpu[0].pc = pc.wrapping_add(8);
                    skip_action = format!("Repeated halt at {pc:#018x} (halted {consecutive_same_pc}x): skip 8 bytes to {:#018x}", pc.wrapping_add(8));
                }
            } else {
                match &reason {
                    HaltReason::Unsupported { addr: _, reason: r } => {
                        if let Some(w) = word_opt {
                            let rd = (w & 0x1F) as usize;
                            if rd != 31 {
                                orch.machine_mut().cpu[0].regs[rd] = 0;
                            }
                            // If this is an ADDS/SUBS instruction, set Z=1 and C=1
                            if (w >> 29) & 0b11 == 0b01 || (w >> 29) & 0b11 == 0b11 {
                                orch.machine_mut().cpu[0].pstate =
                                    (orch.machine().cpu[0].pstate & !0xF000_0000) | 0x6000_0000;
                            }
                        }
                        orch.machine_mut().cpu[0].pc = pc.wrapping_add(4);
                        skip_action = format!("NOP unsupported instruction '{mnemonic}' ({r}), advance PC to {:#018x}", pc.wrapping_add(4));
                    }
                    HaltReason::IllegalInstruction { addr: _, word: w } => {
                        orch.machine_mut().cpu[0].pc = pc.wrapping_add(4);
                        skip_action = format!("NOP illegal opcode {w:#010x}, advance PC to {:#018x}", pc.wrapping_add(4));
                    }
                    HaltReason::WasmTrap { addr: _, message } => {
                        if let Some(w) = word_opt {
                            // Exclusive store: STXR / STLXR (size 001000 ... L=0 ... Rs)
                            // Bit 29:24 == 001000 and bit 22 == 0. Status reg is bits [20:16]
                            if (w >> 24) & 0x3F == 0b001000 && (w >> 22) & 1 == 0 {
                                let ws = ((w >> 16) & 0x1F) as usize;
                                if ws != 31 {
                                    orch.machine_mut().cpu[0].regs[ws] = 0; // 0 = exclusive store SUCCESS
                                }
                                skip_action = format!("WasmTrap on exclusive store ({message}): set W{ws}=0 (success), advance PC to {:#018x}", pc.wrapping_add(4));
                            } else {
                                let rt = (w & 0x1F) as usize;
                                if rt != 31 {
                                    orch.machine_mut().cpu[0].regs[rt] = 0;
                                }
                                // Pair load (LDP)?
                                if (w >> 25) & 0b111 == 0b101 {
                                    let rt2 = ((w >> 10) & 0x1F) as usize;
                                    if rt2 != 31 {
                                        orch.machine_mut().cpu[0].regs[rt2] = 0;
                                    }
                                }
                                skip_action = format!("WasmTrap ({message}): stub dest reg X{rt}=0, advance PC to {:#018x}", pc.wrapping_add(4));
                            }
                        } else {
                            skip_action = format!("WasmTrap ({message}): advance PC to {:#018x}", pc.wrapping_add(4));
                        }
                        orch.machine_mut().cpu[0].pc = pc.wrapping_add(4);
                    }
                    HaltReason::FetchFault { addr } => {
                        if *addr < 0xffffff8008000000 {
                            if lr >= 0xffffff8008000000 {
                                orch.machine_mut().cpu[0].pc = lr;
                                skip_action = format!("FetchFault at invalid {addr:#018x}: force-return via LR={lr:#018x}");
                            } else {
                                orch.machine_mut().cpu[0].pc = pc.wrapping_add(4);
                                skip_action = format!("FetchFault at {addr:#018x}: advance PC to {:#018x}", pc.wrapping_add(4));
                            }
                        } else {
                            if lr >= 0xffffff8008000000 && lr != pc {
                                orch.machine_mut().cpu[0].pc = lr;
                                skip_action = format!("FetchFault at unmapped {addr:#018x}: force-return via LR={lr:#018x}");
                            } else {
                                orch.machine_mut().cpu[0].pc = pc.wrapping_add(4);
                                skip_action = format!("FetchFault at {addr:#018x}: advance PC to {:#018x}", pc.wrapping_add(4));
                            }
                        }
                    }
                    HaltReason::Wfi { addr } => {
                        orch.machine_mut().cpu[0].pc = addr.wrapping_add(4);
                        orch.tick_clock(TIMER_CYCLES_PER_STEP * 10);
                        skip_action = format!("WFI at {addr:#018x}: wake up, advance PC to {:#018x}", addr.wrapping_add(4));
                    }
                    HaltReason::Svc { addr } => {
                        orch.machine_mut().cpu[0].pc = addr.wrapping_add(4);
                        orch.machine_mut().cpu[0].regs[0] = 0; // Success
                        skip_action = format!("SVC at {addr:#018x}: stub return 0 in X0, advance PC to {:#018x}", addr.wrapping_add(4));
                    }
                    HaltReason::ExitVm => {
                        if lr >= 0xffffff8008000000 {
                            orch.machine_mut().cpu[0].pc = lr;
                            skip_action = format!("ExitVm: force-return via LR={lr:#018x}");
                        } else {
                            orch.machine_mut().cpu[0].pc = pc.wrapping_add(4);
                            skip_action = format!("ExitVm: advance PC to {:#018x}", pc.wrapping_add(4));
                        }
                    }
                    HaltReason::StepLimitExceeded => {
                        break;
                    }
                }
            }

            // Unpoison orchestrator state
            orch.halted = None;
            orch.steps += 1;
            orch.tick_clock(TIMER_CYCLES_PER_STEP);

            let record = HaltRecord {
                id: halts.len() + 1,
                step,
                pc,
                word: word_opt,
                mnemonic,
                kind: kind_enum,
                reason: format!("{reason:?}"),
                skip_action: skip_action.clone(),
                regs,
                sp,
                pstate,
                context_before,
                context_after,
                console_snapshot: console_str,
            };

            let word_hex = word_opt.map(|w| format!("{w:#010x}")).unwrap_or_else(|| "N/A".to_string());
            println!(
                "==> [HALT #{:02}] Step {:10} | PC={:#018x} | Op={:10} | Kind={:12} | {}",
                record.id, record.step, record.pc, word_hex, record.mnemonic, record.reason
            );
            println!("    Action: {}", record.skip_action);

            halts.push(record);
        }
    }

    println!();
    println!("[scout] Discovery run completed!");
    println!("[scout] Total steps: {}, Total halts mapped: {}", orch.steps, halts.len());

    // Generate Markdown Report
    generate_markdown_report(&output_path, &halts, orch.steps, &orch);
    println!("[scout] Report successfully written to {}", output_path);
}

fn generate_markdown_report(
    path: &str,
    halts: &[HaltRecord],
    total_steps: u64,
    orch: &Orchestrator,
) {
    use std::fmt::Write;
    let mut out = String::new();

    writeln!(out, "# Track A2 - Halt Discovery Scout Report").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "**Generated:** 2026-10-05").unwrap();
    writeln!(out, "**Repository:** `/Users/Shared/wt-track-a2` (branch: `scout/track-a2-discovery`)").unwrap();
    writeln!(out, "**Baseline Tip:** `636808d` (Fixes vmemmap L1 descriptor gap)").unwrap();
    writeln!(out, "**Target Output:** `{path}`").unwrap();
    writeln!(out, "**Total Steps Executed:** {total_steps}").unwrap();
    writeln!(out, "**Total Halts Discovered & Mapped:** {}", halts.len()).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "---").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## 1. Executive Summary & Strategic Roadmap for Track A1").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "Track A2 operated in **scout-ahead discovery mode** to explore the path beyond the 20M step milestone. By force-skipping halting conditions (NOPing unsupported opcodes, stubbing trapped loads with 0, and safely resuming execution), we mapped **{} distinct halts** across the boot process.", halts.len()).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "### Key Findings & Subsystem Categories:").unwrap();

    let mut dataproc_count = 0;
    let mut trap_count = 0;
    let mut fetch_count = 0;
    let mut wfi_count = 0;
    let mut other_count = 0;

    for h in halts {
        if h.reason.contains("DataProc") || h.kind == "DataProc" {
            dataproc_count += 1;
        } else if h.reason.contains("WasmTrap") {
            trap_count += 1;
        } else if h.reason.contains("FetchFault") {
            fetch_count += 1;
        } else if h.reason.contains("Wfi") {
            wfi_count += 1;
        } else {
            other_count += 1;
        }
    }

    writeln!(out, "- **Data Processing / Arithmetic Opcodes ({} halts):** Missing instruction forms in `u2-ir-lift` and `execute_arm64`, such as `adds/subs` with extended register, sub-word operations, bitfield extractions.", dataproc_count).unwrap();
    writeln!(out, "- **Memory Access Traps ({} halts):** Unmapped peripheral MMIO, device driver probes, or vmemmap/page table traversals.", trap_count).unwrap();
    writeln!(out, "- **Fetch Faults ({} halts):** Function pointers to unmapped driver stubs or indirect branches.", fetch_count).unwrap();
    writeln!(out, "- **WFI / Synchronization ({} halts):** Idle loops or wait-for-interrupt conditions requiring interrupt delivery.", wfi_count).unwrap();
    writeln!(out, "- **Other / System ({} halts):** Sysregs, barriers, or special exception handling.", other_count).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "---").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## 2. Ordered Halts Master Table").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| # | Step | PC | Opcode | Classification | Halt Reason | Force-Skip Action | Priority for A1 |").unwrap();
    writeln!(out, "|---|------|----|--------|----------------|-------------|-------------------|-----------------|").unwrap();

    for h in halts {
        let op_hex = h.word.map(|w| format!("`{w:#010x}`")).unwrap_or_else(|| "`N/A`".to_string());
        let priority = if h.id <= 5 { "P0 - Immediate" } else if h.id <= 15 { "P1 - High" } else { "P2 - Medium" };
        let reason_short = if h.reason.len() > 40 {
            format!("{}...", &h.reason[..37])
        } else {
            h.reason.clone()
        };
        let action_short = if h.skip_action.len() > 45 {
            format!("{}...", &h.skip_action[..42])
        } else {
            h.skip_action.clone()
        };
        writeln!(
            out,
            "| {:02} | {} | `{:#018x}` | {} | {} ({}) | {} | {} | {} |",
            h.id, h.step, h.pc, op_hex, h.mnemonic, h.kind, reason_short, action_short, priority
        ).unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "---").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## 3. Detailed Halt Analysis & Fix Recommendations").unwrap();
    writeln!(out).unwrap();

    for h in halts {
        writeln!(out, "### Halt #{:02}: Step {} at PC `{:#018x}`", h.id, h.step, h.pc).unwrap();
        writeln!(out).unwrap();
        let op_hex = h.word.map(|w| format!("{w:#010x}")).unwrap_or_else(|| "N/A".to_string());
        writeln!(out, "- **Instruction:** `{op_hex}` ({})", h.mnemonic).unwrap();
        writeln!(out, "- **Kind:** `{}`", h.kind).unwrap();
        writeln!(out, "- **Halt Reason:** `{}`", h.reason).unwrap();
        writeln!(out, "- **Force-Skip Action:** `{}`", h.skip_action).unwrap();
        writeln!(out).unwrap();

        writeln!(out, "**Context Disassembly:**").unwrap();
        writeln!(out, "```text").unwrap();
        for (addr, w) in &h.context_before {
            let (_, m) = best_effort_classify(*w);
            writeln!(out, "  {addr:#018x}: {w:#010x}  {m}").unwrap();
        }
        if let Some(w) = h.word {
            let (_, m) = best_effort_classify(w);
            writeln!(out, "=> {:#018x}: {w:#010x}  {m}  <-- FAULT", h.pc).unwrap();
        }
        for (addr, w) in &h.context_after {
            let (_, m) = best_effort_classify(*w);
            writeln!(out, "  {addr:#018x}: {w:#010x}  {m}").unwrap();
        }
        writeln!(out, "```").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "**CPU Register State at Halt:**").unwrap();
        writeln!(out, "```text").unwrap();
        for r in 0..31 {
            write!(out, "x{:02}={:#018x} ", r, h.regs[r]).unwrap();
            if (r + 1) % 4 == 0 {
                writeln!(out).unwrap();
            }
        }
        writeln!(out, "\nsp={:#018x} pstate={:#010x}", h.sp, h.pstate).unwrap();
        writeln!(out, "```").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "**Recommended Permanent Fix for Track A1:**").unwrap();
        if h.reason.contains("DataProc") || h.kind == "DataProc" {
            writeln!(out, "1. Implement lifting in `units/u2-ir-lift` or fast-path execution in `units/u12-orchestrator/src/lib.rs` for `{}` opcode `{op_hex}`.", h.mnemonic).unwrap();
            writeln!(out, "2. Add test vector in `units/u12-orchestrator/tests/` to verify flag calculation and 64-bit / 32-bit register extensions.").unwrap();
        } else if h.reason.contains("WasmTrap") {
            writeln!(out, "1. Check target physical address in page table walk; identify if access is to guest RAM, DTB, or MMIO window.").unwrap();
            writeln!(out, "2. If peripheral MMIO, register stub device window or handle reads/writes with truthful response.").unwrap();
        } else if h.reason.contains("FetchFault") {
            writeln!(out, "1. Verify indirect branch target calculation and MMU page table entry flags (UXN/PXN).").unwrap();
        } else if h.reason.contains("Wfi") {
            writeln!(out, "1. Ensure U5 GIC / timer model ticks properly and injects PPI 27 on timer compare match.").unwrap();
        } else {
            writeln!(out, "1. Inspect kernel caller and provide proper emulator support.").unwrap();
        }
        writeln!(out).unwrap();
        writeln!(out, "---").unwrap();
        writeln!(out).unwrap();
    }

    // Guest Console Output
    let console = &orch.console().tx_bytes;
    if !console.is_empty() {
        writeln!(out, "## 4. Guest Console / Early Printk Output Captured").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "```text").unwrap();
        if let Ok(s) = std::str::from_utf8(console) {
            writeln!(out, "{s}").unwrap();
        } else {
            writeln!(out, "<{} bytes of non-utf8 data>", console.len()).unwrap();
        }
        writeln!(out, "```").unwrap();
        writeln!(out).unwrap();
    }

    fs::write(path, out).expect("failed to write output markdown report");
}
