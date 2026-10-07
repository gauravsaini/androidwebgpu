use super::*;

const TIMER_PPI27: u32 = 1 << 27;
const TEST_WFI_ADDR: u64 = 0xffff_ff80_0808_4eb8;
const T63_WFI_STEP: u64 = 46_436_324;

const STR_W2_X1: u32 = 0xb900_0022;
const LDR_W0_X3: u32 = 0xb940_0060;
const NOP: u32 = 0xd503_201f;
const ERET: u32 = 0xd69f_03e0;

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn make_irq_deliverable(orch: &mut Orchestrator) {
    orch.gic.gicd_ctlr = 1;
    orch.gic.gicc_ctlr = 1;
    orch.gic.gicc_pmr = 0xf0;
    orch.gic.gicd_isenabler0 = TIMER_PPI27;
    orch.machine.irq.pending |= TIMER_PPI27 as u64;
}

fn write_ppi_enable_from_guest(orch: &mut Orchestrator) {
    let old_word = orch.machine.ram[..4].to_vec();
    orch.machine.ram[..4].copy_from_slice(&STR_W2_X1.to_le_bytes());
    orch.machine.cpu[0].sysregs.sctlr_el1 = 0;
    orch.machine.cpu[0].pc = RAM_BASE;
    orch.machine.cpu[0].regs[1] = GICD_BASE + 0x100;
    orch.machine.cpu[0].regs[2] = TIMER_PPI27 as u64;
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    orch.machine.ram[..4].copy_from_slice(&old_word);
}

#[test]
fn virtual_timer_compare_pends_ppi27_and_gicc_iar_acknowledges_it() {
    let mut orch = Orchestrator::new();
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 1;
    orch.machine.cpu[0].sysregs.cntv_cval_el0 = TIMER_CYCLES_PER_STEP;
    orch.gic.gicd_ctlr = 1;
    orch.gic.gicc_ctlr = 1;
    orch.gic.gicc_pmr = 0xf0;
    orch.machine.ram[0..4].copy_from_slice(&STR_W2_X1.to_le_bytes());
    orch.machine.ram[4..8].copy_from_slice(&LDR_W0_X3.to_le_bytes());
    orch.machine.cpu[0].regs[1] = GICD_BASE + 0x100;
    orch.machine.cpu[0].regs[2] = TIMER_PPI27 as u64;
    orch.machine.cpu[0].regs[3] = GICC_BASE + 0x0c;

    orch.tick_clock(TIMER_CYCLES_PER_STEP);
    assert_eq!(orch.pending_irqs(), vec![27]);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "enable timer PPI");
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "read GICC_IAR");
    assert_eq!(orch.machine.cpu[0].regs[0], 27);
}

#[test]
fn el1h_irq_enters_vector_saves_state_and_returns_through_eret() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE;
    let vector_base = RAM_BASE + 0x1000;
    let vector_pc = vector_base + 0x280;
    write_word(&mut orch, source_pc, NOP);
    write_word(&mut orch, vector_pc, NOP);
    write_word(&mut orch, vector_pc + 4, ERET);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = 0x4000_1000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005; // Z set, EL1h, IRQ unmasked.
    }
    make_irq_deliverable(&mut orch);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc + 4);
    assert_eq!(cpu.sysregs.elr_el1, source_pc);
    assert_eq!(cpu.sysregs.spsr_el1, 0x4000_0005);
    assert_eq!(cpu.pstate, 0x4000_03c5); // EL1h with DAIF masked on entry.
    assert_eq!(cpu.sp, 0x4000_1000); // EL1h continues on SP_EL1.
    assert_eq!(orch.irq_entry_count(), 1);
    assert_eq!(orch.irq_entry_log()[0].vector_pc, vector_pc);
    assert_eq!(orch.irq_entry_log()[0].source_mode, EL1H_MODE as u8);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue); // ERET
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, source_pc);
    assert_eq!(cpu.pstate, 0x4000_0005);
    assert_eq!(cpu.sysregs.daif, 0);
    assert_eq!(orch.irq_return_count(), 1);
    assert_eq!(orch.irq_entry_log()[0].return_step, Some(2));
}

