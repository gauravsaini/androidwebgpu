use super::*;

const SVC_1234: u32 = 0xD402_4681;
const HVC_1234: u32 = 0xD402_4682;
const SMC_1234: u32 = 0xD402_4683;
const LDR_X0_X1: u32 = 0xF940_0020;
const STR_X0_X1: u32 = 0xF900_0020;
const MRS_X0_ESR_EL1: u32 = 0xD538_5200;
const MRS_X1_FAR_EL1: u32 = 0xD538_6001;
const ERET: u32 = 0xD69F_03E0;
const NOP: u32 = 0xD503_201F;

const TEST_VBAR: u64 = RAM_BASE + 0x1000;

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn install_syndrome_reads(orch: &mut Orchestrator, vector: u64) {
    write_word(orch, vector, MRS_X0_ESR_EL1);
    write_word(orch, vector + 4, MRS_X1_FAR_EL1);
}

fn read_syndrome(orch: &mut Orchestrator, esr: u64, far: u64) {
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "read ESR_EL1");
    assert_eq!(orch.machine.cpu[0].regs[0], esr, "ESR_EL1");
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "read FAR_EL1");
    assert_eq!(orch.machine.cpu[0].regs[1], far, "FAR_EL1");
}

fn configure_mmu_pages(orch: &mut Orchestrator, read_only_data: bool) {
    let l0 = RAM_BASE + 0x1000;
    let l1 = RAM_BASE + 0x2000;
    let l2 = RAM_BASE + 0x3000;
    let l3 = RAM_BASE + 0x4000;
    let write_desc = |ram: &mut Vec<u8>, pa: u64, value: u64| {
        let offset = (pa - RAM_BASE) as usize;
        ram[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    };
    let table = |pa: u64| pa | 0b11;
    let page = |pa: u64, ap: u64| (pa & !0xFFF) | (ap << 6) | (1 << 10) | 0b11;

    write_desc(&mut orch.machine.ram, l0, table(l1)); // L0[0]
    write_desc(&mut orch.machine.ram, l1 + 8, table(l2)); // L1[1]
    write_desc(&mut orch.machine.ram, l2, table(l3)); // L2[0]
    write_desc(&mut orch.machine.ram, l3, page(RAM_BASE, 0)); // L3[0]: code
    write_desc(&mut orch.machine.ram, l3 + 8, page(RAM_BASE + 0x1000, 0)); // L3[1]: vector
    if read_only_data {
        write_desc(
            &mut orch.machine.ram,
            l3 + 16,
            page(RAM_BASE + 0x2000, 0b10),
        ); // L3[2]
    }

    let sysregs = &mut orch.machine.cpu[0].sysregs;
    sysregs.sctlr_el1 = 1;
    sysregs.tcr_el1 = 16; // 4K granule, 48-bit lower VA range.
    sysregs.ttbr0_el1 = l0;
    sysregs.vbar_el1 = TEST_VBAR;
}

fn configure_service_call(
    word: u32,
    source_mode: u64,
    vector_offset: u64,
) -> (Orchestrator, u64, u64, u64) {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE;
    let vector = TEST_VBAR + vector_offset;
    let saved_pstate = 0xA000_0000 | source_mode | PSTATE_IRQ_MASK;
    write_word(&mut orch, source_pc, word);
    install_syndrome_reads(&mut orch, vector);
    let cpu = &mut orch.machine.cpu[0];
    cpu.pc = source_pc;
    cpu.sp = 0x1111_0000;
    cpu.sp_el1 = 0x2222_0000;
    cpu.pstate = saved_pstate;
    cpu.sysregs.daif = PSTATE_IRQ_MASK;
    cpu.sysregs.vbar_el1 = TEST_VBAR;
    (orch, source_pc, vector, saved_pstate)
}

fn assert_service_entry(
    mut orch: Orchestrator,
    source_pc: u64,
    vector: u64,
    saved_pstate: u64,
    expected_esr: u64,
) {
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector, "synchronous vector");
    assert_eq!(cpu.sysregs.elr_el1, source_pc + 4, "service-call resume PC");
    assert_eq!(cpu.sysregs.spsr_el1, saved_pstate, "saved PSTATE");
    assert_eq!(cpu.pstate & PSTATE_MODE_MASK, EL1H_MODE);
    if saved_pstate & PSTATE_MODE_MASK == EL1H_MODE {
        assert_eq!(cpu.sysregs.sp_el0, 0);
        assert_eq!(cpu.sp, 0x1111_0000);
    } else {
        assert_eq!(cpu.sysregs.sp_el0, 0x1111_0000);
        assert_eq!(cpu.sp, 0x2222_0000);
    }
    read_syndrome(&mut orch, expected_esr, 0);
}

