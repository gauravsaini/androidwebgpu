//! QEMU-matched regression vectors for ARM64 Architectural BRK Debug Exception Vectoring.
//!
//! Verifies:
//! 1. BRK at EL1 with immediate 0x800 (the exact T592 wall):
//!    Vectors to VBAR_EL1 + 0x200 (el1h_sync).
//!    ESR_EL1 = 0xF200_0800 (EC=0x3C, IL=1, ISS=0x800).
//!    ELR_EL1 = faulting PC.
//!    SPSR_EL1 = interrupted PSTATE.
//!    DAIF masked (0x3C0 set).
//!    FAR_EL1 left untouched (debug exception has no fault address; matches QEMU).
//! 2. BRK at EL0 with immediate 0x1 (standard BUG path):
//!    Vectors to VBAR_EL1 + 0x400 (el0_sync), not 0x200.
//!    ESR_EL1 = 0xF200_0001 (EC=0x3C, IL=1, ISS=0x1).
//!    ELR_EL1 = faulting PC.
//!    SPSR_EL1 = interrupted EL0 PSTATE (0x00).
//!    Target mode enters EL1h (0x05) with DAIF masked.
//! 3. BRK with immediate 0x0:
//!    Verify imm16 ISS encoding: ESR_EL1 = 0xF200_0000 (EC=0x3C, IL=1, ISS=0x0).
//!    Vectors to VBAR_EL1 + 0x200.
//! 4. Kernel-mode BRK with DAIF set:
//!    Interrupted PSTATE has DAIF bits set; SPSR_EL1 preserves them accurately.
//!    On entry, all DAIF bits are masked (0x3C0 set).
//! 5. Exception return (ERET) from BRK handler:
//!    Restores PC from ELR_EL1, restores PSTATE and DAIF from SPSR_EL1.

use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const NOP: u32 = 0xd503_201f;
const ERET: u32 = 0xd69f_03e0;

// BRK encodings: 0xd4200000 | (imm16 << 5)
const BRK_0X800: u32 = 0xd421_0000; // brk #0x800 (exact T592 Clang CFI wall)
const BRK_0X1: u32 = 0xd420_0020;   // brk #0x1 (standard BUG path)
const BRK_0X0: u32 = 0xd420_0000;   // brk #0x0

