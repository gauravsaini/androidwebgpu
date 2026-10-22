//! QEMU-matched regression vectors for ARM64 logical immediate AND with Rd=31 (SP / WSP).
//!
//! Track T612 (revived from T486):
//! Verifies correct semantics for non-flag-setting logical-immediate AND where Rd=31:
//! 1. 64-bit AND SP, Xn, #imm writes SP, preserves PSTATE, advances PC.
//! 2. 64-bit exact T486 Linux kernel encoding (`0x927ae53f`, `and sp, x9, #0xffffffffffffffc0`).
//! 3. 64-bit Rn=31 reads XZR (0), writes 0 to SP.
//! 4. 32-bit AND WSP, Wn, #imm writes zero-extended result to SP.
//! 5. 32-bit Rn=31 reads WZR (0), writes 0 to SP.
//! 6. Non-flag-setting AND with Rd!=31 writes Xd and leaves SP untouched.
//! 7. Flag-setting ANDS with Rd=31 (TST) updates NZCV and leaves SP untouched.

use u12_orchestrator::arm64::{FLAG_C, FLAG_N, FLAG_V, FLAG_Z, FLAGS_NZCV_MASK};
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

fn write_words(orch: &mut Orchestrator, addr: u64, words: &[u32]) {
    for (i, word) in words.iter().enumerate() {
        let offset = ((addr + (i as u64) * 4) - RAM_BASE) as usize;
        orch.machine.ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
}

#[test]
fn test_exact_t486_and_sp_64bit() {
    // Exact T486 fault site instruction at 0xffffff80082ea400:
    // AND SP, X9, #0xffffffffffffffc0 (word 0x927ae53f)
    // In T486 trace: X9 = 0xffffff8009943b10, old SP = 0xffffff8009943db0.
    // Must set SP to 0xffffff8009943b00, preserving PSTATE and advancing PC.
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x2000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[9] = 0xffffff8009943b10;
    orch.machine.cpu[0].sp = 0xffffff8009943db0;
    orch.machine.cpu[0].pstate = FLAG_N | FLAG_C; // arbitrary NZCV to verify preservation

    write_words(&mut orch, pc, &[0x927a_e53f]);

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(
        orch.machine.cpu[0].sp, 0xffffff8009943b00,
        "SP must be updated to (X9 & #0xffffffffffffffc0)"
    );
    assert_eq!(orch.machine.cpu[0].pc, pc + 4, "PC must advance by 4");
    assert_eq!(
        orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK,
        FLAG_N | FLAG_C,
        "PSTATE NZCV must be preserved by non-flag-setting AND"
    );
    assert_eq!(orch.machine.cpu[0].regs[9], 0xffffff8009943b10, "X9 must be unchanged");
}

#[test]
fn test_and_sp_64bit_rn_xzr() {
    // AND SP, XZR, #0xffffffffffffffc0 (word 0x927ae7ff)
    // Rn=31 in logical immediate is XZR (reads 0). Result is 0 & mask = 0.
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x2000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].sp = 0xdead_beef_feed_face;
    orch.machine.cpu[0].pstate = FLAG_Z;

    write_words(&mut orch, pc, &[0x927a_e7ff]);

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].sp, 0, "AND with Rn=XZR must set SP to 0");
    assert_eq!(orch.machine.cpu[0].pc, pc + 4);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_Z);
}

#[test]
fn test_and_wsp_32bit_zero_extend() {
    // AND WSP, W9, #0xffffffc0 (word 0x121a653f)
    // 32-bit result: (0x887766f0 & 0xffffffc0) = 0x887766c0.
    // Zero-extended to 64 bits: 0x00000000887766c0.
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x2000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[9] = 0x1122_3344_8877_66f0;
    orch.machine.cpu[0].sp = 0xffff_ffff_ffff_ffff;
    orch.machine.cpu[0].pstate = FLAG_V;

    write_words(&mut orch, pc, &[0x121a_653f]);

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(
        orch.machine.cpu[0].sp, 0x0000_0000_8877_66c0,
        "WSP result must be zero-extended into 64-bit SP"
    );
    assert_eq!(orch.machine.cpu[0].pc, pc + 4);
    assert_eq!(orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK, FLAG_V);
}

#[test]
fn test_and_wsp_32bit_rn_wzr() {
    // AND WSP, WZR, #0xffffffc0 (word 0x121a67ff)
    // Rn=31 reads WZR (0). Result is 0, zero-extended into SP.
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x2000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].sp = 0xaaaa_bbbb_cccc_dddd;

    write_words(&mut orch, pc, &[0x121a_67ff]);

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].sp, 0);
    assert_eq!(orch.machine.cpu[0].pc, pc + 4);
}

#[test]
fn test_and_xd_not_sp_preserved() {
    // AND X0, X9, #0xffffffffffffffc0 (word 0x927ae520)
    // Rd=0 (not SP). Must write X0 and leave SP unchanged.
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x2000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[9] = 0xffffff8009943b10;
    orch.machine.cpu[0].sp = 0xffffff8009943db0;

    write_words(&mut orch, pc, &[0x927a_e520]);

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(orch.machine.cpu[0].regs[0], 0xffffff8009943b00);
    assert_eq!(
        orch.machine.cpu[0].sp, 0xffffff8009943db0,
        "SP must remain unchanged when Rd != 31"
    );
    assert_eq!(orch.machine.cpu[0].pc, pc + 4);
}

#[test]
fn test_ands_flag_setting_sp_untouched() {
    // TST X9, #0xffffffffffffffc0 (word 0xf27ae53f) -> ANDS XZR, X9, #imm
    // Flag-setting ANDS with Rd=31 discards result, updates NZCV, leaves SP unchanged.
    let mut orch = Orchestrator::new();
    let pc = RAM_BASE + 0x2000;
    orch.machine.cpu[0].pc = pc;
    orch.machine.cpu[0].regs[9] = 0xffffff8009943b10;
    orch.machine.cpu[0].sp = 0xffffff8009943db0;
    orch.machine.cpu[0].pstate = 0;

    write_words(&mut orch, pc, &[0xf27a_e53f]);

    let outcome = orch.step_vcpu();
    assert_eq!(outcome, StepOutcome::Continue);
    assert_eq!(
        orch.machine.cpu[0].sp, 0xffffff8009943db0,
        "ANDS with Rd=31 must NOT write SP (treated as XZR)"
    );
    assert_eq!(orch.machine.cpu[0].pc, pc + 4);
    assert_ne!(
        orch.machine.cpu[0].pstate & FLAGS_NZCV_MASK,
        0,
        "ANDS must update NZCV"
    );
}
