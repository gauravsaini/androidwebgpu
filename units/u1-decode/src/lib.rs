//! U1 `aarch64-decode` — 32-bit word → decoded AArch64 instruction.
//!
//! PURE: deterministic, no I/O, no time, no threads, no hidden state.
//! Only `pathn_contracts` is imported (swarm law: no other unit crates).
//!
//! Supported subset (exactly LLD §U1, nothing more):
//! - data-processing immediate: ADD/SUB (immediate, S=0), MOVZ, MOVN
//! - data-processing register: ADD/SUB (shifted register, S=0),
//!   ORR/EOR (shifted register)
//! - loads/stores: LDR/STR (immediate, unsigned offset), integer registers
//! - branches: B, BL, CBZ, CBNZ, RET
//! - system: HINT (NOP = HINT #0)
//!
//! Every other encoding → `DecodeResult::Illegal`, honestly. Illegal words are
//! data, never panics. Unallocated/reserved variants of *supported* mnemonics
//! (ADDS/SUBS, MOVK, extended-register ADD, AND/ANDS, BR/BLR, SIMD …) are also
//! Illegal: this unit claims only what LLD §U1 lists, never fake-decodes.

use pathn_contracts::cpu::{DecodeResult, InsnKind, Instruction};

/// Decode one 32-bit AArch64 instruction word.
///
/// `Instruction.addr` is always 0: decode takes only the word, so no address
/// is known. The lifter (U2) / orchestrator stamps the real address.
pub fn decode(word: u32) -> DecodeResult {
    match classify(word) {
        Some(kind) => DecodeResult::Ok(Instruction {
            addr: 0,
            word,
            kind,
        }),
        None => DecodeResult::Illegal { word },
    }
}

/// Major group by bits[28:25]. Mapping cross-checked against known encodings:
/// 0x91004420 (ADD imm) → 1000, 0xD2800000 (MOVZ) → 1001,
/// 0x14000000 (B) / 0xD503201F (NOP) → 1010, 0xD65F03C0 (RET) → 1011,
/// 0x8B020020 (ADD reg) / 0xAA020020 (ORR reg) → 0101,
/// 0xB9000020 (STR) / 0xF9400020 (LDR) → 1100.
fn classify(word: u32) -> Option<InsnKind> {
    match (word >> 25) & 0xF {
        0b1000 | 0b1001 => decode_dp_imm(word),
        0b1010 | 0b1011 => decode_branch_sys(word),
        0b0101 | 0b1101 => decode_dp_reg(word),
        0b0100 | 0b1100 => decode_ldst(word),
        // 0b000x..0b001x: unallocated; 0b011x/0b111x: SIMD/FP/SVE (out of scope)
        _ => None,
    }
}

/// Data-processing (immediate). Key = bits[28:22] (7 bits) so the shift/halfword
/// selector bit is pinned, not assumed.
fn decode_dp_imm(word: u32) -> Option<InsnKind> {
    match (word >> 22) & 0x7F {
        // Add/subtract (immediate): sf op S 10001 sh imm12 Rn Rd, sh ∈ {00,01}.
        0b1000100 | 0b1000101 => {
            Some(InsnKind::DataProc) // ADD / SUB / ADDS / SUBS (including CMP / CMN imm)
        }
        // Move wide (immediate): sf opc 100101 hw imm16 Rd.
        0b1001010 | 0b1001011 => {
            let sf = (word >> 31) & 1;
            let hw = (word >> 21) & 0x3;
            if sf == 0 && hw == 0b11 {
                return None; // 32-bit LSL#48 is unallocated
            }
            match (word >> 29) & 0x3 {
                0b00 => Some(InsnKind::DataProc), // MOVN
                0b10 => Some(InsnKind::DataProc), // MOVZ
                _ => None,                        // 01 unallocated; 11 = MOVK, out of scope
            }
        }
        // PC-relative addressing (ADR/ADRP): bits[28:24]=0b10000 — Wave 4 (U1-G1).
        // bits[23:22] are immhi[18:17] (either value); the class is exclusive
        // to PC-rel within data-processing-immediate (0x44+ = add/sub-imm…).
        0b1000000..=0b1000011 => Some(InsnKind::PcRel),
        // Logical (immediate): sf opc 100100 N ... bits[28:22] = 0b1001000 | 0b1001001
        0b1001000 | 0b1001001 => {
            Some(InsnKind::DataProc) // AND / ORR / EOR / ANDS (including TST imm)
        }
        // Bitfield, extract: out of scope.
        _ => None,
    }
}