fn write_word(orch: &mut Orchestrator, addr: u64, word: u32) {
    let offset = (addr - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}

/// Configure 39-bit VA MMU mapping for test execution at `RAM_BASE` (PA = VA = 0x4000_0000).
fn setup_test_mmu(orch: &mut Orchestrator) {
    let l1_table_pa = RAM_BASE + 0x10_000;
    for i in 0..512 {
        let offset = (l1_table_pa + i * 8 - RAM_BASE) as usize;
        orch.machine.ram[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
    }
    // L1 entry 1: block descriptor mapping 0x4000_0000..0x8000_0000
    let block_desc: u64 = 0x4000_0000 | (1 << 10) | 0b01;
    let offset = (l1_table_pa + 8 - RAM_BASE) as usize;
    orch.machine.ram[offset..offset + 8].copy_from_slice(&block_desc.to_le_bytes());

    let cpu = &mut orch.machine.cpu[0];
    cpu.sysregs.sctlr_el1 = 1; // MMU enabled
    cpu.sysregs.tcr_el1 = 0x0040_0032_b559_3519;
    cpu.sysregs.ttbr0_el1 = l1_table_pa;
    cpu.sysregs.ttbr1_el1 = l1_table_pa;
}

#[test]
fn qemu_vector_1_brk_el1_imm_0x800_exact_t592_wall() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200; // el1h_sync (VBAR_EL1 + 0x200)

    write_word(&mut orch, source_pc, BRK_0X800);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    let sentinel_far: u64 = 0xdead_beef_cafe_babe;
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.sysregs.far_el1 = sentinel_far;
        cpu.pstate = 0x6000_0005; // EL1h, NZCV: Z=1, C=1, DAIF unmasked (exact T592 register state)
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    // 1. Vector target: VBAR_EL1 + 0x200 (el1h_sync)
    assert_eq!(cpu.pc, vector_pc, "PC must vector to VBAR_EL1 + 0x200 (el1h_sync)");
    // 2. ELR_EL1: faulting instruction PC
    assert_eq!(cpu.sysregs.elr_el1, source_pc, "ELR_EL1 must hold faulting instruction PC");
    // 3. ESR_EL1: EC=0x3c, IL=1, ISS=0x800 -> 0xF2000800
    assert_eq!(cpu.sysregs.esr_el1, 0xF200_0800, "ESR_EL1 must match QEMU (0xF2000800)");
    assert_eq!((cpu.sysregs.esr_el1 >> 26) & 0x3F, 0x3C, "ESR_EL1 EC must be 0x3C (AArch64 BRK)");
    assert_eq!((cpu.sysregs.esr_el1 >> 25) & 0x1, 1, "ESR_EL1 IL must be 1 (32-bit instruction)");
    assert_eq!(cpu.sysregs.esr_el1 & 0xFFFF, 0x800, "ESR_EL1 ISS must be 0x800");
    // 4. SPSR_EL1: preserved pre-exception PSTATE
    assert_eq!(cpu.sysregs.spsr_el1, 0x6000_0005, "SPSR_EL1 must preserve interrupted PSTATE");
    // 5. DAIF masked on exception entry
    assert_eq!(cpu.pstate & 0x3C0, 0x3C0, "DAIF must be masked on exception entry");
    assert_eq!(cpu.sysregs.daif & 0x3C0, 0x3C0, "Sysregs DAIF must be masked on exception entry");
    // 6. FAR_EL1 UNTOUCHED (debug exceptions have no fault address; matches QEMU)
    assert_eq!(cpu.sysregs.far_el1, sentinel_far, "FAR_EL1 must remain untouched on BRK take");
    // 7. Orchestrator counters
    assert_eq!(orch.brk_count(), 1, "brk_count must increment");
}

#[test]
fn qemu_vector_2_brk_el0_imm_0x1_standard_bug_path() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x400; // el0_sync (VBAR_EL1 + 0x400)

    write_word(&mut orch, source_pc, BRK_0X1);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    let sentinel_far: u64 = 0x1122_3344_5566_7788;
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.sysregs.far_el1 = sentinel_far;
        cpu.pstate = 0x0000_0000; // EL0t (user mode), DAIF unmasked
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    // 1. Vector target: VBAR_EL1 + 0x400 (el0_sync) — NOT 0x200
    assert_eq!(cpu.pc, vector_pc, "PC must vector to VBAR_EL1 + 0x400 (el0_sync)");
    // 2. ELR_EL1: faulting instruction PC
    assert_eq!(cpu.sysregs.elr_el1, source_pc, "ELR_EL1 must hold faulting instruction PC");
    // 3. ESR_EL1: EC=0x3c, IL=1, ISS=0x1 -> 0xF2000001
    assert_eq!(cpu.sysregs.esr_el1, 0xF200_0001, "ESR_EL1 must match QEMU (0xF2000001)");
    assert_eq!((cpu.sysregs.esr_el1 >> 26) & 0x3F, 0x3C, "ESR_EL1 EC must be 0x3C");
    assert_eq!(cpu.sysregs.esr_el1 & 0xFFFF, 0x1, "ESR_EL1 ISS must be 0x1");
    // 4. SPSR_EL1: preserved pre-exception EL0 PSTATE
    assert_eq!(cpu.sysregs.spsr_el1, 0x0000_0000, "SPSR_EL1 must preserve interrupted EL0 PSTATE");
    // 5. Target mode is EL1h with DAIF masked
    assert_eq!(cpu.pstate & 0x1F, 0x05, "Current mode must become EL1h (0x05)");
    assert_eq!(cpu.pstate & 0x3C0, 0x3C0, "DAIF must be masked on exception entry");
    // 6. FAR_EL1 untouched
    assert_eq!(cpu.sysregs.far_el1, sentinel_far, "FAR_EL1 must remain untouched");
    assert_eq!(orch.brk_count(), 1);
}

