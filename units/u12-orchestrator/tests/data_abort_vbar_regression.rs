//! QEMU-matched regression vectors for ARM64 Synchronous Data Abort exception vectoring to VBAR_EL1+0x200.
//!
//! Verifies:
//! 1. Data abort on LDR / LDXR with null base vectors to VBAR_EL1 + 0x200 (el1h_sync),
//!    setting ESR_EL1 = 0x96000005 (EC=0x25, IL=1, WnR=0, DFSC=0x05 for Level 1 Translation Fault),
//!    FAR_EL1 = 0x0, saving ELR_EL1 and SPSR_EL1, masking DAIF, and entering EL1h mode.
//! 2. Data abort on STR to unmapped VA sets WnR=1 (bit 6 set) -> ESR_EL1 = 0x96000045,
//!    FAR_EL1 = faulting VA.
//! 3. Exception return (ERET) from the synchronous abort handler restores PC from ELR_EL1,
//!    PSTATE from SPSR_EL1, and restores DAIF mask.
//! 4. Fault-status-code encoding (DFSC) for Translation Fault levels 0, 1, 2, 3 matches ARM ARM & QEMU:
//!    Level 0: 0x96000004 / 0x96000044
//!    Level 1: 0x96000005 / 0x96000045
//!    Level 2: 0x96000006 / 0x96000046
//!    Level 3: 0x96000007 / 0x96000047
//! 5. Permission fault on write to read-only page produces DFSC = 0x0f (Level 3 Permission Fault)
//!    and ESR_EL1 = 0x9600004f, FAR_EL1 = faulting VA.
//! 6. Exact futex_init instruction at PC 0xffffff80082591e4 (ldxr w11, [x10] with x10=0, 0x885f7d4b)
//!    vectors to VBAR_EL1 + 0x200 with ESR_EL1 = 0x96000005 and FAR_EL1 = 0x0, matching the QEMU oracle log.

use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};
use u4_mmu::DetailedFault;

const NOP: u32 = 0xd503_201f;
const ERET: u32 = 0xd69f_03e0;
const LDXR_W11_X10: u32 = 0x885f_7d4b; // ldxr w11, [x10] (T587 / futex_init instruction)
const LDR_X8_X10: u32 = 0xf940_0148;    // ldr x8, [x10]
const STR_X8_X10: u32 = 0xf900_0148;    // str x8, [x10]
const MRS_X1_ESR: u32 = 0xd538_5201;    // mrs x1, esr_el1
const MRS_X2_FAR: u32 = 0xd538_6002;    // mrs x2, far_el1
const MSR_ELR_X3: u32 = 0xd518_4023;    // msr elr_el1, x3

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

fn write_u64(orch: &mut Orchestrator, addr: u64, val: u64) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

/// Configure 39-bit VA MMU mapping for test execution at `RAM_BASE` (PA = VA = 0x4000_0000).
/// Level 1 table at RAM_BASE + 0x10_000.
/// Entry 1 maps 1 GiB (0x4000_0000..0x8000_0000) flat block descriptor.
/// Entry 0 (VA 0x0..0x4000_0000) is unmapped (0), so any access to VA 0 triggers Level 1 Translation Fault.
fn setup_test_mmu(orch: &mut Orchestrator) {
    let l1_table_pa = RAM_BASE + 0x10_000;
    // Clear L1 table
    for i in 0..512 {
        write_u64(orch, l1_table_pa + i * 8, 0);
    }
    // L1 entry 1: block descriptor mapping 0x4000_0000..0x8000_0000
    // desc = 0x4000_0000 | 0b01 (block) | AP=00 (RW) | SH=11 (inner shareable) | AF=1 (bit 10)
    let block_desc: u64 = 0x4000_0000 | (1 << 10) | 0b01;
    write_u64(orch, l1_table_pa + 1 * 8, block_desc);

    let cpu = &mut orch.machine.cpu[0];
    cpu.sysregs.sctlr_el1 = 1; // MMU enabled
    // TCR_EL1: T0SZ=25 (39-bit VA), TG0=0 (4KB granule), T1SZ=25
    cpu.sysregs.tcr_el1 = 0x0040_0032_b559_3519;
    cpu.sysregs.ttbr0_el1 = l1_table_pa;
    cpu.sysregs.ttbr1_el1 = l1_table_pa;
}

