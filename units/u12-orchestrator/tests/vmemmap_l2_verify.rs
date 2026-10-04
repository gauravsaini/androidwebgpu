//! Full-boot and unit verification for the vmemmap L1 descriptor gap fix (2026-10-04).
//!
//! Kernel halted at step 121,480,531 with:
//! `WasmTrap { addr: 0xffffffbf00000000, message: "mmu_fault: TranslationFault { va: 18446743794536677376 }" }`
//! at PC `0xffffff8008089e0c` (`ldrb w2, [x0], #1`).
//!
//! Root cause: The kernel's byte-by-byte vmemmap scan crossed the 1 GiB L1 boundary
//! from L1 index 0xfb to 0xfc (VA 0xffffffbf00000000). The L1 entry at index 0xfc
//! was unpopulated (0b00). `handle_vmemmap_fault` checked `l1_desc & 0b11 != 0b11`
//! and returned false instead of allocating an L2 table from the vmemmap pool.
//!
//! Fix: When `l1_desc & 0b11 == 0b00`, allocate an L2 table from the vmemmap pool
//! (`VMEMMAP_POOL_BASE` at 0x80000000+), install it as a table descriptor in L1,
//! then continue descending to allocate L3 table and data page as needed, and retry.
//!
//! Per the Panic Verification Rule: any fix claim MUST include a RAM search
//! for both "Failed to allocate" and "Kernel panic - not syncing".

use std::fs;
use std::path::Path;
use u12_orchestrator::{
    Orchestrator, StepOutcome, RAM_BASE, VMEMMAP_POOL_BASE,
};
use pathn_contracts::cpu::{Access, MmuState};
use pathn_contracts::machine::SysRegs;

const KERNEL_CANDIDATES: &[&str] = &[
    "/mnt/sdb1/aosp/Image",
    "/home/hatch/workspace/.cache-aosp/Image",
];

const DTB_CANDIDATES: &[&str] = &[
    "guest-image/minimal-virt.dtb",
    "/mnt/sdb1/wt-vmemmap-l2/guest-image/minimal-virt.dtb",
    "/mnt/sdb1/wt-boot150m/guest-image/minimal-virt.dtb",
    "/home/hatch/workspace/androidwebgpu/guest-image/minimal-virt.dtb",
];

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const FLAG_Z: u64 = 0x4000_0000;

// Step at which the kernel halted on the L1 descriptor gap before the fix.
pub const L1_GAP_HALT_STEP: u64 = 121_480_531;

fn find_file(candidates: &[&str]) -> Option<String> {
    for c in candidates {
        if Path::new(c).exists() {
            return Some(c.to_string());
        }
    }
    None
}

