//! QEMU-matched regression vectors for ARM64 IRQ exception injection to VBAR_EL1.
//!
//! Verifies:
//! 1. Virtual timer compare pends PPI 27 and GICC_IAR acknowledges it.
//! 2. EL1h IRQ vectors to VBAR_EL1 + 0x280 (el1_irq), saving ELR_EL1 and SPSR_EL1,
//!    masking DAIF, and returning via ERET to the interrupted PC with restored PSTATE.
//! 3. PSTATE.I mask inhibits IRQ exception entry.
//! 4. WFI wakes when virtual timer compare fires, vectors to VBAR_EL1 + 0x280, and returns.
//! 5. QEMU oracle exception semantics: ESR_EL1 and FAR_EL1 are NOT clobbered by IRQ entry.
//! 6. Level-triggered virtual timer deassertion when CNTV_CVAL_EL0 is advanced or masked.

use u12_orchestrator::{
    Orchestrator, StepOutcome, GICC_BASE, GICD_BASE, RAM_BASE, TIMER_CYCLES_PER_STEP,
};

const TIMER_PPI27: u32 = 1 << 27;
const NOP: u32 = 0xd503_201f;
const ERET: u32 = 0xd69f_03e0;
const WFI: u32 = 0xd503_207f;
const STR_W2_X1: u32 = 0xb900_0022; // STR W2, [X1]
const LDR_W0_X3: u32 = 0xb940_0060; // LDR W0, [X3]

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn configure_gic_for_timer(orch: &mut Orchestrator) {
    orch.gic.gicd_ctlr = 1;
    orch.gic.gicc_ctlr = 1;
    orch.gic.gicc_pmr = 0xf0;
    orch.gic.gicd_isenabler0 |= TIMER_PPI27;
}

#[test]
fn qemu_vector_1_virtual_timer_compare_pends_ppi27_and_gicc_iar_acknowledges() {
    let mut orch = Orchestrator::new();
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 1;
    orch.machine.cpu[0].sysregs.cntv_cval_el0 = TIMER_CYCLES_PER_STEP;
    configure_gic_for_timer(&mut orch);

    // Setup code at RAM_BASE to enable timer PPI in GICD and read GICC_IAR
    write_word(&mut orch, RAM_BASE, STR_W2_X1);
    write_word(&mut orch, RAM_BASE + 4, LDR_W0_X3);
    orch.machine.cpu[0].regs[1] = GICD_BASE + 0x100;
    orch.machine.cpu[0].regs[2] = TIMER_PPI27 as u64;
    orch.machine.cpu[0].regs[3] = GICC_BASE + 0x0c;

    orch.tick_clock(TIMER_CYCLES_PER_STEP);
    assert_eq!(orch.pending_irqs(), vec![27]);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "enable timer PPI");
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue, "read GICC_IAR");
    assert_eq!(orch.machine.cpu[0].regs[0], 27, "GICC_IAR must return INTID 27");
    assert_eq!(orch.gic.gicc_active, Some(27), "GICC active state must be 27");
}

#[test]
fn qemu_vector_2_el1h_irq_vectors_to_vbar_el1_0x280_and_erets() {
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
        cpu.sp = 0x4000_8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005; // Z set, EL1h, IRQ unmasked
    }
    configure_gic_for_timer(&mut orch);
    orch.machine.irq.pending |= TIMER_PPI27 as u64;

    // Step 1: Takes IRQ exception to vector_pc and executes the NOP there
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc + 4, "PC must advance past vector NOP");
    assert_eq!(cpu.sysregs.elr_el1, source_pc, "ELR_EL1 must hold interrupted PC");
    assert_eq!(cpu.sysregs.spsr_el1, 0x4000_0005, "SPSR_EL1 must hold interrupted PSTATE");
    assert_eq!(cpu.pstate & 0x3C0, 0x3C0, "DAIF must be all-masked on EL1 entry");
    assert_eq!(cpu.pstate & 0x1F, 0x05, "PSTATE.M must be EL1h on entry");
    assert_eq!(orch.irq_entry_count(), 1, "irq_entry_count must increment");

    // Step 2: Executes ERET at vector_pc + 4
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, source_pc, "ERET must return to source_pc");
    assert_eq!(cpu.pstate, 0x4000_0005, "PSTATE must be fully restored from SPSR_EL1");
    assert_eq!(cpu.sysregs.daif, 0, "DAIF must be unmasked after ERET");
    assert_eq!(orch.irq_return_count(), 1, "irq_return_count must increment");
}

