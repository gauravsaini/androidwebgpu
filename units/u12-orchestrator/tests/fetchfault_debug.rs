//! Debug: FetchFault at VA 0xffffff8009456158, step 20,738,874.
//! Dumps MMU sysregs and manually walks the page table for the faulting VA.

use std::fs;
use u12_orchestrator::{Orchestrator, StepOutcome};

const KERNEL_PATH: &str = "/home/hatch/workspace/.cache-aosp/Image";
const DTB_PATH: &str = "/home/hatch/workspace/androidwebgpu/guest-image/minimal-virt.dtb";

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const RAM_BASE: u64 = 0x4000_0000;
const FLAG_Z: u64 = 0x4000_0000;

const FAULT_VA: u64 = 0xffffff80_0945_6158;

fn read_u64(ram: &[u8], pa: u64) -> Option<u64> {
    let off = pa.checked_sub(RAM_BASE)? as usize;
    if off + 8 > ram.len() {
        return None;
    }
    Some(u64::from_le_bytes(ram[off..off + 8].try_into().unwrap()))
}

#[test]
fn debug_fetchfault_walk() {
    let kernel_bytes = fs::read(KERNEL_PATH).expect("kernel Image not found");
    let dtb_bytes = fs::read(DTB_PATH).expect("DTB not found");
    let mut orch = Orchestrator::new();
    {
        let k_off = (KERNEL_LOAD_PA - RAM_BASE) as usize;
        orch.machine_mut().ram[k_off..k_off + kernel_bytes.len()].copy_from_slice(&kernel_bytes);
        let d_off = (DTB_PA - RAM_BASE) as usize;
        orch.machine_mut().ram[d_off..d_off + dtb_bytes.len()].copy_from_slice(&dtb_bytes);
    }
    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = KERNEL_LOAD_PA;
        cpu.sp = 0;
        cpu.regs = [0; 31];
        cpu.regs[0] = DTB_PA;
        cpu.regs[4] = KERNEL_LOAD_PA;
        cpu.pstate = FLAG_Z;
    }

    let max_steps = 30_000_000u64;
    let mut halted_at = None;
    for _ in 0..max_steps {
        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            other => {
                halted_at = Some(other);
                break;
            }
        }
    }
    let steps = orch.steps();
    println!("booted {} steps, halt: {:?}", steps, halted_at);

    let m = orch.machine();
    let sr = &m.cpu[0].sysregs;
    println!("sctlr_el1 = {:#018x}", sr.sctlr_el1);
    println!("tcr_el1   = {:#018x}", sr.tcr_el1);
    println!("ttbr0_el1 = {:#018x}", sr.ttbr0_el1);
    println!("ttbr1_el1 = {:#018x}", sr.ttbr1_el1);
    println!("pc        = {:#018x}", m.cpu[0].pc);

    // Manual page-table walk for FAULT_VA (4K granule assumed; verify from TCR).
    let tcr = sr.tcr_el1;
    let tg1 = (tcr >> 30) & 0x3;
    let t1sz = (tcr >> 16) & 0x3f;
    println!("TG1={:#b} T1SZ={}", tg1, t1sz);
    // 4K granule: TG1=0b10
    let ia_bits = 64 - t1sz as u32;
    println!("IA bits = {}", ia_bits);

    // Determine start level for 4K granule.
    let start_level: u32 = if ia_bits > 39 {
        0
    } else if ia_bits > 30 {
        1
    } else if ia_bits > 21 {
        2
    } else {
        3
    };
    println!("start_level = {}", start_level);

    let mut table_base = sr.ttbr1_el1 & 0x0000_FFFF_FFFF_F000;
    println!("ttbr1 base (masked) = {:#x}", table_base);
    let ranges: [(u32, u32, u32); 4] = [(0, 47, 39), (1, 38, 30), (2, 29, 21), (3, 20, 12)];
    for (level, hi, lo) in ranges {
        if level < start_level {
            continue;
        }
        let width = hi - lo + 1;
        let index = (FAULT_VA >> lo) & ((1u64 << width) - 1);
        let entry_pa = table_base + index * 8;
        let desc = read_u64(&m.ram, entry_pa);
        println!(
            "L{}: index={:#x} (bits [{}:{}]) entry_pa={:#x} desc={}",
            level,
            index,
            hi,
            lo,
            entry_pa,
            match desc {
                Some(d) => format!("{:#018x} {}", d, describe_desc(d, level)),
                None => "<unreadable>".to_string(),
            }
        );
        let d = match desc {
            Some(d) => d,
            None => break,
        };
        match d & 0b11 {
            0b11 => {
                // Table descriptor -> next level
                table_base = d & 0x0000_FFFF_FFFF_F000;
                if level == 3 {
                    println!("  -> page descriptor: output base {:#x}", d & 0x0000_FFFF_FFFF_F000);
                    let out_pa = (d & 0x0000_FFFF_FFFF_F000) | (FAULT_VA & 0xfff);
                    println!("  -> final PA = {:#x}", out_pa);
                    break;
                }
            }
            0b01 => {
                // Block descriptor
                let out_mask: u64 = match level {
                    0 => 0x0000_FFFF_C000_0000, // 1GB block: bits [47:30]
                    1 => 0x0000_FFFF_C000_0000,
                    2 => 0x0000_FFFF_FFE0_0000, // 2MB block: bits [47:21]
                    _ => 0,
                };
                let out_pa = (d & out_mask) | (FAULT_VA & !out_mask & 0x0000_FFFF_FFFF_FFFF);
                println!("  -> block descriptor: output PA = {:#x}", out_pa);
                break;
            }
            _ => {
                println!("  -> INVALID/RESERVED descriptor: walk fails here");
                break;
            }
        }
    }

    // What do neighboring kernel-text VAs translate to? Sanity: VA just below.
    println!("--- done ---");
}

fn describe_desc(d: u64, level: u32) -> String {
    match d & 0b11 {
        0b00 => "(invalid)".to_string(),
        0b01 => {
            if level == 3 {
                "(reserved-at-L3)".to_string()
            } else {
                "(block)".to_string()
            }
        }
        0b10 => "(reserved)".to_string(),
        _ => {
            if level == 3 {
                "(page)".to_string()
            } else {
                "(table)".to_string()
            }
        }
    }
}