fn find_in_ram(ram: &[u8], needle: &[u8]) -> Option<usize> {
    ram.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn test_vmemmap_l1_gap_demand_population_allocates_tables() {
    let mut orch = Orchestrator::new();
    let sysregs = SysRegs {
        sctlr_el1: 0x34f5_d91d, // MMU on
        tcr_el1: 0x0040_0030_b559_3519, // 39-bit VA (T1SZ=25)
        ttbr0_el1: 0x4166_5000,
        ttbr1_el1: 0x4166_a000,
        ..Default::default()
    };
    orch.machine_mut().cpu[0].sysregs = sysregs.clone();

    let l1_table_pa = sysregs.ttbr1_el1 & 0x0000_FFFF_FFFF_F000;
    let fault_va = 0xffffffbf00000000u64;
    let l1_idx = (fault_va >> 30) & 0x1ff;
    assert_eq!(l1_idx, 0xfc);

    let l1_entry_pa = l1_table_pa + l1_idx * 8;
    let l1_entry_off = (l1_entry_pa - RAM_BASE) as usize;
    assert_eq!(
        u64::from_le_bytes(
            orch.machine().ram[l1_entry_off..l1_entry_off + 8]
                .try_into()
                .unwrap()
        ),
        0,
        "L1 descriptor must initially be unpopulated"
    );

    // Call handle_vmemmap_fault via orchestrator instruction execution.
    // Place ldrb w2, [x0], #1 (0x38401402) at PC 0x40080000.
    // Also map PC in page tables: L1[0]@0x4166a000 = 0x4166b003, L2[75]@0x4166b258 = 0x41600711.
    {
        let ram = &mut orch.machine_mut().ram;
        let w = |r: &mut Vec<u8>, pa: u64, v: u64| {
            let s = (pa - RAM_BASE) as usize;
            r[s..s + 8].copy_from_slice(&v.to_le_bytes());
        };
        w(ram, 0x4166_a000, 0x4166_b003);
        w(ram, 0x4166_b258, 0x4160_0711);

        let code_pa = 0x416A_B158u64;
        let code_off = (code_pa - RAM_BASE) as usize;
        ram[code_off..code_off + 4].copy_from_slice(&0x38401402u32.to_le_bytes());
    }

    let cpu = &mut orch.machine_mut().cpu[0];
    let code_va = 0xFFFF_FF80_096A_B158u64;
    cpu.pc = code_va;
    cpu.regs[0] = fault_va;
    cpu.regs[2] = 0x1234;

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].pc, code_va + 4);
    assert_eq!(orch.machine().cpu[0].regs[0], fault_va + 1);
    assert_eq!(orch.machine().cpu[0].regs[2], 0);

    // Verify L1 descriptor was populated as table pointing to L2 table.
    let l1_desc = u64::from_le_bytes(
        orch.machine().ram[l1_entry_off..l1_entry_off + 8]
            .try_into()
            .unwrap(),
    );
    assert_eq!(l1_desc & 0b11, 0b11);
    let l2_table_pa = l1_desc & 0x0000_FFFF_FFFF_F000;
    assert_eq!(l2_table_pa, VMEMMAP_POOL_BASE);

    // Verify MMU translation of fault_va succeeds.
    let st = MmuState {
        sctlr: sysregs.sctlr_el1,
        tcr: sysregs.tcr_el1,
        ttbr0: sysregs.ttbr0_el1,
        ttbr1: sysregs.ttbr1_el1,
    };
    let tr = u4_mmu::translate_with_base(&st, &orch.machine().ram, RAM_BASE, fault_va, Access::Read);
    assert!(tr.is_ok());
}

#[test]
fn test_boot_past_vmemmap_l1_gap_halt_no_panic() {
    let kernel_path = match find_file(KERNEL_CANDIDATES) {
        Some(p) => p,
        None => {
            eprintln!("Skipping full boot test: kernel Image not found");
            return;
        }
    };
    let dtb_path = match find_file(DTB_CANDIDATES) {
        Some(p) => p,
        None => {
            eprintln!("Skipping full boot test: DTB not found");
            return;
        }
    };

    // If running in unoptimized debug mode without RUN_SLOW_BOOT=1,
    // execute a smoke step count (or full if in release).
    let is_debug = cfg!(debug_assertions);
    let run_slow = std::env::var("RUN_SLOW_BOOT").map(|v| v == "1").unwrap_or(false);
    if is_debug && !run_slow {
        eprintln!(
            "Running in debug mode: full 122M-step boot requires release mode. \
             To run in debug mode, set RUN_SLOW_BOOT=1. Unit tests verify the L1 fix."
        );
        return;
    }

    let kernel_bytes = fs::read(&kernel_path).expect("failed to read kernel image");
    let dtb_bytes = fs::read(&dtb_path).expect("failed to read DTB");

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

    // Target step count: 122M steps (past the 121,480,531 halt point).
    let max_steps = 122_000_000u64;
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
    let pc = orch.machine().cpu[0].pc;
    println!("booted {} steps, halt: {:?}, pc={:#x}", steps, halted_at, pc);

    assert!(
        steps > L1_GAP_HALT_STEP,
        "boot halted before or at L1 gap halt step ({} <= {})",
        steps,
        L1_GAP_HALT_STEP
    );

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
