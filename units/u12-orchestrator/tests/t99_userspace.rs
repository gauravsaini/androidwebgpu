use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const EL0T: u64 = 0b00000;
const EL1H: u64 = 0b00101;
const ERET: u32 = 0xd69f_03e0;
const MRS_ESR_X0: u32 = 0xd538_5200;
const MRS_ESR_X2: u32 = 0xd538_5202;
const MRS_FAR_X3: u32 = 0xd538_6003;

fn write_word(orch: &mut Orchestrator, address: u64, word: u32) {
    let offset = (address - RAM_BASE) as usize;
    orch.machine_mut().ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn put_bytes(orch: &mut Orchestrator, address: u64, bytes: &[u8]) {
    let offset = (address - RAM_BASE) as usize;
    orch.machine_mut().ram[offset..offset + bytes.len()].copy_from_slice(bytes);
}

#[test]
fn el0_svc_takes_lower_sync_vector_and_saves_qemu_syndrome_and_stack() {
    let mut orch = Orchestrator::new();
    let svc_pc = RAM_BASE + 0x10_000;
    let vector_base = RAM_BASE + 0x20_000;
    let vector_pc = vector_base + 0x400;
    let sp_el0 = RAM_BASE + 0x30_000;
    let sp_el1 = RAM_BASE + 0x40_000;

    write_word(&mut orch, svc_pc, 0xd402_4681); // SVC #0x1234
    write_word(&mut orch, vector_pc, MRS_ESR_X0);
    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = svc_pc;
        cpu.pstate = 0xa000_0000 | EL0T;
        cpu.sp = sp_el0;
        cpu.sp_el1 = sp_el1;
        cpu.sysregs.daif = 0;
        cpu.sysregs.vbar_el1 = vector_base;
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine().cpu[0];
    assert_eq!(cpu.pc, vector_pc);
    assert_eq!(cpu.sysregs.elr_el1, svc_pc + 4);
    assert_eq!(cpu.sysregs.spsr_el1, 0xa000_0000 | EL0T);
    assert_eq!(cpu.pstate, 0xa000_03c5);
    assert_eq!(cpu.sp, sp_el1);
    assert_eq!(cpu.sysregs.sp_el0, sp_el0);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[0], 0x5600_1234);
}

#[test]
fn eret_restores_el0_state_and_el0_eret_takes_undefined_vector() {
    let mut orch = Orchestrator::new();
    let eret_el1_pc = RAM_BASE + 0x10_000;
    let el0_pc = RAM_BASE + 0x11_000;
    let vector_base = RAM_BASE + 0x20_000;
    let vector_pc = vector_base + 0x400;
    let sp_el0 = RAM_BASE + 0x30_000;
    let sp_el1 = RAM_BASE + 0x40_000;
    let el0_pstate = 0xa000_0000 | EL0T;

    write_word(&mut orch, eret_el1_pc, ERET);
    write_word(&mut orch, el0_pc, ERET);
    write_word(&mut orch, vector_pc, MRS_ESR_X0);
    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = eret_el1_pc;
        cpu.pstate = 0x4000_0000 | EL1H;
        cpu.sp = sp_el1;
        cpu.sp_el1 = sp_el1;
        cpu.sysregs.sp_el0 = sp_el0;
        cpu.sysregs.daif = 0;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.elr_el1 = el0_pc;
        cpu.sysregs.spsr_el1 = el0_pstate;
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine().cpu[0];
    assert_eq!(cpu.pc, el0_pc);
    assert_eq!(cpu.pstate, el0_pstate);
    assert_eq!(cpu.sp, sp_el0);
    assert_eq!(cpu.sp_el1, sp_el1);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine().cpu[0];
    assert_eq!(cpu.pc, vector_pc);
    assert_eq!(cpu.sysregs.elr_el1, el0_pc);
    assert_eq!(cpu.sysregs.spsr_el1, el0_pstate);
    assert_eq!(cpu.pstate, 0xa000_03c5);
    assert_eq!(cpu.sp, sp_el1);
    assert_eq!(cpu.sysregs.sp_el0, sp_el0);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[0], 0x0200_0000);
}

#[test]
fn q_pair_load_store_preserves_order_and_qemu_alignment_faults() {
    let mut orch = Orchestrator::new();
    let code = RAM_BASE + 0x10_000;
    let vector_base = RAM_BASE + 0x20_000;
    let vector_pc = vector_base + 0x200;
    let data = RAM_BASE + 0x30_000;
    let output = RAM_BASE + 0x31_000;

    // LDP Q0, Q1, [X0]; STP Q0, Q1, [X1, #16]!
    write_word(&mut orch, code, 0xad40_0400);
    write_word(&mut orch, code + 4, 0xad80_8420);
    // Deliberately unaligned accesses: QEMU reports FSC=0x21 for both.
    write_word(&mut orch, code + 8, 0xad40_0c02); // LDP Q2, Q3, [X0]
    write_word(&mut orch, code + 12, 0xad00_0c02); // STP Q2, Q3, [X0]
    write_word(&mut orch, vector_pc, MRS_ESR_X2);
    write_word(&mut orch, vector_pc + 4, MRS_FAR_X3);
    write_word(&mut orch, vector_pc + 8, ERET);
    put_bytes(&mut orch, data, &(0..40u8).collect::<Vec<_>>());

    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = code;
        cpu.pstate = 0x4000_0000 | EL1H;
        cpu.regs[0] = data;
        cpu.regs[1] = output;
        cpu.sysregs.cpacr_el1 = 0x0030_0000;
        cpu.sysregs.sctlr_el1 = 0;
        cpu.sysregs.vbar_el1 = vector_base;
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[1], output + 16);
    let output_offset = (output + 16 - RAM_BASE) as usize;
    assert_eq!(
        &orch.machine().ram[output_offset..output_offset + 32],
        &orch.machine().ram[(data - RAM_BASE) as usize..(data - RAM_BASE) as usize + 32]
    );

    let unaligned = data + 1;
    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = code + 8;
        cpu.regs[0] = unaligned;
        cpu.sysregs.sctlr_el1 = 0;
    }
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].pc, vector_pc);
    assert_eq!(orch.machine().cpu[0].sysregs.elr_el1, code + 8);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[2], 0x9600_0021);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[3], unaligned);

    // Let the probe resume after the rejected load, then check the store path.
    orch.machine_mut().cpu[0].sysregs.elr_el1 = code + 12;
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].pc, code + 12);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].pc, vector_pc);
    assert_eq!(orch.machine().cpu[0].sysregs.elr_el1, code + 12);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[2], 0x9600_0061);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].regs[3], unaligned);
}

#[test]
fn cpacr_el1_zero_traps_fp_simd_with_ec_07() {
    let mut orch = Orchestrator::new();
    let fp_pc = RAM_BASE + 0x10_000;
    let vector_base = RAM_BASE + 0x20_000;
    let vector_pc = vector_base + 0x200;

    write_word(&mut orch, fp_pc, 0x1e22_2820); // FADD S0, S1, S2
    write_word(&mut orch, vector_pc, MRS_ESR_X0);
    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = fp_pc;
        cpu.pstate = 0x4000_0000 | EL1H;
        cpu.sysregs.cpacr_el1 = 0;
        cpu.sysregs.vbar_el1 = vector_base;
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine().cpu[0].pc, vector_pc);
    assert_eq!(orch.machine().cpu[0].sysregs.elr_el1, fp_pc);
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let esr = orch.machine().cpu[0].regs[0];
    assert_eq!((esr >> 26) & 0x3f, 0x07);
    assert_eq!((esr >> 25) & 1, 1);
}