fn configure_fault_orch(source_mode: u64, faulting_pc: u64, read_only_data: bool) -> Orchestrator {
    let mut orch = Orchestrator::new();
    configure_mmu_pages(&mut orch, read_only_data);
    install_syndrome_reads(&mut orch, TEST_VBAR + 0x200);
    install_syndrome_reads(&mut orch, TEST_VBAR + 0x400);
    let cpu = &mut orch.machine.cpu[0];
    cpu.pc = faulting_pc;
    cpu.sp = 0x1111_0000;
    cpu.sp_el1 = 0x2222_0000;
    cpu.pstate = 0xA000_0000 | source_mode | PSTATE_IRQ_MASK;
    cpu.sysregs.daif = PSTATE_IRQ_MASK;
    cpu.regs[1] = RAM_BASE + 0x2000;
    cpu.sysregs.sp_el0 = 0x1111_0000;
    orch
}

fn assert_abort_entry(
    orch: &mut Orchestrator,
    source_pc: u64,
    vector: u64,
    saved_pstate: u64,
    expected_esr: u64,
    expected_far: u64,
) {
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "take abort");
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector, "abort vector");
    assert_eq!(cpu.sysregs.elr_el1, source_pc, "abort resume PC");
    assert_eq!(cpu.sysregs.spsr_el1, saved_pstate, "saved PSTATE");
    assert_eq!(cpu.pstate & PSTATE_MODE_MASK, EL1H_MODE);
    assert_eq!(cpu.sysregs.sp_el0, 0x1111_0000);
    assert_eq!(
        cpu.sp,
        if saved_pstate & PSTATE_MODE_MASK == EL1H_MODE {
            0x1111_0000
        } else {
            0x2222_0000
        }
    );
    read_syndrome(orch, expected_esr, expected_far);
}

#[test]
fn svc_lower_aarch64_uses_sync_vector_and_syndrome_ec_15() {
    let (orch, source, vector, saved) = configure_service_call(SVC_1234, EL0T_MODE, 0x400);
    assert_service_entry(orch, source, vector, saved, 0x5600_1234);
}

#[test]
fn hvc_uses_current_el_spx_sync_vector_and_syndrome_ec_16() {
    let (orch, source, vector, saved) = configure_service_call(HVC_1234, EL1H_MODE, 0x200);
    assert_service_entry(orch, source, vector, saved, 0x5A00_1234);
}

#[test]
fn smc_uses_current_el_sp0_sync_vector_and_syndrome_ec_17() {
    let (orch, source, vector, saved) = configure_service_call(SMC_1234, EL1T_MODE, 0x000);
    assert_service_entry(orch, source, vector, saved, 0x5E00_1234);
}

#[test]
fn svc_from_lower_aarch32_uses_sync_vector_at_vbar_plus_600() {
    let (orch, source, vector, saved) = configure_service_call(SVC_1234, 0x10, 0x600);
    assert_service_entry(orch, source, vector, saved, 0x4600_1234);
}

#[test]
fn hvc_zero_keeps_the_existing_psci_shim() {
    let mut orch = Orchestrator::new();
    write_word(&mut orch, RAM_BASE, 0xD400_0002);
    orch.machine.cpu[0].pc = RAM_BASE;
    orch.machine.cpu[0].regs[0] = 0x8400_0000; // PSCI_VERSION

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 4);
    assert_eq!(orch.machine.cpu[0].regs[0], 0x0001_0000);
}

