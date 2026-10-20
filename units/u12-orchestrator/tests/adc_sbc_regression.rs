//! QEMU-matched regression vectors for ARM64 ADC, SBC, ADCS, SBCS.
//!
//! Track T587: Verifies NZCV carry flag propagation and arithmetic for:
//! 1. ADC 64-bit (carry-in 0 / 1, Rd write vs Rd=xzr discard)
//! 2. SBC 64-bit (carry-in 0 / 1, Rd write vs Rd=xzr discard)
//! 3. ADCS 64-bit (carry-in 0 / 1, overflow detection, NZCV update)
//! 4. SBCS 64-bit (carry-in 0 / 1, borrow detection, NZCV update)
//! 5. Exact kernel halt instruction: `sbcs xzr, x10, x8` (0xfa08015f) at step 49,688,549
//! 6. 32-bit forms: ADC and SBCS
//! 7. The surrounding kernel idiom: csinv -> sbcs -> cset -> cbz (lib/find_bit.c)
//!
//! All vectors matched against native QEMU 11.1.2 Cortex-A53 reference oracle.

use u12_orchestrator::arm64::{FLAG_C, FLAG_N, FLAG_V, FLAG_Z, FLAGS_NZCV_MASK};
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

fn write_words(orch: &mut Orchestrator, addr: u64, words: &[u32]) {
    for (i, word) in words.iter().enumerate() {
        let offset = ((addr + (i as u64) * 4) - RAM_BASE) as usize;
        orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// 1. ADC 64-bit
// ---------------------------------------------------------------------------

#[test]
fn qemu_vector_1_adc_64_carry_in_0() {
    // ADC X3, X1, X2 (0x9A020023): X1=0x100, X2=0x20, C=0 -> X3=0x120, flags unchanged
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = 0; // C=0
    write_words(&mut orch, pc, &[0x9A02_0023]); // ADC X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0x120);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0);
}

#[test]
fn qemu_vector_2_adc_64_carry_in_1() {
    // ADC X3, X1, X2 (0x9A020023): X1=0x100, X2=0x20, C=1 -> X3=0x121, C preserved
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = FLAG_C; // C=1
    write_words(&mut orch, pc, &[0x9A02_0023]); // ADC X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0x121);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_C);
}

#[test]
fn qemu_vector_3_adc_64_rd_xzr_discard() {
    // ADC XZR, X1, X2 (0x9A02003F): Rd=31 discards result, flags preserved
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = FLAG_C; // C=1
    write_words(&mut orch, pc, &[0x9A02_003F]); // ADC XZR, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_C);
}

// ---------------------------------------------------------------------------
// 2. SBC 64-bit
// ---------------------------------------------------------------------------

#[test]
fn qemu_vector_4_sbc_64_carry_in_0_borrow() {
    // SBC X3, X1, X2 (0xDA020023): X1=0x100, X2=0x20, C=0 (borrow) -> X3 = 0x100 - 0x20 - 1 = 0xDF
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = 0; // C=0
    write_words(&mut orch, pc, &[0xDA02_0023]); // SBC X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0xDF);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0);
}

#[test]
fn qemu_vector_5_sbc_64_carry_in_1_no_borrow() {
    // SBC X3, X1, X2 (0xDA020023): X1=0x100, X2=0x20, C=1 (no borrow) -> X3 = 0x100 - 0x20 = 0xE0
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = FLAG_C; // C=1
    write_words(&mut orch, pc, &[0xDA02_0023]); // SBC X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0xE0);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_C);
}

#[test]
fn qemu_vector_6_sbc_64_rd_xzr_discard() {
    // SBC XZR, X1, X2 (0xDA02003F): Rd=31 discards result, flags preserved
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = FLAG_C; // C=1
    write_words(&mut orch, pc, &[0xDA02_003F]); // SBC XZR, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_C);
}

// ---------------------------------------------------------------------------
// 3. ADCS 64-bit
// ---------------------------------------------------------------------------

#[test]
fn qemu_vector_7_adcs_64_carry_in_0() {
    // ADCS X3, X1, X2 (0xBA020023): X1=0x100, X2=0x20, C=0 -> X3=0x120, C=0, Z=0, N=0, V=0
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = 0;
    write_words(&mut orch, pc, &[0xBA02_0023]); // ADCS X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0x120);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0);
}

#[test]
fn qemu_vector_8_adcs_64_carry_in_1_clears_carry_out() {
    // ADCS X3, X1, X2 (0xBA020023): X1=0x100, X2=0x20, C=1 -> X3=0x121.
    // QEMU verifies: carry_out is 0 because no unsigned wrap, so C flag is cleared!
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = FLAG_C;
    write_words(&mut orch, pc, &[0xBA02_0023]); // ADCS X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0x121);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0);
}

#[test]
fn qemu_vector_9_adcs_64_overflow() {
    // ADCS X3, X1, X2 with X1=0x7FFF_FFFF_FFFF_FFFF, X2=0, C=1 -> X3=0x8000_0000_0000_0000, N=1, V=1
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x7FFF_FFFF_FFFF_FFFF;
    orch.machine.cpu[0].regs[2] = 0;
    orch.machine.cpu[0].pstate = FLAG_C;
    write_words(&mut orch, pc, &[0xBA02_0023]); // ADCS X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0x8000_0000_0000_0000);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_N | FLAG_V);
}

// ---------------------------------------------------------------------------
// 4. SBCS 64-bit
// ---------------------------------------------------------------------------

#[test]
fn qemu_vector_10_sbcs_64_borrow_sets_negative() {
    // SBCS X3, X1, X2 (0xFA020023): X1=0x20, X2=0x100, C=0 -> X3 = 0x20 - 0x100 - 1 = -0xE1
    // N=1, C=0 (borrow generated)
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x20;
    orch.machine.cpu[0].regs[2] = 0x100;
    orch.machine.cpu[0].pstate = 0;
    write_words(&mut orch, pc, &[0xFA02_0023]); // SBCS X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], (-0xE1i64) as u64);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_N);
}

#[test]
fn qemu_vector_11_sbcs_64_no_borrow_sets_carry() {
    // SBCS X3, X1, X2 (0xFA020023): X1=0x100, X2=0x20, C=1 -> X3 = 0xE0
    // C=1 (no borrow), N=0, Z=0, V=0
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0x100;
    orch.machine.cpu[0].regs[2] = 0x20;
    orch.machine.cpu[0].pstate = FLAG_C;
    write_words(&mut orch, pc, &[0xFA02_0023]); // SBCS X3, X1, X2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0xE0);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_C);
}

#[test]
fn qemu_vector_12_sbcs_kernel_halt_step49688549() {
    // Exact instruction halting step 49,688,549 in Linux __find_next_bit:
    // `sbcs xzr, x10, x8` (0xFA08015F)
    // Initial guest state: X10=4, X8=0xFFFF_FFFF_FFFF_FFFF (-1), C=0.
    // 4 - (-1) - 1 with C=0: AddWithCarry(4, 0, 0) -> result 4 (discarded to xzr).
    // Flags: unsigned_sum = 4 < 2^64 -> C=0 (borrow), Z=0, N=0, V=0. NZCV = 0.
    let mut orch = Orchestrator::new();
    let _pc: u64 = 0xffffff8008259154;
    // Map to test RAM address
    let test_pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = test_pc;
    orch.machine.cpu[0].regs[10] = 4;
    orch.machine.cpu[0].regs[8] = u64::MAX;
    orch.machine.cpu[0].pstate = 0x0000_0005; // C=0
    write_words(&mut orch, test_pc, &[0xFA08_015F]); // SBCS XZR, X10, X8

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[10], 4, "X10 must be preserved");
    assert_eq!(orch.machine.cpu[0].regs[8], u64::MAX, "X8 must be preserved");
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0, "NZCV must be 0");
    assert_eq!(orch.machine.cpu[0].pc, test_pc + 4);
}