#[test]
fn qemu_vector_1_data_abort_ldr_null_base() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200; // el1h_sync

    write_word(&mut orch, source_pc, LDR_X8_X10);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005; // EL1h, Z=1, DAIF unmasked
        cpu.regs[10] = 0x0;       // Null base address -> TranslationFault VA 0
        cpu.regs[8] = 0xdead_beef;
    }

    // Step 1: Executes LDR X8, [X10] with X10=0.
    // MMU translation fault on VA 0 triggers synchronous Data Abort to VBAR_EL1 + 0x200.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc, "PC must vector to VBAR_EL1 + 0x200");
    assert_eq!(cpu.sysregs.elr_el1, source_pc, "ELR_EL1 must hold faulting instruction PC");
    assert_eq!(cpu.sysregs.far_el1, 0x0, "FAR_EL1 must hold faulting VA (0x0)");
    // QEMU oracle: ESR = 0x96000005 (EC=0x25, IL=1, WnR=0, DFSC=0x05 for L1 Translation fault)
    assert_eq!(cpu.sysregs.esr_el1, 0x9600_0005, "ESR_EL1 must match QEMU (0x96000005)");
    assert_eq!(cpu.sysregs.spsr_el1, 0x4000_0005, "SPSR_EL1 must preserve interrupted PSTATE");
    assert_eq!(cpu.pstate & 0x3C0, 0x3C0, "DAIF must be masked on exception entry");
    assert_eq!(cpu.pstate & 0x1F, 0x05, "Mode must remain EL1h");
    assert_eq!(cpu.regs[8], 0xdead_beef, "Aborting load must not overwrite destination register");
    assert_eq!(orch.data_abort_count(), 1, "data_abort_count must increment");
}

#[test]
fn qemu_vector_2_data_abort_str_unmapped_va() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200;

    write_word(&mut orch, source_pc, STR_X8_X10);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    let unmapped_va = 0x1234_5000;
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005;
        cpu.regs[10] = unmapped_va; // Unmapped VA in user region (< 1GB)
        cpu.regs[8] = 0xcafe_babe;
    }

    // Step 1: Executes STR X8, [X10] with unmapped address.
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc, "PC must vector to VBAR_EL1 + 0x200");
    assert_eq!(cpu.sysregs.elr_el1, source_pc);
    assert_eq!(cpu.sysregs.far_el1, unmapped_va, "FAR_EL1 must hold unmapped target VA");
    // QEMU oracle: ESR = 0x96000045 (EC=0x25, IL=1, WnR=1 [bit 6=1], DFSC=0x05)
    assert_eq!(cpu.sysregs.esr_el1, 0x9600_0045, "ESR_EL1 must have WnR=1 for STR (0x96000045)");
    assert_eq!(orch.data_abort_count(), 1);
}

#[test]
fn qemu_vector_3_data_abort_eret_returns_to_fixup() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200;
    let fixup_pc = RAM_BASE + 0x3000;

    // source instruction: ldr x8, [x10] with x10=0
    write_word(&mut orch, source_pc, LDR_X8_X10);

    // exception handler at VBAR_EL1 + 0x200:
    // 1. mrs x1, esr_el1
    // 2. mrs x2, far_el1
    // 3. msr elr_el1, x3  (fixup PC)
    // 4. eret
    write_word(&mut orch, vector_pc, MRS_X1_ESR);
    write_word(&mut orch, vector_pc + 4, MRS_X2_FAR);
    write_word(&mut orch, vector_pc + 8, MSR_ELR_X3);
    write_word(&mut orch, vector_pc + 12, ERET);

    // instruction at fixup_pc: NOP
    write_word(&mut orch, fixup_pc, NOP);

    setup_test_mmu(&mut orch);

    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005; // unmasked
        cpu.regs[10] = 0x0;
        cpu.regs[3] = fixup_pc; // fixup target address
    }

    // Step 1: Takes Data Abort, vectors to vector_pc
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, vector_pc);

    // Step 2: MRS X1, ESR_EL1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[1], 0x9600_0005, "MRS X1, ESR_EL1 must read 0x96000005");

    // Step 3: MRS X2, FAR_EL1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[2], 0x0, "MRS X2, FAR_EL1 must read 0x0");

    // Step 4: MSR ELR_EL1, X3 (updates ELR to fixup_pc)
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, fixup_pc);

    // Step 5: ERET
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, fixup_pc, "ERET must return to the updated fixup_pc");
    assert_eq!(cpu.pstate, 0x4000_0005, "PSTATE must be fully restored from SPSR_EL1");
    assert_eq!(cpu.sysregs.daif, 0, "DAIF must be unmasked after ERET");
}