#[test]
fn eret_from_sync_handler_nested_in_irq_keeps_irq_diagnostic_entry() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE;
    let vector_base = RAM_BASE + 0x1000;
    let irq_vector = vector_base + 0x280;
    let sync_vector = vector_base + 0x200;
    write_word(&mut orch, source_pc, NOP);
    write_word(&mut orch, irq_vector, HVC_1234);
    write_word(&mut orch, irq_vector + 4, ERET);
    write_word(&mut orch, sync_vector, ERET);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.pstate = 0x4000_0005; // EL1h, IRQ unmasked.
        cpu.sysregs.daif = 0;
        cpu.sysregs.vbar_el1 = vector_base;
    }
    orch.gic.gicd_ctlr = 1;
    orch.gic.gicc_ctlr = 1;
    orch.gic.gicc_pmr = 0xf0;
    orch.gic.gicd_isenabler0 = 1 << 27;
    orch.machine.irq.pending |= 1 << 27;

    // The IRQ vector executes HVC, taking a synchronous exception at EL1.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, sync_vector);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, irq_vector + 4);
    assert_eq!(orch.irq_entry_stack.len(), 1);

    // The first ERET returns to the IRQ handler and must leave its IRQ record
    // on the diagnostic stack. Model the handler restoring its saved pair.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, irq_vector + 4);
    assert_eq!(orch.irq_return_count(), 0);
    assert_eq!(orch.irq_entry_stack.len(), 1);
    orch.machine.cpu[0].sysregs.elr_el1 = source_pc;
    orch.machine.cpu[0].sysregs.spsr_el1 = 0x4000_0005;

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, source_pc);
    assert_eq!(orch.irq_return_count(), 1);
    assert!(orch.irq_entry_stack.is_empty());
}

#[test]
fn lower_el_instruction_abort_uses_ec_20_and_faulting_resume_pc() {
    let source = RAM_BASE + 0x2000; // L3[2] is invalid.
    let mut orch = configure_fault_orch(EL0T_MODE, source, false);
    let saved = orch.machine.cpu[0].pstate;
    assert_abort_entry(
        &mut orch,
        source,
        TEST_VBAR + 0x400,
        saved,
        0x8200_0007,
        source,
    );
}

#[test]
fn same_el_instruction_abort_uses_ec_21_and_faulting_resume_pc() {
    let source = RAM_BASE + 0x2000; // L3[2] is invalid.
    let mut orch = configure_fault_orch(EL1H_MODE, source, false);
    let saved = orch.machine.cpu[0].pstate;
    assert_abort_entry(
        &mut orch,
        source,
        TEST_VBAR + 0x200,
        saved,
        0x8600_0007,
        source,
    );
}

#[test]
fn lower_el_data_abort_uses_ec_24_syndrome_and_far_el1() {
    let source = RAM_BASE;
    let mut orch = configure_fault_orch(EL0T_MODE, source, false);
    write_word(&mut orch, source, LDR_X0_X1);
    let saved = orch.machine.cpu[0].pstate;
    assert_abort_entry(
        &mut orch,
        source,
        TEST_VBAR + 0x400,
        saved,
        0x9200_0007,
        RAM_BASE + 0x2000,
    );
}

#[test]
fn same_el_data_abort_uses_ec_25_syndrome_and_far_el1() {
    let source = RAM_BASE;
    let mut orch = configure_fault_orch(EL1H_MODE, source, false);
    write_word(&mut orch, source, LDR_X0_X1);
    let saved = orch.machine.cpu[0].pstate;
    assert_abort_entry(
        &mut orch,
        source,
        TEST_VBAR + 0x200,
        saved,
        0x9600_0007,
        RAM_BASE + 0x2000,
    );
}

#[test]
fn lower_el_write_permission_abort_encodes_wnr_and_level_three_dfsc() {
    let source = RAM_BASE;
    let mut orch = configure_fault_orch(EL0T_MODE, source, true);
    write_word(&mut orch, source, STR_X0_X1);
    let saved = orch.machine.cpu[0].pstate;
    assert_abort_entry(
        &mut orch,
        source,
        TEST_VBAR + 0x400,
        saved,
        0x9200_004F,
        RAM_BASE + 0x2000,
    );
}
