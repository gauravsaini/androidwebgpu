use super::*;

const TEST_PC: u64 = 0x4008_0000;
const VBAR: u64 = TEST_PC;
const TTBR0: u64 = RAM_BASE + 0x1000;

fn faulting_ldxr_machine() -> MachineState {
    let mut ram = vec![0; 0x10_0000];
    // T0SZ=25 selects a 39-bit VA with an L1 root. Keep VA 0 invalid and map
    // the code/vector's 1 GiB block back onto the test RAM window.
    let l1 = (TTBR0 - RAM_BASE) as usize;
    ram[l1 + 8..l1 + 16].copy_from_slice(&(RAM_BASE | 0x401).to_le_bytes());
    let pc_offset = (TEST_PC - RAM_BASE) as usize;
    ram[pc_offset..pc_offset + 4].copy_from_slice(&0x885f_7d4bu32.to_le_bytes());

    let mut sysregs = SysRegs::default();
    sysregs.sctlr_el1 = 0x34f5_d91d;
    sysregs.tcr_el1 = 0x0040_0032_b559_3519;
    sysregs.ttbr0_el1 = TTBR0;
    sysregs.ttbr1_el1 = RAM_BASE + 0x2000;
    sysregs.vbar_el1 = VBAR;

    MachineState {
        cpu: vec![CpuState {
            qregs: [0; 32],
            regs: {
                let mut regs = [0; 31];
                regs[10] = 0; // LDXR W11, [X10] faults at VA 0.
                regs
            },
            sp: RAM_BASE + 0x80000,
            sp_el1: RAM_BASE + 0x80000,
            pc: TEST_PC,
            pstate: EL1H_MODE,
            sysregs,
        }],
        mmu: MmuState {
            ttbr0: TTBR0,
            ttbr1: RAM_BASE + 0x2000,
            tcr: 0x0040_0032_b559_3519,
            sctlr: 0x34f5_d91d,
        },
        irq: IrqState {
            enabled: 0,
            pending: 0,
            timer_count: 0,
            timer_compare: 0,
        },
        ram,
        devices: vec![],
    }
}

fn wasm_faulting_literal_machine() -> MachineState {
    let mut ram = vec![0; 0x10_0000];
    // Map VA 0x4000 (code and vectors) through L1/L2 tables to the first
    // physical RAM page. Leave VA 0 unmapped at L3 for the literal load.
    let l1 = (TTBR0 - RAM_BASE) as usize;
    ram[l1..l1 + 8].copy_from_slice(&(RAM_BASE + 0x2000 | 0x3).to_le_bytes());
    let l2 = (RAM_BASE + 0x2000 - RAM_BASE) as usize;
    ram[l2..l2 + 8].copy_from_slice(&(RAM_BASE + 0x3000 | 0x3).to_le_bytes());
    let l3 = (RAM_BASE + 0x3000 - RAM_BASE) as usize;
    ram[l3 + 4 * 8..l3 + 5 * 8].copy_from_slice(&(RAM_BASE | 0x403).to_le_bytes());
    // LDR W0, literal at PC 0x4000 with imm19=-4096 reads VA 0.
    ram[0..4].copy_from_slice(&0x18fe_0000u32.to_le_bytes());

    let mut sysregs = SysRegs::default();
    sysregs.sctlr_el1 = 0x34f5_d91d;
    sysregs.tcr_el1 = 0x0040_0032_b559_3519;
    sysregs.ttbr0_el1 = TTBR0;
    sysregs.ttbr1_el1 = RAM_BASE + 0x4000;
    sysregs.vbar_el1 = 0x4000;

    MachineState {
        cpu: vec![CpuState {
            qregs: [0; 32],
            regs: [0; 31],
            sp: RAM_BASE + 0x80000,
            sp_el1: RAM_BASE + 0x80000,
            pc: 0x4000,
            pstate: EL1H_MODE,
            sysregs,
        }],
        mmu: MmuState {
            ttbr0: TTBR0,
            ttbr1: RAM_BASE + 0x4000,
            tcr: 0x0040_0032_b559_3519,
            sctlr: 0x34f5_d91d,
        },
        irq: IrqState {
            enabled: 0,
            pending: 0,
            timer_count: 0,
            timer_compare: 0,
        },
        ram,
        devices: vec![],
    }
}

#[test]
fn va0_translation_fault_enters_el1_data_abort_vector() {
    let mut orch = Orchestrator::with_machine(faulting_ldxr_machine());

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, VBAR + 0x200);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, TEST_PC);
    assert_eq!(orch.machine.cpu[0].sysregs.spsr_el1, 0x3c5);
    assert_eq!(orch.machine.cpu[0].sysregs.esr_el1, 0x9600_0005);
    assert_eq!(orch.machine.cpu[0].sysregs.far_el1, 0);
    assert_eq!(orch.machine.cpu[0].sysregs.ttbr0_el1, TTBR0);
    assert_eq!(orch.machine.cpu[0].sysregs.ttbr1_el1, RAM_BASE + 0x2000);
    assert_eq!(orch.machine.cpu[0].sysregs.tcr_el1, 0x0040_0032_b559_3519);
}

#[test]
fn wasm_memory_fault_enters_el1_data_abort_vector() {
    let mut orch = Orchestrator::with_machine(wasm_faulting_literal_machine());

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, 0x4200);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, 0x4000);
    assert_eq!(orch.machine.cpu[0].sysregs.spsr_el1, 0x3c5);
    assert_eq!(orch.machine.cpu[0].sysregs.esr_el1, 0x9600_0007);
    assert_eq!(orch.machine.cpu[0].sysregs.far_el1, 0);
}

#[test]
fn instruction_translation_fault_enters_el1_instruction_abort_vector() {
    let mut machine = faulting_ldxr_machine();
    machine.cpu[0].pc = 0; // VA 0 is invalid in TTBR0's level-1 table.
    let mut orch = Orchestrator::with_machine(machine);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, VBAR + 0x200);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, 0);
    assert_eq!(orch.machine.cpu[0].sysregs.spsr_el1, 0x3c5);
    assert_eq!(orch.machine.cpu[0].sysregs.esr_el1, 0x8600_0005);
    assert_eq!(orch.machine.cpu[0].sysregs.far_el1, 0);
}

#[test]
fn instruction_permission_fault_enters_el1_instruction_abort_vector() {
    let mut machine = faulting_ldxr_machine();
    let l1 = (TTBR0 - RAM_BASE) as usize;
    let descriptor = u64::from_le_bytes(machine.ram[l1 + 8..l1 + 16].try_into().unwrap());
    machine.ram[l1 + 8..l1 + 16]
        .copy_from_slice(&(descriptor | (1 << 53)).to_le_bytes()); // PXN denies EL1 execute.
    let mut orch = Orchestrator::with_machine(machine);

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, VBAR + 0x200);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, TEST_PC);
    assert_eq!(orch.machine.cpu[0].sysregs.spsr_el1, 0x3c5);
    assert_eq!(orch.machine.cpu[0].sysregs.esr_el1, 0x8600_000d);
    assert_eq!(orch.machine.cpu[0].sysregs.far_el1, TEST_PC);
}