#[test]
fn qemu_vector_3_masked_daif_i_inhibits_irq_exception() {
    let mut orch = Orchestrator::new();
    write_word(&mut orch, RAM_BASE, NOP);

    orch.machine.cpu[0].pc = RAM_BASE;
    orch.machine.cpu[0].pstate = 0x0000_0085; // DAIF.I masked (bit 7), EL1h
    orch.machine.cpu[0].sysregs.daif = 0x80;
    configure_gic_for_timer(&mut orch);
    orch.machine.irq.pending |= TIMER_PPI27 as u64;

    // Step should NOT vector to VBAR_EL1; it executes NOP at RAM_BASE normally
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 4);
    assert_eq!(orch.irq_entry_count(), 0, "No IRQ must be taken when masked");
}

#[test]
fn qemu_vector_4_wfi_wakes_on_timer_and_vectors_to_el1_irq() {
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x280;

    let mut orch = Orchestrator::new();
    write_word(&mut orch, RAM_BASE, WFI);
    write_word(&mut orch, RAM_BASE + 4, NOP);
    write_word(&mut orch, vector_pc, NOP);
    write_word(&mut orch, vector_pc + 4, ERET);

    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = RAM_BASE;
        cpu.pstate = 0x0000_0005; // EL1h, unmasked
        cpu.sysregs.daif = 0;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.cntv_ctl_el0 = 1; // timer enabled
        cpu.sysregs.cntv_cval_el0 = 10;
    }
    configure_gic_for_timer(&mut orch);

    // Step 1: WFI executes, detects pending virtual timer expiry in 10 cycles,
    // advances clock to expiry, sets pending bit, and wakes up.
    let outcome1 = orch.step_vcpu();
    assert_eq!(outcome1, StepOutcome::Continue, "WFI must wake on timer expiry");
    assert_eq!(orch.pending_irqs(), vec![27], "PPI 27 must be pending");

    // Step 2: Next step sees deliverable IRQ, vectors to VBAR_EL1 + 0x280, executes NOP
    let outcome2 = orch.step_vcpu();
    assert_eq!(outcome2, StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, vector_pc + 4);
    assert_eq!(orch.irq_entry_count(), 1);

    // Step 3: Executes ERET in handler
    let outcome3 = orch.step_vcpu();
    assert_eq!(outcome3, StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, RAM_BASE + 4, "Must return to instruction after WFI");
    assert_eq!(orch.irq_return_count(), 1);
}

#[test]
fn qemu_vector_5_esr_el1_and_far_el1_unmodified_by_irq_entry() {
    let mut orch = Orchestrator::new();
    let vector_base = RAM_BASE + 0x1000;
    let vector_pc = vector_base + 0x280;

    write_word(&mut orch, RAM_BASE, NOP);
    write_word(&mut orch, vector_pc, ERET);

    const CANARY_ESR: u64 = 0x9200_004f;
    const CANARY_FAR: u64 = 0xffff_ff80_0812_3456;

    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = RAM_BASE;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x0000_0005;
        // Pre-populate ESR_EL1 and FAR_EL1 with canaries
        cpu.sysregs.esr_el1 = CANARY_ESR;
        cpu.sysregs.far_el1 = CANARY_FAR;
    }
    configure_gic_for_timer(&mut orch);
    orch.machine.irq.pending |= TIMER_PPI27 as u64;

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.sysregs.esr_el1, CANARY_ESR, "ARM ARM: IRQ must NOT modify ESR_EL1");
    assert_eq!(cpu.sysregs.far_el1, CANARY_FAR, "ARM ARM: IRQ must NOT modify FAR_EL1");
}

#[test]
fn qemu_vector_6_level_triggered_timer_deassertion() {
    let mut orch = Orchestrator::new();
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 1; // enabled, unmasked
    orch.machine.cpu[0].sysregs.cntv_cval_el0 = 5;

    orch.tick_clock(5);
    assert_eq!(orch.pending_irqs(), vec![27], "Timer must assert when count reaches cval");

    // Guest ISR masks the timer by setting IMASK (bit 1) in CNTV_CTL_EL0
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 3; // ENABLE=1, IMASK=1
    orch.tick_clock(1);
    assert!(
        orch.pending_irqs().is_empty(),
        "Level-triggered timer must deassert when IMASK is set"
    );

    // Guest ISR re-arms with a future CVAL and unmasks
    orch.machine.cpu[0].sysregs.cntv_cval_el0 = 100;
    orch.machine.cpu[0].sysregs.cntv_ctl_el0 = 1; // ENABLE=1, IMASK=0
    orch.tick_clock(1);
    assert!(
        orch.pending_irqs().is_empty(),
        "Timer must stay deasserted while count < new CVAL"
    );
}
