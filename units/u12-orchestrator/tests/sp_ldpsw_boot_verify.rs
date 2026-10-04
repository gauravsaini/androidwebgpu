//! Full-boot verification for the SP-relative LDPSW fix (2026-10-04).
//!
//! After the memblock SUB fix (6da41b8), the kernel halted at step 20,610,084
//! on `ldpsw x9, x10, [sp]` (0x69402BE9) -- LDPSW was excluded from the
//! u12 fast-path pair arm and U2 traps all SP-relative pairs. This test
//! boots past that point and RAM-searches for panic strings.
//!
//! Per the Panic Verification Rule: any fix claim MUST include a RAM search
//! for both "Failed to allocate" and "Kernel panic - not syncing".

use std::fs;
use u12_orchestrator::{Orchestrator, StepOutcome};

const KERNEL_PATH: &str = "/home/hatch/workspace/.cache-aosp/Image";
const DTB_PATH: &str = "/home/hatch/workspace/androidwebgpu/guest-image/minimal-virt.dtb";

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const RAM_BASE: u64 = 0x4000_0000;
const FLAG_Z: u64 = 0x4000_0000;

// Step at which the kernel halted on the LDPSW trap before the fix.
const LDPSW_HALT_STEP: u64 = 20_610_084;

fn find_in_ram(ram: &[u8], needle: &[u8]) -> Option<usize> {
    ram.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn test_full_boot_past_ldpsw_halt_no_panic() {
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

    // Must have proceeded past the old LDPSW halt point.
    assert!(
        steps > LDPSW_HALT_STEP,
        "boot did not pass the LDPSW halt point ({} steps)",
        steps
    );

    // MANDATORY RAM search for panic strings.
    // NOTE: search for FORMATTED messages (with values), not the printk
    // format strings (which always live in .rodata, e.g.
    // "Failed to allocate %llu bytes ..." and
    // "Kernel panic - not syncing: %s"). A real panic prints the values:
    // "Failed to allocate 0x1000 bytes below 0x0".
    let ram = &orch.machine().ram;
    let fail_alloc = find_in_ram(ram, b"Failed to allocate 0x");
    let kernel_panic = find_in_ram(ram, b"Kernel panic - not syncing: Failed");

    if let Some(off) = fail_alloc {
        let end = (off + 120).min(ram.len());
        println!(
            "FOUND 'Failed to allocate' at RAM offset {:#x}: {:?}",
            off,
            String::from_utf8_lossy(&ram[off..end])
        );
    }
    if let Some(off) = kernel_panic {
        let end = (off + 120).min(ram.len());
        println!(
            "FOUND 'Kernel panic' at RAM offset {:#x}: {:?}",
            off,
            String::from_utf8_lossy(&ram[off..end])
        );
    }

    assert!(fail_alloc.is_none(), "'Failed to allocate 0x' found in RAM");
    assert!(
        kernel_panic.is_none(),
        "'Kernel panic - not syncing: Failed' found in RAM"
    );
    println!(
        "RAM SEARCH CLEAN: no panic strings after {} steps (past LDPSW halt)",
        steps
    );
}
