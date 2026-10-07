use super::*;

const TIMER_PPI27: u32 = 1 << 27;
const TEST_WFI_ADDR: u64 = 0xffff_ff80_0808_4eb8;
const T63_WFI_STEP: u64 = 46_436_324;

const STR_W2_X1: u32 = 0xb900_0022;
const LDR_W0_X3: u32 = 0xb940_0060;

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