#[test]
fn qemu_vector_4_fault_status_code_levels_0_to_3() {
    // Verify architectural DFSC encoding matching QEMU arm_fi_to_lfsc:
    // Translation fault: 0b0001LL (LL = level) -> 0x04..=0x07
    // Read: WnR = 0 -> ISS = DFSC
    // Write: WnR = 1 -> ISS = (1 << 6) | DFSC = 0x40 | DFSC

    let read_cases = [
        (0u8, 0x9600_0004),
        (1u8, 0x9600_0005),
        (2u8, 0x9600_0006),
        (3u8, 0x9600_0007),
    ];
    for (level, expected_esr) in read_cases {
        let df = DetailedFault {
            va: 0x1000,
            is_permission: false,
            level,
        };
        let esr = Orchestrator::compute_data_abort_esr(true, false, df.dfsc());
        assert_eq!(
            esr, expected_esr,
            "Level {level} translation read fault must produce ESR {expected_esr:#010x}"
        );
    }

    let write_cases = [
        (0u8, 0x9600_0044),
        (1u8, 0x9600_0045),
        (2u8, 0x9600_0046),
        (3u8, 0x9600_0047),
    ];
    for (level, expected_esr) in write_cases {
        let df = DetailedFault {
            va: 0x1000,
            is_permission: false,
            level,
        };
        let esr = Orchestrator::compute_data_abort_esr(true, true, df.dfsc());
        assert_eq!(
            esr, expected_esr,
            "Level {level} translation write fault must produce ESR {expected_esr:#010x}"
        );
    }
}

#[test]
fn qemu_vector_5_permission_fault_read_only_page() {
    // Permission fault on Level 3 page: DFSC = 0b001111 = 0x0f
    // Write: WnR = 1 (bit 6 = 1) -> ISS = 0x4f
    // ESR_EL1 = 0x9600004f
    let df = DetailedFault {
        va: 0x4000_1000,
        is_permission: true,
        level: 3,
    };
    assert_eq!(df.dfsc(), 0x0f, "Level 3 Permission fault DFSC must be 0x0f");
    let esr_write = Orchestrator::compute_data_abort_esr(true, true, df.dfsc());
    assert_eq!(
        esr_write, 0x9600_004f,
        "Level 3 Permission fault write ESR must be 0x9600004f matching QEMU"
    );
    let esr_read = Orchestrator::compute_data_abort_esr(true, false, df.dfsc());
    assert_eq!(
        esr_read, 0x9600_000f,
        "Level 3 Permission fault read ESR must be 0x9600000f matching QEMU"
    );
}

#[test]
fn qemu_vector_6_futex_init_instruction_vectors_to_el1_sync() {
    // PC 0xffffff80082591e4 in Linux kernel Image is:
    // 0x885f7d4b: ldxr w11, [x10]
    // In futex_init, x10 is 0x0 (NULL pointer).
    // QEMU logged:
    // Taking exception 4 [Data Abort] on CPU 0
    // ...from EL1 to EL1
    // ...with ESR 0x25/0x96000005
    // ...with FAR 0x0
    // ...with SPSR 0x40000005
    // ...with ELR 0xffffff80082591e4
    // ...to EL1 PC 0xffffff8008082200 PSTATE 0x3c5

    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200;

    write_word(&mut orch, source_pc, LDXR_W11_X10);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x4000_0005; // EL1h, Z=1
        cpu.regs[10] = 0x0;       // NULL pointer
        cpu.regs[11] = 0x1234;
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc, "Must vector to el1h_sync (VBAR_EL1 + 0x200)");
    assert_eq!(cpu.sysregs.elr_el1, source_pc, "ELR_EL1 must hold faulting instruction PC");
    assert_eq!(cpu.sysregs.far_el1, 0x0, "FAR_EL1 must be 0x0 matching QEMU oracle");
    assert_eq!(cpu.sysregs.esr_el1, 0x9600_0005, "ESR_EL1 must be 0x96000005 matching QEMU oracle");
    assert_eq!(cpu.sysregs.spsr_el1, 0x4000_0005, "SPSR_EL1 must match QEMU oracle");
    assert_eq!(cpu.pstate, 0x4000_03c5, "PSTATE must be 0x3c5 with Z=1 matching QEMU oracle");
    assert_eq!(cpu.regs[11], 0x1234, "Destination register w11 must not be modified by aborting load");
    assert_eq!(orch.data_abort_count(), 1);
}