// ---------------------------------------------------------------------------
// 5. 32-bit forms
// ---------------------------------------------------------------------------

#[test]
fn qemu_vector_13_adc_32_carry_in_1() {
    // ADC W3, W1, W2 (0x1A020023): W1=0x100, W2=0x20, C=1 -> W3=0x121, upper 32 zeroed
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[1] = 0xDEAD_BEEF_0000_0100;
    orch.machine.cpu[0].regs[2] = 0xCAFE_BABE_0000_0020;
    orch.machine.cpu[0].pstate = FLAG_C;
    write_words(&mut orch, pc, &[0x1A02_0023]); // ADC W3, W1, W2

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[3], 0x121);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_C);
}

#[test]
fn qemu_vector_14_sbcs_32_wzr_discard() {
    // SBCS WZR, W10, W8 (0x7A08015F): W10=4, W8=0xFFFFFFFF, C=0 -> flags NZCV=0
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[10] = 4;
    orch.machine.cpu[0].regs[8] = 0xFFFF_FFFF;
    orch.machine.cpu[0].pstate = 0;
    write_words(&mut orch, pc, &[0x7A08_015F]); // SBCS WZR, W10, W8

    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[10], 4);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0);
}

// ---------------------------------------------------------------------------
// 6. Surrounding Linux kernel idiom (lib/find_bit.c)
// ---------------------------------------------------------------------------

