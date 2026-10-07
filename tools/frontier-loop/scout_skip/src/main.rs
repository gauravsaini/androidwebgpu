use pathn_contracts::cpu::{Access, MmuState};
use pathn_contracts::machine::MachineState;
use std::env;
use std::fs;
use std::path::Path;
use u12_orchestrator::{HaltReason, Orchestrator};

const RAM_BASE: u64 = 0x4000_0000;
const NOP: u32 = 0xd503_201f;

#[derive(Default)]
struct Args {
    action: String,
    input: String,
    output: Option<String>,
    method: Option<String>,
    pc: Option<u64>,
    word: Option<u32>,
    value: Option<u64>,
    patch_pc: Option<u64>,
}

fn number(value: &str) -> Result<u64, String> {
    if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|error| error.to_string())
    } else {
        value.parse::<u64>().map_err(|error| error.to_string())
    }
}

fn parse_args() -> Result<Args, String> {
    let mut args = env::args().skip(1);
    let mut parsed = Args {
        action: args.next().ok_or("expected action: inspect or apply")?,
        input: args.next().ok_or("expected input snapshot path")?,
        ..Args::default()
    };
    while let Some(option) = args.next() {
        let value = args.next().ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--output" => parsed.output = Some(value),
            "--method" => parsed.method = Some(value),
            "--pc" => parsed.pc = Some(number(&value)?),
            "--word" => parsed.word = Some(number(&value)? as u32),
            "--value" => parsed.value = Some(number(&value)?),
            "--patch-pc" => parsed.patch_pc = Some(number(&value)?),
            _ => return Err(format!("unknown option: {option}")),
        }
    }
    Ok(parsed)
}

fn load_snapshot(path: &str) -> Result<Orchestrator, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {path}: {error}"))?;
    Orchestrator::restore_full(&bytes).map_err(|error| format!("restore snapshot: {error}"))
}

fn physical_address(machine: &MachineState, va: u64, access: Access) -> Result<u64, String> {
    let sysregs = &machine.cpu[0].sysregs;
    let mmu = MmuState {
        ttbr0: sysregs.ttbr0_el1,
        ttbr1: sysregs.ttbr1_el1,
        tcr: sysregs.tcr_el1,
        sctlr: sysregs.sctlr_el1,
    };
    u4_mmu::translate_with_base(&mmu, &machine.ram, RAM_BASE, va, access)
        .map_err(|fault| format!("translate {va:#x}: {fault:?}"))
}

fn ram_offset(machine: &MachineState, pa: u64, length: usize) -> Result<usize, String> {
    let offset = pa
        .checked_sub(RAM_BASE)
        .ok_or_else(|| format!("physical address {pa:#x} is below guest RAM"))? as usize;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| "guest RAM offset overflow".to_string())?;
    if end > machine.ram.len() {
        return Err(format!("physical address {pa:#x} is outside guest RAM"));
    }
    Ok(offset)
}

fn instruction_word(machine: &MachineState, pc: u64) -> Result<u32, String> {
    let pa = physical_address(machine, pc, Access::Execute)?;
    let offset = ram_offset(machine, pa, 4)?;
    Ok(u32::from_le_bytes(machine.ram[offset..offset + 4].try_into().unwrap()))
}

fn size_bytes(word: u32) -> usize {
    1usize << ((word >> 30) & 0x3)
}

fn base_register(machine: &MachineState, reg: u32) -> u64 {
    let cpu = &machine.cpu[0];
    if reg == 31 {
        cpu.sp
    } else {
        cpu.regs[reg as usize]
    }
}

fn effective_address(machine: &MachineState, word: u32) -> Result<(u64, usize, bool), String> {
    // Unsigned-immediate single-register loads/stores (the common device-register form).
    if word & 0x3b00_0000 == 0x3900_0000 {
        let size = size_bytes(word);
        let immediate = (((word >> 10) & 0xfff) as u64) * size as u64;
        let base = base_register(machine, (word >> 5) & 0x1f);
        let opc = (word >> 22) & 0x3;
        let is_load = opc != 0;
        return Ok((base.wrapping_add(immediate), size, is_load));
    }

    // Unscaled, pre-index, and post-index immediate single-register forms.
    if word & 0x3b20_0000 == 0x3800_0000 {
        let size = size_bytes(word);
        let raw = ((word >> 12) & 0x1ff) as i64;
        let offset = if raw & 0x100 != 0 { raw - 0x200 } else { raw };
        let mode = (word >> 10) & 0x3;
        let base_reg = (word >> 5) & 0x1f;
        let base = base_register(machine, base_reg);
        let address = if mode == 1 { base } else { base.wrapping_add_signed(offset) };
        let opc = (word >> 22) & 0x3;
        return Ok((address, size, opc != 0));
    }

    Err(format!("unsupported load/store encoding {word:#010x}"))
}