#[test]
fn qemu_vector_3_brk_imm_0x0_encoding() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200; // el1h_sync

    write_word(&mut orch, source_pc, BRK_0X0);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = 0x2000_0005; // EL1h, C=1
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc);
    assert_eq!(cpu.sysregs.elr_el1, source_pc);
    // ESR_EL1: EC=0x3c, IL=1, ISS=0x0 -> 0xF2000000
    assert_eq!(cpu.sysregs.esr_el1, 0xF200_0000, "ESR_EL1 for imm=0 must be 0xF2000000");
    assert_eq!(cpu.sysregs.esr_el1 & 0xFFFF, 0x0, "ISS must be 0");
    assert_eq!(cpu.sysregs.spsr_el1, 0x2000_0005);
    assert_eq!(orch.brk_count(), 1);
}

#[test]
fn qemu_vector_4_kernel_mode_brk_with_daif_masking() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let vector_pc = vector_base + 0x200; // el1h_sync

    write_word(&mut orch, source_pc, BRK_0X800);
    write_word(&mut orch, vector_pc, NOP);

    setup_test_mmu(&mut orch);

    // Initial state: D=1, A=0, I=1, F=0 (DAIF = 0b1010_000000 = 0x280)
    // NZCV: N=1, Z=0, C=1, V=0 -> 0xA000_0000
    // Combined PSTATE = 0xA000_0285
    let pre_pstate = 0xA000_0285;
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0x280;
        cpu.pstate = pre_pstate;
    }

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);

    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, vector_pc);
    assert_eq!(cpu.sysregs.elr_el1, source_pc);
    // SPSR_EL1 must preserve the exact interrupted PSTATE including flags and partial DAIF
    assert_eq!(cpu.sysregs.spsr_el1, pre_pstate, "SPSR_EL1 must preserve interrupted DAIF and flags");
    // On entry, ALL DAIF bits are masked (0x3C0)
    assert_eq!(cpu.pstate & 0x3C0, 0x3C0, "All DAIF bits must be masked in PSTATE");
    assert_eq!(cpu.sysregs.daif & 0x3C0, 0x3C0, "All DAIF bits must be masked in sysregs.daif");
    // Mode must be EL1h
    assert_eq!(cpu.pstate & 0x1F, 0x05, "Mode must be EL1h");
    // Flags preserved
    assert_eq!(cpu.pstate & 0xF000_0000, 0xA000_0000, "NZCV flags must be preserved in PSTATE");
}

#[test]
fn qemu_vector_5_brk_eret_round_trip() {
    let mut orch = Orchestrator::new();
    let source_pc = RAM_BASE + 0x1000;
    let vector_base = RAM_BASE + 0x2000;
    let handler_pc = vector_base + 0x200; // el1h_sync

    // At source_pc: BRK #0x800
    write_word(&mut orch, source_pc, BRK_0X800);
    // At handler_pc: ERET (0xd69f03e0)
    write_word(&mut orch, handler_pc, ERET);

    setup_test_mmu(&mut orch);

    let initial_pstate = 0x4000_0005; // EL1h, Z=1, DAIF=0
    {
        let cpu = &mut orch.machine.cpu[0];
        cpu.pc = source_pc;
        cpu.sp = RAM_BASE + 0x8000;
        cpu.sysregs.vbar_el1 = vector_base;
        cpu.sysregs.daif = 0;
        cpu.pstate = initial_pstate;
    }

    // Step 1: Execute BRK -> vectors to handler_pc
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, handler_pc);
    assert_eq!(orch.machine.cpu[0].sysregs.elr_el1, source_pc);

    // In handler, assume kernel advances ELR_EL1 by 4 to skip faulting BRK
    let next_pc = source_pc + 4;
    orch.machine.cpu[0].sysregs.elr_el1 = next_pc;

    // Step 2: Execute ERET -> restores PC from ELR_EL1 and PSTATE from SPSR_EL1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    let cpu = &orch.machine.cpu[0];
    assert_eq!(cpu.pc, next_pc, "ERET must restore PC to ELR_EL1");
    assert_eq!(cpu.pstate, initial_pstate, "ERET must restore PSTATE from SPSR_EL1");
    assert_eq!(cpu.sysregs.daif, 0, "ERET must unmask DAIF according to restored SPSR_EL1");
    assert_eq!(orch.irq_return_count(), 1, "ERET must increment irq_return_count");
}
