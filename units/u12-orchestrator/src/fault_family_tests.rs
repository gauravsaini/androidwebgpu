use super::*;

const LDR_X0_X1: u32 = 0xF940_0020;
const STR_X0_X1: u32 = 0xF900_0020;
const LDR_X0_SP: u32 = 0xF940_03E0;
const MRS_X0_ESR_EL1: u32 = 0xD538_5200;
const MRS_X1_FAR_EL1: u32 = 0xD538_6001;
const TEST_VBAR: u64 = RAM_BASE + 0x8000;
const TEST_DATA: u64 = RAM_BASE + 0xA000;

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn configure(source_mode: u64, source_pc: u64, word: u32) -> Orchestrator {
    let mut orch = Orchestrator::new();
    write_word(&mut orch, source_pc, word);
    let vector_offset = if source_mode == EL1H_MODE {
        0x200
    } else {
        0x400
    };
    let vector = TEST_VBAR + vector_offset;
    write_word(&mut orch, vector, MRS_X0_ESR_EL1);
    write_word(&mut orch, vector + 4, MRS_X1_FAR_EL1);
    let cpu = &mut orch.machine.cpu[0];
    cpu.pc = source_pc;
    cpu.sp = 0x1111_0000;
    cpu.sp_el1 = 0x2222_0000;
    cpu.pstate = 0xA000_0000 | source_mode | PSTATE_IRQ_MASK;
    cpu.sysregs.daif = PSTATE_IRQ_MASK;
    cpu.sysregs.sp_el0 = 0x1111_0000;
    cpu.sysregs.vbar_el1 = TEST_VBAR;
    orch
}

fn configure_mmu(orch: &mut Orchestrator, data_accessed: bool) {
    let l0 = RAM_BASE + 0x1000;
    let l1 = RAM_BASE + 0x2000;
    let l2 = RAM_BASE + 0x3000;
    let l3 = RAM_BASE + 0x4000;
    let write_desc = |ram: &mut Vec<u8>, pa: u64, value: u64| {
        let offset = (pa - RAM_BASE) as usize;
        ram[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    };
    let table = |pa: u64| pa | 0b11;
    let page = |pa: u64, accessed: bool| (pa & !0xFFF) | (u64::from(accessed) << 10) | 0b11;

    write_desc(&mut orch.machine.ram, l0, table(l1));
    write_desc(&mut orch.machine.ram, l1 + 8, table(l2));
    write_desc(&mut orch.machine.ram, l2, table(l3));
    write_desc(&mut orch.machine.ram, l3, page(RAM_BASE, true));
    write_desc(&mut orch.machine.ram, l3 + 8 * 8, page(TEST_VBAR, true));
    write_desc(
        &mut orch.machine.ram,
        l3 + 10 * 8,
        page(TEST_DATA, data_accessed),
    );

    let sysregs = &mut orch.machine.cpu[0].sysregs;
    sysregs.sctlr_el1 = 1;
    sysregs.tcr_el1 = 16;
    sysregs.ttbr0_el1 = l0;
    sysregs.vbar_el1 = TEST_VBAR;
}

fn read_syndrome(orch: &mut Orchestrator, esr: u64, far: u64) {
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "read ESR_EL1");
    assert_eq!(orch.machine.cpu[0].regs[0], esr, "ESR_EL1");
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "read FAR_EL1");
    assert_eq!(orch.machine.cpu[0].regs[1], far, "FAR_EL1");
}

fn assert_entry(
    orch: &mut Orchestrator,
    source_mode: u64,
    source_pc: u64,
    return_pc: u64,
    esr: u64,
    far: u64,
) {
    let cpu = &orch.machine.cpu[0];
    let saved_pstate = (cpu.pstate & !PSTATE_DAIF_MASK) | (cpu.sysregs.daif & PSTATE_DAIF_MASK);
    assert!(
        return_pc == source_pc || return_pc == source_pc.wrapping_add(4),
        "exception return PC must be the faulting instruction or the next instruction"
    );
    assert_eq!(
        orch.step_vcpu(),
        StepOutcome::Continue,
        "take synchronous exception"
    );
    let cpu = &orch.machine.cpu[0];
    let offset = if source_mode == EL1H_MODE {
        0x200
    } else {
        0x400
    };
    assert_eq!(cpu.pc, TEST_VBAR + offset, "exception vector");
    assert_eq!(cpu.sysregs.elr_el1, return_pc, "return address");
    assert_eq!(cpu.sysregs.spsr_el1, saved_pstate, "saved PSTATE");
    assert_eq!(cpu.pstate & PSTATE_MODE_MASK, EL1H_MODE);
    read_syndrome(orch, esr, far);
}

#[test]
fn fp_access_trap_enters_lower_el_vector_with_ec_07() {
    // FADD D0, D1, D2. CPACR_EL1.FPEN=00 traps this EL0 access.
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL0T_MODE, pc, 0x1E62_2820);
    orch.machine.cpu[0].sysregs.cpacr_el1 = 0;

    assert_entry(&mut orch, EL0T_MODE, pc, pc, 0x1FE0_0000, 0);
}

#[test]
fn misaligned_pc_enters_pc_alignment_vector_with_far() {
    let pc = RAM_BASE + 0x101;
    let mut orch = configure(EL1H_MODE, pc, 0xD503_201F);

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x8A00_0000, pc);
}

