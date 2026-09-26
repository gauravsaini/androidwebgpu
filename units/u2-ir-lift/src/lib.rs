//! U2 `arm-ir-lift`: decoded AArch64 instruction -> explicit IR ops.
//!
//! Purity: PURE. `lift` is a deterministic function of `&Instruction` only.
//! No I/O, no time, no threads, no hidden state. Imports only `pathn_contracts`.
//!
//! ## Lifting model
//!
//! The frozen contract (`pathn_contracts::cpu::IrOp`) is the whole truth about
//! what can be expressed:
//!
//! - `Add { dst, a, b }` / `Mov { dst, imm }` — operands are indices into the
//!   guest register file (`0..=30` = X0-X30/W0-W30, `31` = XZR/SP following the
//!   usual contextual AArch64 rule; the execution backend applies it).
//! - `Load { dst, addr, size }` / `Store { src, addr, size }` — `addr` is a
//!   **static** `u64` guest address and `size` is the access width in bytes
//!   (`4` or `8`). The lifter runs ahead of time and never knows register
//!   contents, so only statically-known addresses lift: PC-relative literal
//!   loads (`LDR (literal)` -> `addr = insn.addr + offset`). Any
//!   register/SP-relative form has a dynamic effective address and therefore
//!   traps — inventing a static address would be inventing semantics.
//! - `Branch { target }` — statically computed `insn.addr + offset`; this is
//!   the explicit control-flow exit for the block.
//!
//! ## Scratch register
//!
//! `IrOp::Add` is register-register only, so immediate operands are
//! materialized first: `ADD Xd, Xn, #imm` lifts to
//! `Mov { dst: SCRATCH, imm }` followed by `Add { dst: rd, a: rn, b: SCRATCH }`.
//! `SCRATCH` is index `32`, outside the 32 architectural registers. The
//! execution backend (U3/orchestrator) must provide at least this one scratch
//! slot; it is never an architectural register and is dead after the
//! instruction's op sequence.
//!
//! ## Honesty rule
//!
//! Anything the contract cannot express — flag-setting ALU ops, shifted
//! register operands, 32-bit ALU widths (upper-bit zeroing is not expressible
//! in `IrOp::Add`), sub-word memory widths, dynamic memory addresses,
//! system/privileged instructions, unrecognized words — lifts to
//! `IrOp::Trap { reason }` naming exactly what is unsupported. A trap is data,
//! never a silent nop and never a panic.

use pathn_contracts::cpu::{InsnKind, Instruction, IrOp};

/// Private scratch register index used to materialize immediates.
/// See module docs. Outside the architectural `0..=31` range.
pub const SCRATCH: u8 = 32;

// ---- Trap reasons (exact strings; tests pin them) ----
const R_SYSTEM: &str = "System: system and privileged semantics are not lifted";
const R_UNKNOWN: &str = "Unknown: illegal or unrecognized instruction word";
const R_DP_UNSUPPORTED: &str = "DataProc: unsupported encoding";
const R_MOVZ_HW: &str = "DataProc: MOVZ hw field > 1 with sf = 0 is unallocated";
const R_ADD32_IMM: &str = "DataProc: 32-bit ADD immediate width is not expressible in IrOp::Add";
const R_ADD32_REG: &str = "DataProc: 32-bit ADD register width is not expressible in IrOp::Add";
const R_ADD_SHIFT: &str =
    "DataProc: shifted or extended ADD register operand is not expressible in IrOp";
const R_LS_UNSUPPORTED: &str = "LoadStore: unsupported encoding";
const R_LS_SUBWORD: &str = "LoadStore: sub-word access width is not expressible in IrOp";
const R_LS_DYNAMIC: &str =
    "LoadStore: register-relative address is dynamic; IrOp::Load/Store carry static addresses only";
const R_BR_UNSUPPORTED: &str = "Branch: only unconditional immediate B is lifted";

fn trap(reason: &'static str) -> Vec<IrOp> {
    vec![IrOp::Trap { reason }]
}

