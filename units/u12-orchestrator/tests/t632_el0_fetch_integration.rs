//! T632: EL0 instruction fetch through the full Orchestrator path.
//!
//! The post-fix cold boot halted identically to the pre-fix boot
//! (147,106,947 steps, FetchFault at 0x210158), so these tests pin the
//! fixed path directly: a Linux user PROT_EXEC page (AP=0b11, PXN=1,
//! UXN=0) must fetch at EL0, must fault at EL1, and the ERET handoff
//! (SPSR mode EL0t) must land in the EL0 fetch path.

use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};

const NOP: u32 = 0xd503_201f;
const ERET: u32 = 0xd69f_03e0;

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn write_u64(orch: &mut Orchestrator, addr: u64, val: u64) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

fn table_desc(base_pa: u64) -> u64 {
    (base_pa & 0x0000_FFFF_FFFF_F000) | 0b11
}

/// Build TTBR0 tables for VA 0x210158 (user .text, Linux PROT_EXEC mapping)
/// and VA 0x1000 (kernel-style text page holding the ERET probe).
/// Returns nothing; tables live in guest RAM.
fn setup_tables(orch: &mut Orchestrator) {
    let l1_pa = RAM_BASE + 0x10_000;
    let l2_pa = RAM_BASE + 0x11_000;
    let l3_pa = RAM_BASE + 0x12_000;
    let user_code_pa = RAM_BASE + 0x13_000;
    let kern_code_pa = RAM_BASE + 0x14_000;
    for i in 0..512 {
        write_u64(orch, l1_pa + i * 8, 0);
        write_u64(orch, l2_pa + i * 8, 0);
        write_u64(orch, l3_pa + i * 8, 0);
    }
    write_u64(orch, l1_pa, table_desc(l2_pa));
    // VA 0x1000 lives in L2 block 0, VA 0x210158 in L2 block 1; both link
    // to the same L3 table (indices 1 and 0x10 do not overlap).
    write_u64(orch, l2_pa, table_desc(l3_pa));
    write_u64(orch, l2_pa + 1 * 8, table_desc(l3_pa));
    // L3[0x10] = VA 0x210158: user PROT_EXEC page, AP=0b11, PXN=1, UXN=0.
    let user_desc: u64 =
        (user_code_pa & 0x0000_FFFF_FFFF_F000) | (0b11 << 6) | (1 << 10) | (1 << 53) | 0b11;
    write_u64(orch, l3_pa + 0x10 * 8, user_desc);
    // L3[1] = VA 0x1000: kernel-style text page, AP=0b00, PXN=0, UXN=1.
    let kern_desc: u64 =
        (kern_code_pa & 0x0000_FFFF_FFFF_F000) | (1 << 10) | (1 << 54) | 0b11;
    write_u64(orch, l3_pa + 1 * 8, kern_desc);
    write_word(orch, user_code_pa + 0x158, NOP);
    write_word(orch, kern_code_pa, ERET);

    let cpu = &mut orch.machine.cpu[0];
    cpu.sysregs.sctlr_el1 = 1;
    cpu.sysregs.tcr_el1 = 0x0040_0032_b559_3519;
    cpu.sysregs.ttbr0_el1 = l1_pa;
    cpu.sysregs.ttbr1_el1 = l1_pa;
}

#[test]
fn t632_el0_fetch_user_exec_page_via_step() {
    let mut orch = Orchestrator::new();
    setup_tables(&mut orch);
    let cpu = &mut orch.machine.cpu[0];
    cpu.pc = 0x210158;
    cpu.pstate = 0x00; // EL0t
    match orch.step_vcpu() {
        StepOutcome::Continue => {}
        other => panic!("expected Continue at EL0, got {:?}", other),
    }
    assert_eq!(orch.machine.cpu[0].pc, 0x21015c);
}

#[test]
fn t632_el1_fetch_user_exec_page_denied_via_step() {
    let mut orch = Orchestrator::new();
    setup_tables(&mut orch);
    let cpu = &mut orch.machine.cpu[0];
    cpu.pc = 0x210158;
    cpu.pstate = 0x05; // EL1h
    match orch.step_vcpu() {
        StepOutcome::Halted(HaltReason::FetchFault { addr }) => {
            assert_eq!(addr, 0x210158);
        }
        other => panic!("expected FetchFault halt at EL1, got {:?}", other),
    }
}

#[test]
fn t632_eret_lands_in_el0_fetch_path() {
    let mut orch = Orchestrator::new();
    setup_tables(&mut orch);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = 0x1000;
        cpu.pstate = 0x05; // EL1h
        cpu.sysregs.elr_el1 = 0x210158;
        cpu.sysregs.spsr_el1 = 0x00; // EL0t
    }
    // Step 1: ERET executes, restoring PC + PSTATE from ELR/SPSR.
    match orch.step_vcpu() {
        StepOutcome::Continue => {}
        other => panic!("expected Continue on ERET, got {:?}", other),
    }
    assert_eq!(orch.machine.cpu[0].pc, 0x210158);
    assert_eq!(orch.machine.cpu[0].pstate & 0x1F, 0x00);
    // Step 2: the first userspace fetch must succeed through the EL0 path.
    match orch.step_vcpu() {
        StepOutcome::Continue => {}
        other => panic!("expected Continue on EL0 fetch, got {:?}", other),
    }
    assert_eq!(orch.machine.cpu[0].pc, 0x21015c);
}