fn write_snapshot(path: &str, orchestrator: &Orchestrator) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create output directory: {error}"))?;
    }
    fs::write(path, orchestrator.snapshot_full()).map_err(|error| format!("write {path}: {error}"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("scout-skip: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut orchestrator = load_snapshot(&args.input)?;
    let pc = args.pc.unwrap_or(orchestrator.machine().cpu[0].pc);
    let live_pc = orchestrator.machine().cpu[0].pc;
    let word = args.word.unwrap_or(instruction_word(orchestrator.machine(), pc)?);
    if args.action != "inspect" && args.action != "apply" {
        return Err(format!("unsupported action {:?}", args.action));
    }

    let mut details = format!(
        "{{\"snapshot_step\":{},\"pc\":\"0x{pc:016x}\",\"word\":\"0x{word:08x}\",\"live_pc\":\"0x{live_pc:016x}\",\"pending_irqs\":{:?}",
        orchestrator.steps(),
        orchestrator.pending_irqs()
    );
    let cpu = &orchestrator.machine().cpu[0];
    details.push_str(&format!(
        ",\"pstate\":\"0x{:016x}\",\"vbar_el1\":\"0x{:016x}\",\"elr_el1\":\"0x{:016x}\",\"spsr_el1\":\"0x{:016x}\"",
        cpu.pstate, cpu.sysregs.vbar_el1, cpu.sysregs.elr_el1, cpu.sysregs.spsr_el1
    ));
    if matches!(word & 0x3b00_0000, 0x3900_0000) || word & 0x3b20_0000 == 0x3800_0000 {
        let (va, size, is_load) = effective_address(orchestrator.machine(), word)?;
        let access = if is_load { Access::Read } else { Access::Write };
        let pa = physical_address(orchestrator.machine(), va, access)?;
        details.push_str(&format!(
            ",\"effective_address\":\"0x{va:016x}\",\"physical_address\":\"0x{pa:016x}\",\"access_size\":{size},\"is_load\":{is_load}"
        ));
    }

    if args.action == "inspect" {
        details.push('}');
        println!("{details}");
        return Ok(());
    }

    let output = args.output.as_deref().ok_or("apply requires --output")?;
    let method = args.method.as_deref().ok_or("apply requires --method")?;
    match method {
        "mmio-read" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            let loaded = args.value.ok_or("mmio-read requires --value")?;
            let rt = (word & 0x1f) as usize;
            let size = size_bytes(word);
            let opc = (word >> 22) & 0x3;
            let destination_width = match opc {
                1 => if size == 8 { 64 } else { 32 },
                2 => 32,
                3 => 64,
                _ => return Err(format!("faulting instruction is not a load: {word:#010x}")),
            };
            let value = if opc == 1 || size == 8 {
                loaded
            } else {
                let bits = size * 8;
                let mask = (1u64 << bits) - 1;
                let narrowed = loaded & mask;
                if narrowed & (1u64 << (bits - 1)) != 0 {
                    narrowed | !mask
                } else {
                    narrowed
                }
            };
            if rt < 31 {
                orchestrator.machine_mut().cpu[0].regs[rt] = if destination_width == 32 {
                    value as u32 as u64
                } else {
                    value
                };
            }
            orchestrator.machine_mut().cpu[0].pc = pc.wrapping_add(4);
            orchestrator.tick_clock(1000);
            details.push_str(&format!(",\"substituted_value\":\"0x{loaded:016x}\",\"rt\":{rt},\"next_pc\":\"0x{:016x}\",\"clock_cycles_advanced\":1000", orchestrator.machine().cpu[0].pc));
        }
        "advance" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            orchestrator.machine_mut().cpu[0].pc = pc.wrapping_add(4);
            orchestrator.tick_clock(1000);
            details.push_str(&format!(",\"next_pc\":\"0x{:016x}\",\"clock_cycles_advanced\":1000", orchestrator.machine().cpu[0].pc));
        }
        "nop" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            let pa = physical_address(orchestrator.machine(), pc, Access::Execute)?;
            let offset = ram_offset(orchestrator.machine(), pa, 4)?;
            orchestrator.machine_mut().ram[offset..offset + 4].copy_from_slice(&NOP.to_le_bytes());
            details.push_str(&format!(",\"patched_pc\":\"0x{pc:016x}\",\"replacement\":\"0x{NOP:08x}\""));
        }
        "sysreg-zero" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            let rt = (word & 0x1f) as usize;
            if rt < 31 {
                orchestrator.machine_mut().cpu[0].regs[rt] = 0;
            }
            orchestrator.machine_mut().cpu[0].pc = pc.wrapping_add(4);
            orchestrator.tick_clock(1000);
            details.push_str(&format!(",\"sysreg_zero_rt\":{rt},\"next_pc\":\"0x{:016x}\",\"clock_cycles_advanced\":1000", orchestrator.machine().cpu[0].pc));
        }
        "eret-return" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match ERET PC {pc:#x}"));
            }
            if word != 0xd69f_03e0 {
                return Err(format!("eret-return requires ERET encoding 0xd69f03e0, got {word:#010x}"));
            }
            let cpu = &mut orchestrator.machine_mut().cpu[0];
            let restored_pc = cpu.sysregs.elr_el1;
            let restored_pstate = cpu.sysregs.spsr_el1;
            if restored_pc == 0 || restored_pstate & 0xf != 0x5 {
                return Err(format!("saved EL1 return state is invalid: ELR={restored_pc:#x} SPSR={restored_pstate:#x}"));
            }
            cpu.pc = restored_pc;
            cpu.pstate = restored_pstate;
            cpu.sysregs.daif = restored_pstate & 0x3c0;
            details.push_str(&format!(",\"imprecise_skip\":\"ERET return from saved ELR_EL1/SPSR_EL1\",\"eret_pc\":\"0x{pc:016x}\",\"restored_pc\":\"0x{restored_pc:016x}\",\"restored_pstate\":\"0x{restored_pstate:016x}\""));
        }
        "resume-step-limit" => {
            if !matches!(
                orchestrator.halted.as_ref(),
                None | Some(HaltReason::StepLimitExceeded)
            ) {
                return Err("resume-step-limit requires a runner step-budget checkpoint".into());
            }
            details.push_str(",\"imprecise_skip\":\"clear runner step-limit latch and continue\"");
        }
        "spin-break" => {
            let patch_pc = args.patch_pc.ok_or("spin-break requires --patch-pc")?;
            let patch_pa = physical_address(orchestrator.machine(), patch_pc, Access::Execute)?;
            let offset = ram_offset(orchestrator.machine(), patch_pa, 4)?;
            let original = u32::from_le_bytes(orchestrator.machine().ram[offset..offset + 4].try_into().unwrap());
            orchestrator.machine_mut().ram[offset..offset + 4].copy_from_slice(&NOP.to_le_bytes());
            details.push_str(&format!(",\"patched_pc\":\"0x{patch_pc:016x}\",\"original\":\"0x{original:08x}\",\"replacement\":\"0x{NOP:08x}\""));
        }
        "irq-vector" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match IRQ-entry PC {pc:#x}"));
            }
            let pending = orchestrator.pending_irqs();
            if !pending.contains(&27) {
                return Err(format!("INTID 27 is not pending; pending IRQs are {pending:?}"));
            }
            let cpu = &mut orchestrator.machine_mut().cpu[0];
            let old_pstate = cpu.pstate;
            if old_pstate & 0x80 != 0 {
                return Err(format!("IRQ is masked in PSTATE.DAIF; PSTATE={old_pstate:#x}"));
            }
            let source_mode = old_pstate & 0xf;
            // This runner does not currently preserve the EL mode bits at
            // kernel entry (observed as M=0 while executing kernel VAs). The
            // discovery-only skip supplies the EL1h mode QEMU/Linux expects;
            // this is explicitly recorded as an imprecise state repair.
            let saved_pstate = (old_pstate & !0xf) | 0x5;
            let vbar = cpu.sysregs.vbar_el1;
            if vbar & 0x7ff != 0 {
                return Err(format!("VBAR_EL1 is not 2 KiB aligned: {vbar:#x}"));
            }
            let vector = vbar.wrapping_add(0x280);
            cpu.sysregs.elr_el1 = pc;
            cpu.sysregs.spsr_el1 = saved_pstate;
            cpu.sysregs.daif |= 0x3c0;
            cpu.pstate = (saved_pstate & !0x3cf) | 0x3c5;
            cpu.pc = vector;
            details.push_str(&format!(",\"imprecise_skip\":\"timer IRQ vector entry\",\"intid\":27,\"source_mode\":\"0x{source_mode:x}\",\"synthetic_el1h\":{},\"vbar_el1\":\"0x{vbar:016x}\",\"vector_pc\":\"0x{vector:016x}\",\"elr_el1\":\"0x{pc:016x}\",\"spsr_el1\":\"0x{saved_pstate:016x}\",\"pstate_after\":\"0x{:016x}\"", source_mode != 0x5, cpu.pstate));
        }
        other => return Err(format!("unsupported skip method {other}")),
    }
    if orchestrator.halted.take().is_some() {
        details.push_str(",\"halt_latch_cleared\":true");
    }
    details.push('}');
    write_snapshot(output, &orchestrator)?;
    println!("{details}");
    Ok(())
}