/// Lift one decoded instruction to an explicit IR op sequence.
///
/// Every [`InsnKind`] is mapped: supported forms become guest-state
/// read/write ops, everything else becomes [`IrOp::Trap`] with a reason that
/// names the unsupported kind/form.
pub fn lift(insn: &Instruction) -> Vec<IrOp> {
    match insn.kind {
        InsnKind::DataProc => lift_data_proc(insn.word),
        InsnKind::LoadStore => lift_load_store(insn),
        InsnKind::Branch => lift_branch(insn),
        InsnKind::System => trap(R_SYSTEM),
        InsnKind::Unknown => trap(R_UNKNOWN),
    }
}

/// Data-processing: MOVZ, ADD (immediate), ADD (shifted register, LSL #0).
fn lift_data_proc(word: u32) -> Vec<IrOp> {
    // MOVZ: sf 10 100101 hw imm16 Rd  (bits 30:23 = 0xA5)
    if (word >> 23) & 0xFF == 0xA5 {
        let sf = word >> 31;
        let hw = (word >> 21) & 0x3;
        if sf == 0 && hw > 1 {
            return trap(R_MOVZ_HW);
        }
        let imm16 = (word >> 5) & 0xFFFF;
        let rd = (word & 0x1F) as u8;
        // 32-bit MOVZ zero-extends by definition, so the 64-bit slot value is
        // exactly imm16 << (hw * 16) with upper bits zero.
        let imm = (imm16 as u64) << (hw * 16);
        return vec![IrOp::Mov { dst: rd, imm }];
    }
    // ADD (immediate): sf 0 0 10001 sh imm12 Rn Rd  (bits 30:24 = 0x11;
    // bit 29 = 0 excludes ADDS, bit 30 = 0 excludes SUB(S)).
    if (word >> 24) & 0x7F == 0x11 {
        if word >> 31 == 0 {
            return trap(R_ADD32_IMM);
        }
        let sh = (word >> 22) & 1;
        let imm12 = (word >> 10) & 0xFFF;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        let imm = (imm12 as u64) << (if sh == 1 { 12 } else { 0 });
        return vec![
            IrOp::Mov { dst: SCRATCH, imm },
            IrOp::Add {
                dst: rd,
                a: rn,
                b: SCRATCH,
            },
        ];
    }
    // ADD (shifted register): sf 0 01011 00 0 Rm imm6 Rn Rd, LSL #0 only
    // (bits 31:24 = 0x0B/0x8B; S = 1 would be ADDS and never matches).
    let top = (word >> 24) & 0xFF;
    if top == 0x0B || top == 0x8B {
        if word >> 31 == 0 {
            return trap(R_ADD32_REG);
        }
        let shift = (word >> 22) & 0x3;
        let imm6 = (word >> 10) & 0x3F;
        if shift != 0 || imm6 != 0 {
            return trap(R_ADD_SHIFT);
        }
        let rm = ((word >> 16) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        return vec![IrOp::Add {
            dst: rd,
            a: rn,
            b: rm,
        }];
    }
    trap(R_DP_UNSUPPORTED)
}

/// Loads/stores: only forms with statically-known addresses lift.
fn lift_load_store(insn: &Instruction) -> Vec<IrOp> {
    let word = insn.word;
    // LDR (literal): sf 00 011000 imm19 Rt  (bits 31:24 = 0x18 / 0x58).
    // Address = PC + sign_extend(imm19 << 2): fully static.
    let top = (word >> 24) & 0xFF;
    if top == 0x18 || top == 0x58 {
        let size: u8 = if top == 0x58 { 8 } else { 4 };
        let imm19 = (word >> 5) & 0x7FFFF;
        let offset = (((imm19 as i32) << 13) >> 13) as i64 * 4;
        let addr = (insn.addr as i64).wrapping_add(offset) as u64;
        let rt = (word & 0x1F) as u8;
        return vec![IrOp::Load {
            dst: rt,
            addr,
            size,
        }];
    }
    // LD/ST (immediate, unsigned offset): size 11 111001 L imm12 Rn Rt
    // (bits 29:24 = 0x39; bit 22 L: 1 = load, 0 = store).
    if (word >> 24) & 0x3F == 0x39 {
        // Sub-word widths are not expressible even before the address
        // question arises.
        match (word >> 30) & 0x3 {
            0 | 1 => return trap(R_LS_SUBWORD),
            _ => {}
        }
        // AArch64: for these forms Rn = 31 is SP, and any other Rn is a
        // general register — either way the base is a dynamic value the
        // static lifter cannot know, so the effective address is dynamic.
        return trap(R_LS_DYNAMIC);
    }
    trap(R_LS_UNSUPPORTED)
}

/// Branches: unconditional immediate B only.
fn lift_branch(insn: &Instruction) -> Vec<IrOp> {
    let word = insn.word;
    // B: 000101 imm26 — target = addr + sign_extend(imm26 << 2).
    if (word >> 26) & 0x3F == 0x05 {
        let imm26 = word & 0x3FFF_FFFF;
        let offset = (((imm26 as i32) << 6) >> 6) as i64 * 4;
        let target = (insn.addr as i64).wrapping_add(offset) as u64;
        return vec![IrOp::Branch { target }];
    }
    trap(R_BR_UNSUPPORTED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insn(addr: u64, word: u32, kind: InsnKind) -> Instruction {
        Instruction { addr, word, kind }
    }

    // ---------- goldens: exact op sequences ----------

    #[test]
    fn golden_movz_64_lsl16() {
        // MOVZ X0, #0x1234, LSL #16
        let ops = lift(&insn(0x4000, 0xD2A2_4680, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x1234_0000
            }]
        );
    }

    #[test]
    fn golden_movz_32_zero_extends() {
        // MOVZ W1, #0xAB  -> X1 = 0xAB exactly (upper 32 zeroed by definition)
        let ops = lift(&insn(0x4000, 0x5280_1561, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Mov { dst: 1, imm: 0xAB }]);
    }

    #[test]
    fn golden_add_imm() {
        // ADD X0, X1, #0x10
        let ops = lift(&insn(0x4000, 0x9100_4020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0x10
                },
                IrOp::Add {
                    dst: 0,
                    a: 1,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn golden_add_imm_shifted12() {
        // ADD X2, X3, #1, LSL #12  -> imm = 0x1000
        let ops = lift(&insn(0x4000, 0x9140_0462, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0x1000
                },
                IrOp::Add {
                    dst: 2,
                    a: 3,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn golden_add_reg() {
        // ADD X0, X1, X2
        let ops = lift(&insn(0x4000, 0x8B02_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Add { dst: 0, a: 1, b: 2 }]);
    }

    #[test]
    fn golden_ldr_literal_64() {
        // LDR X0, [PC, #8] at 0x4000 -> static address 0x4008
        let ops = lift(&insn(0x4000, 0x5800_0040, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Load {
                dst: 0,
                addr: 0x4008,
                size: 8
            }]
        );
    }

    #[test]
    fn golden_ldr_literal_32_negative() {
        // LDR W3, [PC, #-4] at 0x4000 -> static address 0x3FFC, 4-byte width
        let ops = lift(&insn(0x4000, 0x18FF_FFE3, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Load {
                dst: 3,
                addr: 0x3FFC,
                size: 4
            }]
        );
    }

    #[test]
    fn golden_str_register_base_traps_dynamic() {
        // STR X5, [X6, #0x20]: base is dynamic, so no honest static Store exists.
        let ops = lift(&insn(0x4000, 0xF900_10C5, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_LS_DYNAMIC
            }]
        );
    }

    #[test]
    fn golden_ldr_register_base_traps_dynamic() {
        // LDR X0, [X1, #8]: same dynamic-address honesty rule as STR.
        let ops = lift(&insn(0x4000, 0xF940_0420, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_LS_DYNAMIC
            }]
        );
    }

    #[test]
    fn golden_b_forward() {
        // B +0x100 at 0x4000 -> explicit exit to 0x4100
        let ops = lift(&insn(0x4000, 0x1400_0040, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::Branch { target: 0x4100 }]);
    }

    #[test]
    fn golden_b_backward_sign_extended() {
        // B -4 at 0x4000 -> 0x3FFC (imm26 sign extension exercised)
        let ops = lift(&insn(0x4000, 0x17FF_FFFF, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::Branch { target: 0x3FFC }]);
    }

    // ---------- traps: unsupported kinds/forms are data, never silent ----------

    #[test]
    fn trap_system_kind_names_kind() {
        let ops = lift(&insn(0x4000, 0xD538_1040, InsnKind::System));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_SYSTEM }]);
        assert!(matches!(&ops[0], IrOp::Trap { reason } if reason.contains("System")));
    }

    #[test]
    fn trap_unknown_kind_names_kind() {
        let ops = lift(&insn(0x4000, 0xFFFF_FFFF, InsnKind::Unknown));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_UNKNOWN }]);
    }

    #[test]
    fn trap_adds_flag_update() {
        // ADDS X0, X1, #1: flag semantics not expressible -> trap
        let ops = lift(&insn(0x4000, 0xB100_0420, InsnKind::DataProc));
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], IrOp::Trap { .. }));
    }

    #[test]
    fn trap_add_imm_32bit_width() {
        // ADD W0, W1, #1: 32-bit zeroing of Xd not expressible in IrOp::Add
        let ops = lift(&insn(0x4000, 0x1100_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_ADD32_IMM
            }]
        );
    }

    #[test]
    fn trap_add_reg_shifted() {
        // ADD X0, X1, X2, LSL #1: shifter not expressible
        let ops = lift(&insn(0x4000, 0x8B02_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_ADD_SHIFT
            }]
        );
    }

    #[test]
    fn trap_movz_bad_hw() {
        // sf = 0, hw = 2: unallocated encoding
        let ops = lift(&insn(0x4000, 0x52C0_0000, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_MOVZ_HW }]);
    }

    #[test]
    fn trap_ldrb_subword() {
        // LDRB W0, [X1]: 8-bit width not expressible
        let ops = lift(&insn(0x4000, 0x3940_0420, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_LS_SUBWORD
            }]
        );
    }

    #[test]
    fn trap_bl_not_plain_b() {
        // BL: link semantics not lifted
        let ops = lift(&insn(0x4000, 0x9400_0040, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_BR_UNSUPPORTED
            }]
        );
    }

    // ---------- structural properties ----------

    #[test]
    fn every_kind_maps_to_nonempty_ops() {
        let kinds = [
            InsnKind::DataProc,
            InsnKind::LoadStore,
            InsnKind::Branch,
            InsnKind::System,
            InsnKind::Unknown,
        ];
        for kind in kinds {
            let ops = lift(&insn(0x4000, 0xFFFF_FFFF, kind));
            assert!(!ops.is_empty(), "kind {kind:?} produced no ops");
        }
    }

    #[test]
    fn determinism_lift_twice_equal() {
        let cases = [
            insn(0x4000, 0xD2A2_4680, InsnKind::DataProc),
            insn(0x4000, 0x9100_4020, InsnKind::DataProc),
            insn(0x4000, 0x5800_0040, InsnKind::LoadStore),
            insn(0x4000, 0x1400_0040, InsnKind::Branch),
            insn(0x4000, 0xFFFF_FFFF, InsnKind::Unknown),
        ];
        for c in cases {
            assert_eq!(lift(&c), lift(&c));
        }
    }

    #[test]
    fn scratch_does_not_collide_with_arch_regs() {
        const { assert!(SCRATCH > 31) } // scratch must sit outside X0-X30/XZR-SP
    }
}