const MSR_SPSR_X21: u32 = 0xd518_4015; // msr spsr_el1, x21
const MSR_ELR_X22: u32 = 0xd518_4020 | 22; // msr elr_el1, x22

/// T632 hypothesis (a): the kernel restores the user PSTATE via
/// MSR SPSR_EL1 (not a direct sysreg write), then ERET. If the engine's
/// MSR path drops or mangles SPSR, ERET restores a stale EL1 mode and
/// the first EL0 fetch is denied by PXN — the exact observed halt.
#[test]
fn t632_msr_spsr_then_eret_lands_in_el0() {
    let mut orch = Orchestrator::new();
    setup_tables(&mut orch);
    // Kernel page: MSR SPSR_EL1, X21 ; MSR ELR_EL1, X22 ; ERET.
    // VA 0x1000 maps to kern_code_pa (L3[1]); lay the sequence there.
    let kern_page = RAM_BASE + 0x14_000;
    let off = (kern_page - RAM_BASE) as usize;
    orch.machine.ram[off..off + 4].copy_from_slice(&MSR_SPSR_X21.to_le_bytes());
    orch.machine.ram[off + 4..off + 8].copy_from_slice(&MSR_ELR_X22.to_le_bytes());
    orch.machine.ram[off + 8..off + 12].copy_from_slice(&ERET.to_le_bytes());
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = 0x1000;
        cpu.pstate = 0x05; // EL1h
        cpu.regs[21] = 0x00; // user PSTATE: EL0t, DAIF clear
        cpu.regs[22] = 0x210158;
        // Poison SPSR_EL1 with an EL1 value: if MSR fails to overwrite it,
        // ERET must restore EL1 and the fetch must fault — the test's
        // failure mode, not a silent pass.
        cpu.sysregs.spsr_el1 = 0x05;
    }
    for _ in 0..4 {
        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            other => panic!("expected Continue, got {:?}", other),
        }
    }
    assert_eq!(orch.machine.cpu[0].pc, 0x21015c);
    assert_eq!(orch.machine.cpu[0].pstate & 0x1F, 0x00);
}

const MSR_TTBR0_X23: u32 = 0xd518_2017; // msr ttbr0_el1, x23

/// T632 hypothesis (a''): the kernel switches TTBR0 at execve via
/// MSR TTBR0_EL1. If that MSR writes the wrong register (or a masked
/// base), the ERET lands at the right PC but the walk uses a stale
/// table base and the first EL0 fetch faults — the exact observed halt.
/// Poison TTBR0 first so a no-op MSR cannot pass silently.
#[test]
fn t632_msr_ttbr0_then_eret_lands_in_el0() {
    let mut orch = Orchestrator::new();
    setup_tables(&mut orch);
    let kern_page = RAM_BASE + 0x14_000;
    let off = (kern_page - RAM_BASE) as usize;
    orch.machine.ram[off..off + 4].copy_from_slice(&MSR_TTBR0_X23.to_le_bytes());
    orch.machine.ram[off + 4..off + 8].copy_from_slice(&MSR_SPSR_X21.to_le_bytes());
    orch.machine.ram[off + 8..off + 12].copy_from_slice(&MSR_ELR_X22.to_le_bytes());
    orch.machine.ram[off + 12..off + 16].copy_from_slice(&ERET.to_le_bytes());
    let l1_pa = RAM_BASE + 0x10_000;
    {
        let cpu = &mut orch.machine.cpu[0];
        // Kernel VA: fetched via TTBR1, so the poisoned TTBR0 cannot
        // break the sequence itself — exactly like the real ret_to_user.
        cpu.pc = 0xffffff80_00001000;
        cpu.pstate = 0x05; // EL1h
        cpu.regs[21] = 0x00; // EL0t
        cpu.regs[22] = 0x210158;
        cpu.regs[23] = l1_pa;
        cpu.sysregs.ttbr0_el1 = 0xdead_0000; // poisoned: no valid tables
        cpu.sysregs.spsr_el1 = 0x05;
    }
    for _ in 0..5 {
        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            other => panic!("expected Continue, got {:?}", other),
        }
    }
    assert_eq!(orch.machine.cpu[0].pc, 0x21015c);
    assert_eq!(orch.machine.cpu[0].pstate & 0x1F, 0x00);
    assert_eq!(orch.machine.cpu[0].sysregs.ttbr0_el1, l1_pa);
}
