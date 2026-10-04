//! Extend boot beyond 35M steps to find the next halt (2026-10-04).
//!
//! After the vmemmap demand-population fix (0b8f2bb), the kernel booted
//! 35M steps with NO HALT (hit the test cap). This test raises the cap to
//! 100M to find where the kernel halts next — or whether it keeps going.
//!
//! Per the Panic Verification Rule: RAM is searched for both panic strings
//! regardless of outcome.

use std::fs;
use u12_orchestrator::{Orchestrator, StepOutcome};

const KERNEL_PATH: &str = "/home/hatch/workspace/.cache-aosp/Image";
const DTB_PATH: &str = "/home/hatch/workspace/androidwebgpu/guest-image/minimal-virt.dtb";

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const RAM_BASE: u64 = 0x4000_0000;
const FLAG_Z: u64 = 0x4000_0000;

fn find_in_ram(ram: &[u8], needle: &[u8]) -> Option<usize> {
    ram.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn test_boot_100m_find_next_halt() {
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

    let max_steps = 100_000_000u64;
    let mut halted_at = None;
    let mut last_report = 0u64;
    for _ in 0..max_steps {
        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            other => {
                halted_at = Some(other);
                break;
            }
        }
        let steps = orch.steps();
        if steps - last_report >= 5_000_000 {
            let pc = orch.machine().cpu[0].pc;
            println!("... {} steps, pc={:#x}", steps, pc);
            last_report = steps;
        }
    }
    let steps = orch.steps();
    let pc = orch.machine().cpu[0].pc;
    println!("booted {} steps, halt: {:?}, pc={:#x}", steps, halted_at, pc);

    // Mandatory RAM search per Panic Verification Rule.
    let ram = &orch.machine().ram;
    let panic1 = b"Failed to allocate 0x";
    let panic2 = b"Kernel panic - not syncing: Failed";
    let found1 = find_in_ram(ram, panic1);
    let found2 = find_in_ram(ram, panic2);
    println!(
        "RAM search 'Failed to allocate 0x': {:?}",
        found1.map(|o| format!("{:#x}", o))
    );
    println!(
        "RAM search 'Kernel panic - not syncing: Failed': {:?}",
        found2.map(|o| format!("{:#x}", o))
    );
    assert!(found1.is_none(), "PANIC STRING FOUND in RAM: 'Failed to allocate 0x'");
    assert!(
        found2.is_none(),
        "PANIC STRING FOUND in RAM: 'Kernel panic - not syncing: Failed'"
    );
    println!("RAM SEARCH CLEAN — no panic strings after {} steps.", steps);
}