/// Data-processing (register). Key = bits[28:24].
fn decode_dp_reg(word: u32) -> Option<InsnKind> {
    match (word >> 24) & 0x1F {
        // Add/subtract (shifted register): sf op S 01011 shift 0 Rm imm6 Rn Rd.
        0b01011 => {
            let shifted = (word >> 21) & 1 == 0;
            let shift = (word >> 22) & 0x3;
            if shifted && shift < 0b11 {
                Some(InsnKind::DataProc) // ADD / SUB / ADDS / SUBS (including CMP / CMN reg)
            } else {
                None // extended register / reserved shift: out of scope
            }
        }
        // Logical (shifted register): sf opc 01010 shift N Rm imm6 Rn Rd.
        0b01010 => {
            let sf = (word >> 31) & 1;
            let n = (word >> 21) & 1;
            let shift = (word >> 22) & 0x3;
            // sf==0 && N==1 is unallocated; shift==0b11 is reserved.
            let encoding_valid = (sf == 1 || n == 0) && shift < 0b11;
            if encoding_valid {
                Some(InsnKind::DataProc) // AND / BIC / ORR / ORN / EOR / EON / ANDS (TST) / BICS
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Loads/stores (GB-1 scope):
/// - Load/store pair: STP, LDP, LDPSW, STNP, LDNP
/// - Load/store literal: LDR (32/64-bit), LDRSW
/// - Load/store register (immediate, unsigned offset): LDR, STR, LDRSW
/// - Load/store register (immediate pre/post-indexed): LDR, STR, LDRSW
/// - Load/store register (register offset): LDR, STR, LDRSW
fn decode_ldst(word: u32) -> Option<InsnKind> {
    // 1. Load/store pair (STP, LDP, LDPSW, STNP, LDNP):
    // opc 101 V 0 index L imm7 Rt2 Rn Rt
    // with bits[29:25] == 0b10100 (which pins V=0, integer registers).
    if (word >> 25) & 0x1F == 0b10100 {
        let opc = (word >> 30) & 0x3;
        let is_load = (word >> 22) & 1 == 1;
        return match opc {
            0b00 => Some(InsnKind::LoadStore),            // 32-bit STP / LDP
            0b01 if is_load => Some(InsnKind::LoadStore), // LDPSW
            0b10 => Some(InsnKind::LoadStore),            // 64-bit STP / LDP
            _ => None,                                   // 01 with store or 11: unallocated
        };
    }

    // 2. Load/store literal:
    // opc 011 V 00 imm19 Rt with bits[29:24] == 0b011000 (V=0).
    if (word >> 24) & 0x3F == 0b011000 {
        let opc = (word >> 30) & 0x3;
        return match opc {
            0b00 | 0b01 | 0b10 => Some(InsnKind::LoadStore), // 32-bit LDR, 64-bit LDR, LDRSW
            _ => None,                                       // 11 = PRFM (out of scope)
        };
    }

    // 3. Load/store register (immediate, unsigned offset):
    // size 111 V 01 opc imm12 Rn Rt with bits[29:24] == 0b111001 (V=0).
    if (word >> 24) & 0x3F == 0b111001 {
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        return match opc {
            0b00 => Some(InsnKind::LoadStore),                   // STR (B, H, W, X)
            0b01 => Some(InsnKind::LoadStore),                   // LDR (B, H, W, X)
            0b10 if size_bits == 2 => Some(InsnKind::LoadStore), // LDRSW
            _ => None,
        };
    }

    // 4. Load/store register (immediate pre/post-indexed):
    // size 111 V 00 opc 0 imm9 type Rn Rt
    // with bits[29:24] == 0b111000, bit 21 == 0, and type in {0b01 (post), 0b11 (pre)}.
    // (type == 0b00 is unscaled LDUR/STUR, kept out of scope per existing tests).
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 0 {
        let idx_type = (word >> 10) & 0x3;
        if idx_type == 0b01 || idx_type == 0b11 {
            let size_bits = (word >> 30) & 0x3;
            let opc = (word >> 22) & 0x3;
            return match opc {
                0b00 => Some(InsnKind::LoadStore),                   // STR
                0b01 => Some(InsnKind::LoadStore),                   // LDR
                0b10 if size_bits == 2 => Some(InsnKind::LoadStore), // LDRSW
                _ => None,
            };
        }
    }

    // 5. Load/store register (register offset):
    // size 111 V 00 opc 1 Rm option S 10 Rn Rt
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 {
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        return match opc {
            0b00 => Some(InsnKind::LoadStore),                   // STR
            0b01 => Some(InsnKind::LoadStore),                   // LDR
            0b10 if size_bits == 2 => Some(InsnKind::LoadStore), // LDRSW
            _ => None,
        };
    }

    None
}

/// Load/store register pair: STP, LDP.
/// opc(2) 101 V(1) 0 mode(2) L(1) imm7(7) Rt2(5) Rn(5) Rt(5).
/// bits[29:25] == 0b10100 pins V=0 (integer registers; V=1 is SIMD).
fn decode_ldst_pair(word: u32) -> Option<InsnKind> {
    if (word >> 25) & 0x1F != 0b10100 {
        return None;
    }
    let mode = (word >> 23) & 0x3;
    if mode == 0 {
        return None; // 00 is unallocated
    }
    let opc = (word >> 30) & 0x3;
    if opc == 0b11 {
        return None; // 11 is unallocated
    }
    Some(InsnKind::LoadStore)
}

/// Branches + system instructions.
fn decode_branch_sys(word: u32) -> Option<InsnKind> {
    // B / BL: 000101 / 100101 imm26.
    match (word >> 26) & 0x3F {
        0b000101 | 0b100101 => return Some(InsnKind::Branch),
        _ => {}
    }
    // B.cond: 0101010 0 imm19 0 cond.
    // bits[31:24] == 0x54, bit 4 == 0, cond < 0b1111 (0..14).
    if (word >> 24) == 0x54 && (word & 0x10) == 0 {
        let cond = word & 0xF;
        if cond < 0b1111 {
            return Some(InsnKind::Branch);
        }
    }
    // CBZ / CBNZ: sf 011010 op imm19 Rt. bits[29:24] = 0b11010_op, so the op
    // bit (bit 24) distinguishes them: 0b110100 = CBZ, 0b110101 = CBNZ.
    if (word >> 24) & 0x3F == 0b110100 || (word >> 24) & 0x3F == 0b110101 {
        return Some(InsnKind::Branch);
    }
    // TBZ / TBNZ: b5 011011 op b40 imm14 Rt.
    // bits[30:25] == 0b011011.
    if (word >> 25) & 0x3F == 0b011011 {
        return Some(InsnKind::Branch);
    }
    // RET: 1101011 0 010 11111 000000 Rn 00000. Mask keeps everything except
    // the Rn field, so any RET <Xn> matches; BR (0xD61F…) / BLR (0xD63F…) do not.
    if word & 0xFFFF_FC1F == 0xD65F_0000 {
        return Some(InsnKind::Branch);
    }
    // System instructions: bits[31:22] == 0b1101_0101_00 (0x354).
    // Covers barriers (DMB, DSB, ISB), HINTs (NOP, WFI), MSR, MRS, SYS ops (DC, IC, TLBI).
    if (word >> 22) & 0x3FF == 0x354 {
        return Some(InsnKind::System);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathn_contracts::cpu::DecodeResult;

    fn ok_kind(word: u32) -> InsnKind {
        match decode(word) {
            DecodeResult::Ok(insn) => {
                assert_eq!(insn.word, word, "decoded word must echo input");
                assert_eq!(insn.addr, 0, "decode knows no address");
                insn.kind
            }
            DecodeResult::Illegal { .. } => panic!("0x{word:08X} should decode"),
        }
    }

    fn assert_illegal(word: u32) {
        assert_eq!(
            decode(word),
            DecodeResult::Illegal { word },
            "0x{word:08X} must be Illegal, never fake-decoded"
        );
    }

    // ---- data-processing immediate ----

    #[test]
    fn add_imm_x0_x1_0x10_is_dataproc() {
        assert_eq!(ok_kind(0x9100_4420), InsnKind::DataProc);
    }

    #[test]
    fn add_imm_w_is_dataproc() {
        assert_eq!(ok_kind(0x1100_4420), InsnKind::DataProc);
    }

    #[test]
    fn sub_imm_is_dataproc() {
        assert_eq!(ok_kind(0xD100_4420), InsnKind::DataProc);
    }

    #[test]
    fn add_imm_lsl12_is_dataproc() {
        assert_eq!(ok_kind(0x9140_4420), InsnKind::DataProc);
    }

    #[test]
    fn adds_imm_with_flags_is_dataproc() {
        assert_eq!(ok_kind(0xB100_4420), InsnKind::DataProc); // ADDS / CMN imm
    }

    #[test]
    fn add_imm_reserved_shift_is_illegal() {
        assert_illegal(0x9180_4420); // sh=0b10 is reserved
    }

    #[test]
    fn movz_x0_zero_is_dataproc() {
        assert_eq!(ok_kind(0xD280_0000), InsnKind::DataProc);
    }

    #[test]
    fn movz_w0_imm16_is_dataproc() {
        assert_eq!(ok_kind(0x52A2_4680), InsnKind::DataProc);
    }

    #[test]
    fn movn_x0_is_dataproc() {
        assert_eq!(ok_kind(0x9280_0000), InsnKind::DataProc);
    }

    #[test]
    fn movk_is_illegal_out_of_scope() {
        assert_illegal(0xF2A2_4680); // real MOVK, but LLD lists only MOVZ/MOVN
    }

    #[test]
    fn movz_32bit_hw3_unallocated_is_illegal() {
        assert_illegal(0x52E2_4680); // sf=0, hw=0b11 is unallocated
    }

    #[test]
    fn orr_imm_logical_is_dataproc() {
        assert_eq!(ok_kind(0x3200_03E0), InsnKind::DataProc); // logical-imm
    }

    // ---- PC-relative (Wave 4: U1-G1) ----

    #[test]
    fn pcrel_adrp_vectors_are_pcrel() {
        assert_eq!(ok_kind(0xB000_000A), InsnKind::PcRel); // ADRP X10, page+1 (real guest entry word)
        assert_eq!(ok_kind(0xF0FF_FFE0), InsnKind::PcRel); // ADRP X0, page-1
        assert_eq!(ok_kind(0x9000_0005), InsnKind::PcRel); // ADRP X5, page+0
    }

    #[test]
    fn pcrel_adr_is_pcrel() {
        assert_eq!(ok_kind(0x1000_0000), InsnKind::PcRel); // ADR X0, #0
        assert_eq!(ok_kind(0x7000_001F), InsnKind::PcRel); // ADR X31, #3 (immlo=0b11)
    }

    // ---- data-processing register ----

    #[test]
    fn add_reg_is_dataproc() {
        assert_eq!(ok_kind(0x8B02_0020), InsnKind::DataProc);
    }

    #[test]
    fn sub_reg_is_dataproc() {
        assert_eq!(ok_kind(0xCB02_0020), InsnKind::DataProc);
    }

    #[test]
    fn orr_reg_is_dataproc() {
        assert_eq!(ok_kind(0xAA02_0020), InsnKind::DataProc);
    }

    #[test]
    fn eor_reg_is_dataproc() {
        assert_eq!(ok_kind(0xCA02_0020), InsnKind::DataProc);
    }

    #[test]
    fn adds_reg_with_flags_is_dataproc() {
        assert_eq!(ok_kind(0xAB02_0020), InsnKind::DataProc); // ADDS / CMN reg
    }

    #[test]
    fn add_reg_extended_is_illegal() {
        assert_illegal(0x8B20_2020); // bit21=1: extend form, out of scope
    }

    #[test]
    fn and_reg_is_dataproc() {
        assert_eq!(ok_kind(0x8A02_0020), InsnKind::DataProc); // AND / TST reg
    }

    #[test]
    fn orr_w_unallocated_n_is_illegal() {
        assert_illegal(0x2A22_2020); // sf=0, N=1 is unallocated
    }

    // ---- loads / stores ----

    #[test]
    fn str_w0_x1_is_loadstore() {
        assert_eq!(ok_kind(0xB900_0020), InsnKind::LoadStore);
    }

    #[test]
    fn ldr_x0_x1_is_loadstore() {
        assert_eq!(ok_kind(0xF940_0020), InsnKind::LoadStore);
    }

    #[test]
    fn ldr_w0_x1_is_loadstore() {
        assert_eq!(ok_kind(0xB940_0020), InsnKind::LoadStore);
    }

    #[test]
    fn str_x0_sp_offset_is_loadstore() {
        assert_eq!(ok_kind(0xF900_03E0), InsnKind::LoadStore);
    }

    #[test]
    fn stur_unscaled_is_illegal() {
        assert_illegal(0xB800_0020); // unscaled-imm form: out of scope
    }

    #[test]
    fn simd_str_is_illegal() {
        assert_illegal(0x3D00_0020); // V=1: SIMD, out of scope
    }

    // ---- GB-1: STP/LDP pair, indexed LDR/STR, literal, LDRSW ----

    #[test]
    fn gb1_stp_x21_x1_x0_is_loadstore() {
        // Kernel instruction 6: stp x21, x1, [x0]
        assert_eq!(ok_kind(0xA900_0415), InsnKind::LoadStore);
    }

    #[test]
    fn gb1_stp_offset_and_indexed_are_loadstore() {
        assert_eq!(ok_kind(0xA901_0C02), InsnKind::LoadStore); // stp x2, x3, [x0, #16]
        assert_eq!(ok_kind(0xA881_0C02), InsnKind::LoadStore); // stp x2, x3, [x0], #16 (post-index)
        assert_eq!(ok_kind(0xA981_0C02), InsnKind::LoadStore); // stp x2, x3, [x0, #16]! (pre-index)
    }

    #[test]
    fn gb1_ldp_variants_are_loadstore() {
        assert_eq!(ok_kind(0xA940_0415), InsnKind::LoadStore); // ldp x21, x1, [x0]
        assert_eq!(ok_kind(0xA8C1_0C02), InsnKind::LoadStore); // ldp x2, x3, [x0], #16 (post-index)
        assert_eq!(ok_kind(0xA9C1_0C02), InsnKind::LoadStore); // ldp x2, x3, [x0, #16]! (pre-index)
        assert_eq!(ok_kind(0x2900_0C02), InsnKind::LoadStore); // stp w2, w3, [x0] (32-bit)
        assert_eq!(ok_kind(0x2940_0C02), InsnKind::LoadStore); // ldp w2, w3, [x0] (32-bit)
        assert_eq!(ok_kind(0x6940_0C02), InsnKind::LoadStore); // ldpsw x2, x3, [x0]
    }

    #[test]
    fn gb1_stnp_ldnp_are_loadstore() {
        assert_eq!(ok_kind(0xA800_0440), InsnKind::LoadStore); // stnp x0, x1, [x2]
        assert_eq!(ok_kind(0xA840_0440), InsnKind::LoadStore); // ldnp x0, x1, [x2]
    }

    #[test]
    fn gb1_ldr_literal_variants_are_loadstore() {
        assert_eq!(ok_kind(0x18FF_FFE3), InsnKind::LoadStore); // ldr w3, [pc, #-4]
        assert_eq!(ok_kind(0x5800_0040), InsnKind::LoadStore); // ldr x0, [pc, #8]
        assert_eq!(ok_kind(0x9800_0020), InsnKind::LoadStore); // ldrsw x0, label
    }

    #[test]
    fn gb1_indexed_ldr_str_are_loadstore() {
        assert_eq!(ok_kind(0xF840_8C20), InsnKind::LoadStore); // ldr x0, [x1, #8]! (pre-index)
        assert_eq!(ok_kind(0xF840_8420), InsnKind::LoadStore); // ldr x0, [x1], #8 (post-index)
        assert_eq!(ok_kind(0xF800_8C20), InsnKind::LoadStore); // str x0, [x1, #8]! (pre-index)
        assert_eq!(ok_kind(0xF800_8420), InsnKind::LoadStore); // str x0, [x1], #8 (post-index)
        assert_eq!(ok_kind(0xB840_4C20), InsnKind::LoadStore); // ldr w0, [x1, #4]! (pre-index)
        assert_eq!(ok_kind(0xB840_4420), InsnKind::LoadStore); // ldr w0, [x1], #4 (post-index)
    }

    #[test]
    fn gb1_ldrsw_variants_are_loadstore() {
        assert_eq!(ok_kind(0xB980_0420), InsnKind::LoadStore); // ldrsw x0, [x1, #4] (unsigned offset)
        assert_eq!(ok_kind(0xB880_4C20), InsnKind::LoadStore); // ldrsw x0, [x1, #4]! (pre-index)
        assert_eq!(ok_kind(0xB880_4420), InsnKind::LoadStore); // ldrsw x0, [x1], #4 (post-index)
    }

    #[test]
    fn gb1_reg_offset_are_loadstore() {
        assert_eq!(ok_kind(0xF862_6820), InsnKind::LoadStore); // ldr x0, [x1, x2]
        assert_eq!(ok_kind(0xF822_6820), InsnKind::LoadStore); // str x0, [x1, x2]
    }

    // ---- branches ----

    #[test]
    fn b_forward_is_branch() {
        assert_eq!(ok_kind(0x1400_0001), InsnKind::Branch);
    }

    #[test]
    fn bl_call_is_branch() {
        assert_eq!(ok_kind(0x97FF_FFFE), InsnKind::Branch);
    }

    #[test]
    fn cbz_w_is_branch() {
        assert_eq!(ok_kind(0x3400_0020), InsnKind::Branch);
    }

    #[test]
    fn cbnz_w_is_branch() {
        assert_eq!(ok_kind(0x3500_0020), InsnKind::Branch); // CBNZ W0 (genuine op=1 encoding)
    }

    #[test]
    fn cbz_x_is_branch() {
        assert_eq!(ok_kind(0xB400_0020), InsnKind::Branch);
    }

    #[test]
    fn cbnz_x_is_branch() {
        assert_eq!(ok_kind(0xB500_0020), InsnKind::Branch); // CBNZ X0 (genuine op=1 encoding)
    }

    #[test]
    fn cbnz_real_guest_word_is_branch() {
        // The real guest's read_loop: CBNZ X0, got_byte (0x4000006c).
        assert_eq!(ok_kind(0xB500_0060), InsnKind::Branch);
    }

    #[test]
    fn ret_x30_is_branch() {
        assert_eq!(ok_kind(0xD65F_03C0), InsnKind::Branch);
    }

    #[test]
    fn ret_any_register_is_branch() {
        assert_eq!(ok_kind(0xD65F_0120), InsnKind::Branch); // RET X9
    }

    #[test]
    fn br_x0_is_illegal() {
        assert_illegal(0xD61F_0000); // BR: out of scope, must not match RET mask
    }

    #[test]
    fn blr_is_illegal() {
        assert_illegal(0xD63F_0000); // BLR: out of scope
    }

    // ---- system ----

    #[test]
    fn nop_is_system() {
        assert_eq!(ok_kind(0xD503_201F), InsnKind::System);
    }

    #[test]
    fn hint_yield_is_system() {
        assert_eq!(ok_kind(0xD503_203F), InsnKind::System); // HINT #1
    }

    // ---- GB-2: branches + flags golden words ----


    #[test]
    fn dmb_sy_is_system() {
        assert_eq!(ok_kind(0xD503_3FBF), InsnKind::System); // DMB sy
    }

    #[test]
    fn dmb_ish_is_system() {
        assert_eq!(ok_kind(0xD503_3BBF), InsnKind::System); // DMB ish
    }

    #[test]
    fn dsb_sy_is_system() {
        assert_eq!(ok_kind(0xD503_3F9F), InsnKind::System); // DSB sy
    }

    #[test]
    fn dsb_ishst_is_system() {
        assert_eq!(ok_kind(0xD503_3A9F), InsnKind::System); // DSB ishst
    }

    #[test]
    fn isb_is_system() {
        assert_eq!(ok_kind(0xD503_3FDF), InsnKind::System); // ISB
    }

    #[test]
    fn mrs_currentel_is_system() {
        assert_eq!(ok_kind(0xD538_4240), InsnKind::System); // MRS X0, CurrentEL
    }

    #[test]
    fn mrs_daif_is_system() {
        assert_eq!(ok_kind(0xD53B_4220), InsnKind::System); // MRS X0, DAIF
    }

    #[test]
    fn msr_daif_is_system() {
        assert_eq!(ok_kind(0xD51B_4220), InsnKind::System); // MSR DAIF, X0
    }

    #[test]
    fn mrs_nzcv_is_system() {
        assert_eq!(ok_kind(0xD53B_4200), InsnKind::System); // MRS X0, NZCV
    }

    #[test]
    fn msr_nzcv_is_system() {
        assert_eq!(ok_kind(0xD51B_4200), InsnKind::System); // MSR NZCV, X0
    }

    #[test]
    fn mrs_tpidr_el1_is_system() {
        assert_eq!(ok_kind(0xD538_D080), InsnKind::System); // MRS X0, TPIDR_EL1
    }

    #[test]
    fn msr_tpidr_el1_is_system() {
        assert_eq!(ok_kind(0xD518_D080), InsnKind::System); // MSR TPIDR_EL1, X0
    }

    #[test]
    fn mrs_ctr_el0_is_system() {
        assert_eq!(ok_kind(0xD53B_0023), InsnKind::System); // MRS X3, CTR_EL0
    }

    #[test]
    fn msr_spsel_is_system() {
        assert_eq!(ok_kind(0xD500_41BF), InsnKind::System); // MSR SPSel, #1
    }

    #[test]
    fn b_cond_all_14_conditions_are_branch() {
        // B.cond: 0x54000000 | (imm19 << 5) | cond
        for cond in 0..=14u32 {
            let word = 0x5400_0000 | cond;
            assert_eq!(
                ok_kind(word),
                InsnKind::Branch,
                "B.cond with cond {cond} must decode as Branch"
            );
        }
    }

    #[test]
    fn b_cond_backward_target_is_branch() {
        // Real guest word: 0x54ffff61 (B.NE -5)
        assert_eq!(ok_kind(0x54FF_FF61), InsnKind::Branch);
    }

    #[test]
    fn tbz_and_tbnz_are_branch() {
        // TBZ W0, #0, +4
        assert_eq!(ok_kind(0x3600_0020), InsnKind::Branch);
        // TBNZ W1, #31, +4
        assert_eq!(ok_kind(0x37F8_0021), InsnKind::Branch);
        // TBZ X2, #63, +4
        assert_eq!(ok_kind(0xB6F8_0022), InsnKind::Branch);
        // TBNZ X3, #32, +4
        assert_eq!(ok_kind(0xB700_0023), InsnKind::Branch);
    }

    #[test]
    fn cmp_cmn_tst_are_dataproc() {
        // CMP X0, #1 (SUBS XZR, X0, #1)
        assert_eq!(ok_kind(0xF100_041F), InsnKind::DataProc);
        // CMN W0, #1 (ADDS WZR, W0, #1)
        assert_eq!(ok_kind(0xB100_041F), InsnKind::DataProc);
        // CMP X0, X1 (SUBS XZR, X0, X1)
        assert_eq!(ok_kind(0xEB01_001F), InsnKind::DataProc);
        // TST X0, X1 (ANDS XZR, X0, X1)
        assert_eq!(ok_kind(0xEA01_001F), InsnKind::DataProc);
        // TST X1, X3 (real guest word: 0xea03003f)
        assert_eq!(ok_kind(0xEA03_003F), InsnKind::DataProc);
        // SUBS X1, X1, #0x40 (real guest word: 0xf1010021)
        assert_eq!(ok_kind(0xF101_0021), InsnKind::DataProc);
    }

    // ---- acceptance: LLD spot-checks ----

    #[test]
    fn all_ones_is_illegal() {
        assert_illegal(0xFFFF_FFFF); // LLD acceptance: decode(0xFFFFFFFF) == Illegal
    }

    #[test]
    fn zero_word_is_illegal() {
        assert_illegal(0x0000_0000);
    }

    #[test]
    fn simd_group_is_illegal() {
        assert_illegal(0x0E20_8800); // bits[28:25] = 0b0111: SIMD&FP
    }

    #[test]
    fn decode_is_deterministic_on_goldens() {
        for w in [
            0x9100_4420,
            0xD280_0000,
            0x8B02_0020,
            0xAA02_0020,
            0xB900_0020,
            0xF940_0020,
            0x1400_0001,
            0xD65F_03C0,
            0xD503_201F,
            0xFFFF_FFFF,
        ] {
            assert_eq!(decode(w), decode(w), "decode(0x{w:08X}) not deterministic");
        }
    }

    // ---- fuzz: LLD acceptance — random words never panic, decode is pure ----

    /// Deterministic xorshift64* — no external crates, no entropy, reproducible.
    struct XorShift64(u64);
    impl XorShift64 {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
    }

    #[test]
    fn fuzz_decode_never_panics_and_is_deterministic() {
        let mut rng = XorShift64(0x1234_5678_9ABC_DEF0);
        for _ in 0..60_000 {
            let w = rng.next() as u32;
            let a = decode(w);
            let b = decode(w);
            assert_eq!(a, b, "decode(0x{w:08X}) not deterministic");
            if let DecodeResult::Illegal { word } = a {
                assert_eq!(word, w, "Illegal must echo the input word");
            }
        }
    }
}
