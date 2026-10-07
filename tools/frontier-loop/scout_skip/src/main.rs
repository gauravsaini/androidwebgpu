use pathn_contracts::cpu::{Access, MmuState};
use pathn_contracts::machine::{MachineState, Snapshot};
use std::env;
use std::fs;
use std::path::Path;
use u11_snapshot::{restore, snapshot};

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

fn load_snapshot(path: &str) -> Result<(u64, MachineState), String> {
    let bytes = fs::read(path).map_err(|error| format!("read {path}: {error}"))?;
    if bytes.len() < 8 {
        return Err("snapshot is missing its 8-byte step-count header".into());
    }
    let steps = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let machine = restore(&bytes[8..]).map_err(|error| format!("restore snapshot: {error:?}"))?;
    Ok((steps, machine))
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

fn write_snapshot(path: &str, steps: u64, machine: &MachineState) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create output directory: {error}"))?;
    }
    let blob: Snapshot = snapshot(machine);
    let mut bytes = Vec::with_capacity(8 + blob.0.len());
    bytes.extend_from_slice(&steps.to_le_bytes());
    bytes.extend_from_slice(&blob.0);
    fs::write(path, bytes).map_err(|error| format!("write {path}: {error}"))
}

fn tick_clock(machine: &mut MachineState) {
    const CYCLES_PER_STEP: u64 = 1000;
    const TIMER_ENABLE_BIT: u32 = 0;
    {
        let sysregs = &machine.cpu[0].sysregs;
        let irq = &mut machine.irq;
        if sysregs.cntp_ctl_el0 & 1 != 0 {
            irq.enabled |= 1u32 << TIMER_ENABLE_BIT;
        } else {
            irq.enabled &= !(1u32 << TIMER_ENABLE_BIT);
        }
        irq.timer_compare = sysregs.cntp_cval_el0;
    }
    machine.irq = u5_gic_timer::tick(&machine.irq, CYCLES_PER_STEP).0;
    machine.cpu[0].sysregs.cntpct_el0 = machine.irq.timer_count;
}

fn main() {
    if let Err(error) = run() {
        eprintln!("scout-skip: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let (mut steps, mut machine) = load_snapshot(&args.input)?;
    let pc = args.pc.unwrap_or(machine.cpu[0].pc);
    let live_pc = machine.cpu[0].pc;
    let word = args.word.unwrap_or(instruction_word(&machine, pc)?);
    if args.action != "inspect" && args.action != "apply" {
        return Err(format!("unsupported action {:?}", args.action));
    }

    let mut details = format!(
        "{{\"snapshot_step\":{steps},\"pc\":\"0x{pc:016x}\",\"word\":\"0x{word:08x}\",\"live_pc\":\"0x{live_pc:016x}\""
    );
    if matches!(word & 0x3b00_0000, 0x3900_0000) || word & 0x3b20_0000 == 0x3800_0000 {
        let (va, size, is_load) = effective_address(&machine, word)?;
        let access = if is_load { Access::Read } else { Access::Write };
        let pa = physical_address(&machine, va, access)?;
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
                machine.cpu[0].regs[rt] = if destination_width == 32 {
                    value as u32 as u64
                } else {
                    value
                };
            }
            machine.cpu[0].pc = pc.wrapping_add(4);
            steps = steps.wrapping_add(1);
            tick_clock(&mut machine);
            details.push_str(&format!(",\"substituted_value\":\"0x{loaded:016x}\",\"rt\":{rt},\"next_pc\":\"0x{:016x}\",\"clock_cycles_advanced\":1000", machine.cpu[0].pc));
        }
        "advance" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            machine.cpu[0].pc = pc.wrapping_add(4);
            steps = steps.wrapping_add(1);
            tick_clock(&mut machine);
            details.push_str(&format!(",\"next_pc\":\"0x{:016x}\",\"clock_cycles_advanced\":1000", machine.cpu[0].pc));
        }
        "nop" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            let pa = physical_address(&machine, pc, Access::Execute)?;
            let offset = ram_offset(&machine, pa, 4)?;
            machine.ram[offset..offset + 4].copy_from_slice(&NOP.to_le_bytes());
            details.push_str(&format!(",\"patched_pc\":\"0x{pc:016x}\",\"replacement\":\"0x{NOP:08x}\""));
        }
        "sysreg-zero" => {
            if live_pc != pc {
                return Err(format!("snapshot PC {live_pc:#x} does not match fault PC {pc:#x}"));
            }
            let rt = (word & 0x1f) as usize;
            if rt < 31 {
                machine.cpu[0].regs[rt] = 0;
            }
            machine.cpu[0].pc = pc.wrapping_add(4);
            steps = steps.wrapping_add(1);
            tick_clock(&mut machine);
            details.push_str(&format!(",\"sysreg_zero_rt\":{rt},\"next_pc\":\"0x{:016x}\",\"clock_cycles_advanced\":1000", machine.cpu[0].pc));
        }
        "spin-break" => {
            let patch_pc = args.patch_pc.ok_or("spin-break requires --patch-pc")?;
            let patch_pa = physical_address(&machine, patch_pc, Access::Execute)?;
            let offset = ram_offset(&machine, patch_pa, 4)?;
            let original = u32::from_le_bytes(machine.ram[offset..offset + 4].try_into().unwrap());
            machine.ram[offset..offset + 4].copy_from_slice(&NOP.to_le_bytes());
            details.push_str(&format!(",\"patched_pc\":\"0x{patch_pc:016x}\",\"original\":\"0x{original:08x}\",\"replacement\":\"0x{NOP:08x}\""));
        }
        other => return Err(format!("unsupported skip method {other}")),
    }
    details.push('}');
    write_snapshot(output, steps, &machine)?;
    println!("{details}");
    Ok(())
}