#[test]
fn el1t_irq_uses_sp0_vector_and_switches_to_banked_el1_stack() {
    let mut orch = Orchestrator::new();
    let vector_base = RAM_BASE + 0x1000;
    let vector_pc = vector_base + 0x80;
    write_word(&mut orch, RAM_BASE, NOP);
    write_word(&mut orch, vector_pc, NOP);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = RAM_BASE;
        cpu.sp = 0x1111_0000; // Active SP_EL0 while the source mode is EL1t.
        cpu.sp_el1 = 0x2222_0000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = EL1T_MODE;
    }
    make_irq_deliverable(&mut orch);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc + 4);
    assert_eq!(cpu.pstate & PSTATE_MODE_MASK, EL1H_MODE);
    assert_eq!(cpu.sysregs.sp_el0, 0x1111_0000);
    assert_eq!(cpu.sp, 0x2222_0000);
    assert_eq!(orch.irq_entry_log()[0].vector_pc, vector_pc);
}

#[test]
fn masked_irq_does_not_take_exception() {
    let mut orch = Orchestrator::new();
    write_word(&mut orch, RAM_BASE, NOP);
    orch.machine.cpu[0].pc = RAM_BASE;
    orch.machine.cpu[0].pstate = EL1H_MODE | PSTATE_IRQ_MASK;
    orch.machine.cpu[0].sysregs.daif = PSTATE_IRQ_MASK;
    make_irq_deliverable(&mut orch);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 4);
    assert_eq!(orch.irq_entry_count(), 0);
}

#[test]
fn nested_irq_unwinds_through_eret_after_guest_restores_outer_state() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE;
    let vector_base = RAM_BASE + 0x1000;
    let vector_pc = vector_base + 0x280;
    write_word(&mut orch, source_pc, NOP);
    write_word(&mut orch, vector_pc, NOP);
    write_word(&mut orch, vector_pc + 4, ERET);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.cntv_ctl_el0 = 1;
        cpu.sysregs.cntv_cval_el0 = 0;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005;
    }
    make_irq_deliverable(&mut orch);

    // First IRQ takes the vector and runs its first instruction.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, vector_pc + 4);
    let outer_elr = orch.machine.cpu[0].sysregs.elr_el1;
    let outer_spsr = orch.machine.cpu[0].sysregs.spsr_el1;

    // The guest has saved ELR/SPSR to its frame and unmasks IRQs. The still
    // pending line now takes a nested IRQ, which overwrites the live EL1 pair.
    orch.machine.cpu[0].sysregs.daif = 0;
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.irq_entry_count(), 2);
    assert_eq!(orch.machine.cpu[0].pc, vector_pc + 4);

    // Model the nested handler acknowledging the source and returning to the
    // interrupted vector instruction; then restore the outer exception pair
    // from the guest's saved frame before the outer ERET.
    orch.machine.irq.pending = 0;
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, vector_pc + 4);
    assert_eq!(orch.irq_return_count(), 1);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.sysregs.cntv_ctl_el0 = 0;
        cpu.sysregs.elr_el1 = outer_elr;
        cpu.sysregs.spsr_el1 = outer_spsr;
    }
    orch.machine.irq.pending = 0;
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, source_pc);
    assert_eq!(orch.irq_return_count(), 2);
    assert_eq!(orch.irq_entry_log()[0].return_step, Some(4));
    assert_eq!(orch.irq_entry_log()[1].return_step, Some(3));
}

