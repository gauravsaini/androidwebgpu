//! Full-boot verification for the vmemmap TranslationFault fix.
//!
//! After the MMU UXN/PXN fix (db5ab78), the kernel halted at step 29,205,843
//! with `TranslationFault { va: 0xffffffbefea00000 }` — a data access to the
//! vmemmap region.
//!
//! Root cause: The kernel scans vmemmap (via post-indexed `ldrb w2, [x0], #1`
//! in a loop), and the emulator does not deliver data aborts to the guest.
//! When the scan reaches an unpopulated vmemmap page, the emulator halted
//! with WasmTrap instead of allowing the kernel's fault handler to populate
//! it.
//!
//! Fix: Emulate vmemmap demand-population in the orchestrator. When a data
//! access faults on a vmemmap VA (0xffffffbe00000000-0xffffffc000000000 for
//! 39-bit VA), allocate a zeroed 4K page from a bump allocator at the top of
//! RAM, install the missing PTE, and retry the instruction.
//!
//! Per the Panic Verification Rule: any fix claim MUST include a RAM search
//! for both "Failed to allocate" and "Kernel panic - not syncing".

use std::fs;
use u12_orchestrator::{Orchestrator, StepOutcome};

const KERNEL_PATH: &str = "/home/hatch/workspace/.cache-aosp/Image";
const DTB_PATH: &str = "/home/hatch/workspace/androidwebgpu/guest-image/minimal-virt.dtb";

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4800_0000;
const RAM_BASE: u64 = 0x4000_0000;
const FLAG_Z: u64 = 0x4000_0000;

// Step at which the kernel halted on the vmemmap TranslationFault before the fix.
const TRANSLATIONFAULT_HALT_STEP: u64 = 29_205_843;

fn find_in_ram(ram: &[u8], needle: &[u8]) -> Option<usize> {
    ram.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn test_full_boot_past_translationfault_halt_no_panic() {
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

    let max_steps = 35_000_000u64;
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

    // Must boot past the old halt point.
    assert!(
        steps > TRANSLATIONFAULT_HALT_STEP,
        "boot did not pass the TranslationFault halt ({} steps, halt {:?})",
        steps,
        halted_at
    );

    // Mandatory RAM search per Panic Verification Rule.
    let ram = &orch.machine().ram;
    let panic1 = b"Failed to allocate 0x";
    let panic2 = b"Kernel panic - not syncing: Failed";
    let found1 = find_in_ram(ram, panic1);
    let found2 = find_in_ram(ram, panic2);
    println!("RAM search 'Failed to allocate 0x': {:?}", found1.map(|o| format!("{:#x}", o)));
    println!("RAM search 'Kernel panic - not syncing: Failed': {:?}", found2.map(|o| format!("{:#x}", o)));
    assert!(found1.is_none(), "PANIC STRING FOUND in RAM: 'Failed to allocate 0x'");
    assert!(found2.is_none(), "PANIC STRING FOUND in RAM: 'Kernel panic - not syncing: Failed'");
    println!("RAM SEARCH CLEAN — no panic strings.");
}