#[test]
fn unaligned_data_access_with_sctlr_a_enters_data_abort() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_X1);
    let far = TEST_DATA + 1;
    orch.machine.cpu[0].regs[1] = far;
    orch.machine.cpu[0].sysregs.sctlr_el1 |= 1 << 1; // SCTLR_EL1.A

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x9600_0021, far);
}

#[test]
fn misaligned_sp_access_with_sctlr_sa_matches_qemu_data_alignment_abort() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_SP);
    orch.machine.cpu[0].sp = TEST_DATA + 1;
    orch.machine.cpu[0].sysregs.sctlr_el1 &= !(1 << 1); // SCTLR_EL1.A clear.
    orch.machine.cpu[0].sysregs.sctlr_el1 |= 1 << 3; // SCTLR_EL1.SA

    // QEMU 11.1.2 -M virt -cpu cortex-a53: EC=0x25, ISS=0x21,
    // FAR_EL1=the misaligned SP-based access address.
    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x9600_0021, TEST_DATA + 1);
}

#[test]
fn out_of_region_va_uses_level_zero_translation_fault() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_X1);
    configure_mmu(&mut orch, true);
    orch.machine.cpu[0].regs[1] = 0x0001_0000_0000_0000;

    assert_entry(
        &mut orch,
        EL1H_MODE,
        pc,
        pc,
        0x9600_0004,
        0x0001_0000_0000_0000,
    );
}

#[test]
fn access_flag_fault_uses_level_three_dfsc() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_X1);
    configure_mmu(&mut orch, false);
    orch.machine.cpu[0].regs[1] = TEST_DATA;

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x9600_000B, TEST_DATA);
}

#[test]
fn write_to_readonly_page_reports_permission_abort() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, STR_X0_X1);
    configure_mmu(&mut orch, true);
    let l3_entry = RAM_BASE + 0x4000 + 10 * 8;
    let offset = (l3_entry - RAM_BASE) as usize;
    let mut descriptor =
        u64::from_le_bytes(orch.machine.ram[offset..offset + 8].try_into().unwrap());
    descriptor |= 0b10 << 6; // AP=10: EL1 read-only.
    orch.machine.ram[offset..offset + 8].copy_from_slice(&descriptor.to_le_bytes());
    orch.machine.cpu[0].regs[1] = TEST_DATA;

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x9600_004F, TEST_DATA);
}

#[test]
fn unmapped_physical_access_is_external_abort_with_valid_far() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_X1);
    let far = 0x9000_0000;
    orch.machine.cpu[0].regs[1] = far;

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x9600_0010, far);
}

#[test]
fn external_abort_during_table_walk_uses_tablewalk_dfsc() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_X1);
    configure_mmu(&mut orch, true);
    let l1 = RAM_BASE + 0x2000;
    let offset = (l1 + 16 - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 8].copy_from_slice(&(0x9000_0000u64 | 0b11).to_le_bytes());
    orch.machine.cpu[0].regs[1] = 0x8000_0000;

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0x9600_0016, 0x8000_0000);
}

#[test]
fn brk_enters_software_breakpoint_vector_with_comment() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, 0xD422_4680);

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0xF200_1234, 0);
}

#[test]
fn hardware_breakpoint_enters_current_el_debug_vector() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, 0xD503_201F);
    let cpu = &mut orch.machine.cpu[0];
    cpu.sysregs.dbg_bvr[0] = pc;
    cpu.sysregs.dbg_bcr[0] = (0xF << 5) | (0b01 << 1) | 1;
    cpu.sysregs.mdscr_el1 = (1 << 15) | (1 << 13); // MDE + KDE

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0xC600_0022, 0);
}

#[test]
fn el1_hardware_breakpoint_ignores_el0_only_control() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, 0xD503_201F);
    let cpu = &mut orch.machine.cpu[0];
    cpu.sysregs.dbg_bvr[0] = pc;
    cpu.sysregs.dbg_bcr[0] = (0xF << 5) | (0b10 << 1) | 1;
    cpu.sysregs.mdscr_el1 = (1 << 15) | (1 << 13); // MDE + KDE

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, pc + 4);
}

#[test]
fn watchpoint_enters_current_el_debug_vector_with_far() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, LDR_X0_X1);
    let far = TEST_DATA;
    let cpu = &mut orch.machine.cpu[0];
    cpu.regs[1] = far;
    cpu.sysregs.dbg_wvr[0] = far;
    cpu.sysregs.dbg_wcr[0] = (0xFF << 5) | (0b11 << 3) | (0b01 << 1) | 1;
    cpu.sysregs.mdscr_el1 = (1 << 15) | (1 << 13); // MDE + KDE

    assert_entry(&mut orch, EL1H_MODE, pc, pc, 0xD600_0022, far);
}

#[test]
fn software_step_enters_current_el_debug_vector_after_retirement() {
    let pc = RAM_BASE + 0x100;
    let mut orch = configure(EL1H_MODE, pc, 0xD503_201F);
    orch.machine.cpu[0].pstate |= 1 << 21; // PSTATE.SS
    orch.machine.cpu[0].sysregs.mdscr_el1 = (1 << 15) | (1 << 13) | 1; // MDE + KDE + SS

    assert_entry(&mut orch, EL1H_MODE, pc, pc + 4, 0xCE00_0022, 0);
}