#[test]
fn timer_wfi_wake_runs_gic_handler_and_returns_via_eret() {
    const GICC_BASE: u64 = 0x0801_0000;
    const EL1H_VECTOR_BASE: u64 = RAM_BASE + 0x1000;
    const EL1H_IRQ_VECTOR: u64 = EL1H_VECTOR_BASE + 0x280;
    const WFI: u32 = 0xd503_207f;
    const LDR_W0_X1_12: u32 = 0xb940_0c20; // LDR W0, [X1, #12] = GICC_IAR
    const STR_W0_X1_16: u32 = 0xb900_1020; // STR W0, [X1, #16] = GICC_EOIR

    let mut orch = Orchestrator::new();
    write_word(&mut orch, RAM_BASE, WFI);
    write_word(&mut orch, RAM_BASE + 4, NOP);
    write_word(&mut orch, EL1H_IRQ_VECTOR, LDR_W0_X1_12);
    write_word(&mut orch, EL1H_IRQ_VECTOR + 4, STR_W0_X1_16);
    write_word(&mut orch, EL1H_IRQ_VECTOR + 8, ERET);
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = RAM_BASE;
        cpu.pstate = 0x4000_0005; // EL1h, IRQ unmasked.
        cpu.sysregs.daif = 0;
        cpu.sysregs.vbar_el1 = EL1H_VECTOR_BASE;
        cpu.sysregs.cntv_ctl_el0 = 1;
        cpu.sysregs.cntv_cval_el0 = TIMER_CYCLES_PER_STEP;
        cpu.regs[1] = GICC_BASE;
    }
    orch.gic.gicd_ctlr = 1;
    orch.gic.gicc_ctlr = 1;
    orch.gic.gicc_pmr = 0xf0;
    orch.gic.gicd_isenabler0 = TIMER_PPI27;

    // WFI advances the timer to its compare and leaves the next PC as ELR.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 4);
    assert_eq!(orch.pending_irqs(), vec![27]);
    // The compare has fired; keep it from reasserting after IAR acknowledges it.
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 0;

    // Exception entry executes the EL1h vector, then the guest acknowledges,
    // EOIs the timer PPI, and returns to the instruction after WFI via ERET.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, EL1H_IRQ_VECTOR + 4);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, RAM_BASE + 4);
    assert_eq!(orch.irq_entry_count(), 1);
    assert_eq!(orch.irq_entry_log()[0].vector_pc, EL1H_IRQ_VECTOR);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue); // GICC_EOIR
    assert_eq!(orch.machine.cpu[0].regs[0], 27); // GICC_IAR returned PPI 27.
    assert!(orch.pending_irqs().is_empty());
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue); // ERET
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 4);
    assert_eq!(orch.irq_return_count(), 1);
    assert_eq!(orch.irq_entry_log()[0].return_step, Some(4));

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue); // resumed guest
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 8);
}

#[test]
#[ignore = "uses the saved T63 boot snapshot at the shared guest-input path"]
fn t63_boot_wfi_wakes_and_advances_past_the_saved_wfi_step() {
    let snapshot = std::fs::read("/Users/Shared/codex-logs/track63-initboot.snap")
        .expect("T63 WFI checkpoint exists");
    let saved_step = u64::from_le_bytes(snapshot[..8].try_into().unwrap());
    assert_eq!(saved_step, T63_WFI_STEP);

    let machine = u11_snapshot::restore(&snapshot[8..]).expect("T63 machine snapshot restores");
    let mut orch = Orchestrator::new();
    orch.machine = machine;
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 0;
    orch.gic.gicd_ctlr = 1;
    orch.gic.gicc_ctlr = 1;
    orch.gic.gicc_pmr = 0xf0;
    let sctlr = orch.machine.cpu[0].sysregs.sctlr_el1;
    write_ppi_enable_from_guest(&mut orch);
    orch.machine.cpu[0].sysregs.sctlr_el1 = sctlr;
    orch.machine.cpu[0].pc = TEST_WFI_ADDR;
    orch.steps = saved_step - 1;
    let count = orch.machine.cpu[0].sysregs.cntpct_el0;
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 1;
    orch.machine.cpu[0].sysregs.cntv_cval_el0 = count + TIMER_CYCLES_PER_STEP;

    assert_eq!(
        orch.step_vcpu(),
        StepOutcome::Continue,
        "WFI must wait for the timer PPI"
    );
    assert_eq!(orch.steps(), T63_WFI_STEP);
    let next = orch.step_vcpu();
    println!(
        "T63_WFI_WAKE saved_step={saved_step} after_wfi={} after_resume={} pc={:#018x} outcome={next:?}",
        T63_WFI_STEP,
        orch.steps(),
        orch.machine.cpu[0].pc
    );
    assert!(
        orch.steps() > T63_WFI_STEP,
        "halt_step stayed at {T63_WFI_STEP} after the timer compare became pending"
    );
}