#[test]
fn qemu_vector_15_surrounding_idiom_c0_advances_past_cbz() {
    // Surrounding disassembly from Linux __find_next_bit:
    // 0x00: csinv x10, x10, xzr, lo  (0xDA9F314A)
    // 0x04: sbcs  xzr, x10, x8       (0xFA08015F)
    // 0x08: cset  x10, ls            (0x9A9F87EA)
    // 0x0C: cbz   x10, #+0x34        (0xB40001AA)
    // 0x10: ... next instruction (target when not taken)
    let words = [
        0xDA9F_314A, // csinv x10, x10, xzr, lo
        0xFA08_015F, // sbcs xzr, x10, x8
        0x9A9F_87EA, // cset x10, ls
        0xB400_01AA, // cbz x10, #+0x34
    ];
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[10] = 4;
    orch.machine.cpu[0].regs[8] = u64::MAX;
    orch.machine.cpu[0].pstate = 0x0000_0005; // C=0
    write_words(&mut orch, pc, &words);

    // Step 1: csinv x10, x10, xzr, lo -> with C=0 (lo true), selects Rn = x10 = 4
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[10], 4);

    // Step 2: sbcs xzr, x10, x8 -> 4 - (-1) - 1 with C=0 -> C=0, Z=0
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, 0);

    // Step 3: cset x10, ls -> with C=0 (ls true: !C || Z), x10 = 1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[10], 1);

    // Step 4: cbz x10, #+0x34 -> x10=1 != 0, so branch is NOT taken, falls through to pc + 16!
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pc, pc + 16, "Must fall through cbz when x10 == 1");
}

#[test]
fn qemu_vector_16_surrounding_idiom_c1() {
    // Same sequence but with C=1 initial state
    let words = [
        0xDA9F_314A, // csinv x10, x10, xzr, lo
        0xFA08_015F, // sbcs xzr, x10, x8
        0x9A9F_87EA, // cset x10, ls
    ];
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x1000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[10] = 4;
    orch.machine.cpu[0].regs[8] = u64::MAX;
    orch.machine.cpu[0].pstate = FLAG_C; // C=1
    write_words(&mut orch, pc, &words);

    // Step 1: csinv x10, x10, xzr, lo -> with C=1 (lo false), selects !Rm = !xzr = -1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[10], u64::MAX);

    // Step 2: sbcs xzr, x10, x8 -> -1 - (-1) - 0 -> AddWithCarry(-1, 0, 1) -> 0
    // Z=1, C=1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_Z | FLAG_C);

    // Step 3: cset x10, ls -> with Z=1 (ls true: !C || Z), x10 = 1
    assert_eq!(orch.step_vcpu(), StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[10], 1);
}
