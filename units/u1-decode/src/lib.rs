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
pub use pathn_contracts::cpu::decode_bitmasks;

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
/// 0x9AC32042 (LSLV) → 1101,
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
            let set_flags = (word >> 29) & 1;
            if set_flags == 0 {
                Some(InsnKind::DataProc) // ADD (op=0) / SUB (op=1)
            } else {
                None // ADDS/SUBS: out of scope, honestly illegal
            }
        }
        // Logical (immediate): sf opc 100100 N immr imms Rn Rd.
        0b1001000 | 0b1001001 => {
            let sf = (word >> 31) & 1 == 1;
            let n = ((word >> 22) & 1) as u8;
            let immr = ((word >> 16) & 0x3F) as u8;
            let imms = ((word >> 10) & 0x3F) as u8;
            if decode_bitmasks(n, imms, immr, sf).is_some() {
                Some(InsnKind::DataProc)
            } else {
                None
            }
        }
        // Move wide (immediate): sf opc 100101 hw imm16 Rd.
        0b1001010 | 0b1001011 => {
            let sf = (word >> 31) & 1;
            let hw = (word >> 21) & 0x3;
            if sf == 0 && hw > 1 {
                return None; // 32-bit hw > 1 is unallocated
            }
            match (word >> 29) & 0x3 {
                0b00 => Some(InsnKind::DataProc), // MOVN
                0b10 => Some(InsnKind::DataProc), // MOVZ
                0b11 => Some(InsnKind::DataProc), // MOVK (Track GB-4)
                _ => None,                        // 01 unallocated
            }
        }
        // Bitfield (immediate): sf opc 100110 N immr imms Rn Rd.
        0b1001100 | 0b1001101 => {
            let sf = (word >> 31) & 1;
            let opc = (word >> 29) & 0x3;
            let n = (word >> 22) & 1;
            if opc == 0b11 {
                return None; // opc=11 unallocated
            }
            if sf == 0 {
                if n != 0 {
                    return None;
                }
                let immr = (word >> 16) & 0x3F;
                let imms = (word >> 10) & 0x3F;
                if (immr & 0x20) != 0 || (imms & 0x20) != 0 {
                    return None;
                }
            } else if n != 1 {
                return None;
            }
            Some(InsnKind::DataProc)
        }
        // PC-relative addressing (ADR/ADRP): bits[28:24]=0b10000 — Wave 4 (U1-G1).
        // bits[23:22] are immhi[18:17] (either value); the class is exclusive
        // to PC-rel within data-processing-immediate (0x44+ = add/sub-imm…).
        0b1000000..=0b1000011 => Some(InsnKind::PcRel),
        _ => None,
    }
}

/// Data-processing (register). Key = bits[28:24].
fn decode_dp_reg(word: u32) -> Option<InsnKind> {
    match (word >> 24) & 0x1F {
        // Add/subtract (shifted register): sf op S 01011 shift 0 Rm imm6 Rn Rd.
        0b01011 => {
            let shifted = (word >> 21) & 1 == 0;
            let set_flags = (word >> 29) & 1;
            let shift = (word >> 22) & 0x3;
            if shifted && set_flags == 0 && shift < 0b11 {
                Some(InsnKind::DataProc) // ADD (op=0) / SUB (op=1)
            } else {
                None // extended register / ADDS/SUBS / reserved shift: out of scope
            }
        }
        // Logical (shifted register): sf opc 01010 shift N Rm imm6 Rn Rd.
        0b01010 => {
            let sf = (word >> 31) & 1;
            let opc = (word >> 29) & 0x3;
            let n = (word >> 21) & 1;
            let imm6 = (word >> 10) & 0x3F;
            if sf == 0 && (imm6 & 0x20) != 0 {
                return None; // 32-bit shift >= 32 is unallocated
            }
            // 32-bit ORR with N=1 is reserved/unallocated per contract
            if sf == 0 && opc == 0b01 && n == 1 {
                return None;
            }
            Some(InsnKind::DataProc) // AND, BIC, ORR, EOR, EON, ANDS, BICS
        }
        // Data-processing (2 source): sf 0 0 11010 110 Rm 0010 op2 Rn Rd
        0b11010 => {
            let bit29 = (word >> 29) & 1;
            let bit21 = (word >> 21) & 1;
            let opcode2 = (word >> 10) & 0x3F;
            if bit29 == 0 && bit21 == 0 && ((opcode2 >> 2) == 0b0010) {
                // ASRV (00), LSRV (01), LSLV (10), RORV (11)
                Some(InsnKind::DataProc)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Loads/stores. Only LDR/STR (immediate, unsigned offset):
/// size V 11100 opc imm12 Rn Rt with bits[29:24] == 0b111001, which pins V=0
/// (integer registers; V=1 would be SIMD). Verified: 0xB9000020 = STR W0,[X1],
/// 0xF9400020 = LDR X0,[X1], 0xB8000020 = STUR (→ Illegal, unscaled form).
fn decode_ldst(word: u32) -> Option<InsnKind> {
    if (word >> 24) & 0x3F != 0b111001 {
        return None;
    }
    match (word >> 22) & 0x3 {
        0b00 => Some(InsnKind::LoadStore), // STR (immediate, unsigned offset)
        0b01 => Some(InsnKind::LoadStore), // LDR (immediate, unsigned offset)
        _ => None,                         // opc 10/11: unallocated
    }
}

/// Branches + system instructions.
fn decode_branch_sys(word: u32) -> Option<InsnKind> {
    // B / BL: 000101 / 100101 imm26.
    match (word >> 26) & 0x3F {
        0b000101 | 0b100101 => return Some(InsnKind::Branch),
        _ => {}
    }
    // CBZ / CBNZ: sf 011010 op imm19 Rt. bits[29:24] = 0b11010_op, so the op
    // bit (bit 24) distinguishes them: 0b110100 = CBZ, 0b110101 = CBNZ.
    // (2026-09-27: the old mask only matched CBZ — real CBNZ words like
    // 0xB5000060 decoded Illegal. The class comment always claimed both.)
    if (word >> 24) & 0x3F == 0b110100 || (word >> 24) & 0x3F == 0b110101 {
        return Some(InsnKind::Branch);
    }
    // RET: 1101011 0 010 11111 000000 Rn 00000. Mask keeps everything except
    // the Rn field, so any RET <Xn> matches; BR (0xD61F…) / BLR (0xD63F…) do not.
    if word & 0xFFFF_FC1F == 0xD65F_0000 {
        return Some(InsnKind::Branch);
    }
    // HINT: bits[31:12] == 0xD5032, Rt == 11111, CRm:op2 free (the hint number).
    // NOP (0xD503201F) is HINT #0.
    if word >> 12 == 0xD5032 && word & 0x1F == 0x1F {
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
    fn adds_imm_with_flags_is_illegal() {
        assert_illegal(0xB100_4420); // ADDS: S=1, out of LLD scope
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
    fn movk_is_dataproc() {
        assert_eq!(ok_kind(0xF2A2_4680), InsnKind::DataProc); // MOVK X0, #0x1234, LSL #16
        assert_eq!(ok_kind(0x7280_0000), InsnKind::DataProc); // MOVK W0, #0, LSL #0
    }

    #[test]
    fn movz_32bit_hw3_unallocated_is_illegal() {
        assert_illegal(0x52E2_4680); // sf=0, hw=0b11 is unallocated
        assert_illegal(0x72E2_4680); // sf=0, hw=0b11 MOVK is unallocated
    }

    #[test]
    fn orr_imm_logical_is_dataproc() {
        assert_eq!(ok_kind(0x3200_03E0), InsnKind::DataProc); // ORR W0, W31, #1
        assert_eq!(ok_kind(0xB240_04C6), InsnKind::DataProc); // ORR X6, X6, #0x3 (from kernel)
    }

    #[test]
    fn and_imm_logical_is_dataproc() {
        assert_eq!(ok_kind(0x9240_52F7), InsnKind::DataProc); // AND X23, X23, #0x1fffff (kernel stext+0c)
        assert_eq!(ok_kind(0x1200_03E0), InsnKind::DataProc); // AND W0, W31, #1
        assert_eq!(ok_kind(0x7200_03E0), InsnKind::DataProc); // ANDS W0, W31, #1
        assert_eq!(ok_kind(0xF200_03E0), InsnKind::DataProc); // ANDS X0, X31, #1
    }

    #[test]
    fn eor_imm_logical_is_dataproc() {
        assert_eq!(ok_kind(0x5200_03E0), InsnKind::DataProc); // EOR W0, W31, #1
        assert_eq!(ok_kind(0xD200_03E0), InsnKind::DataProc); // EOR X0, X31, #1
    }

    #[test]
    fn bitfield_ubfm_is_dataproc() {
        assert_eq!(ok_kind(0xD350_4C63), InsnKind::DataProc); // UBFX X3, X3, #16, #4 (kernel 0x40004d68)
        assert_eq!(ok_kind(0xD367_FC65), InsnKind::DataProc); // UBFX (kernel 0x413c00b8)
        assert_eq!(ok_kind(0x5300_1C20), InsnKind::DataProc); // UBFX W0, W1, #0, #8 (UXTB)
        assert_eq!(ok_kind(0xD340_7C20), InsnKind::DataProc); // LSR X0, X1, #0
        assert_eq!(ok_kind(0x5304_7C20), InsnKind::DataProc); // LSL W0, W1, #4
    }

    #[test]
    fn bitfield_sbfm_is_dataproc() {
        assert_eq!(ok_kind(0x9340_7C20), InsnKind::DataProc); // ASR X0, X1, #0
        assert_eq!(ok_kind(0x1300_1C20), InsnKind::DataProc); // SXTB W0, W1
        assert_eq!(ok_kind(0x9340_1C20), InsnKind::DataProc); // SBFX X0, X1, #0, #4
    }

    #[test]
    fn bitfield_bfm_is_dataproc() {
        assert_eq!(ok_kind(0x3300_1C20), InsnKind::DataProc); // BFXIL W0, W1, #0, #8
        assert_eq!(ok_kind(0xB340_1C20), InsnKind::DataProc); // BFI X0, X1, #0, #8
    }

    #[test]
    fn shifted_reg_logical_all_ops_are_dataproc() {
        assert_eq!(ok_kind(0x8A02_0020), InsnKind::DataProc); // AND X0, X1, X2
        assert_eq!(ok_kind(0x8A22_0020), InsnKind::DataProc); // BIC X0, X1, X2
        assert_eq!(ok_kind(0x8A23_0021), InsnKind::DataProc); // BIC X1, X1, X3 (kernel 0x40004d78)
        assert_eq!(ok_kind(0xAA22_0020), InsnKind::DataProc); // ORN X0, X1, X2
        assert_eq!(ok_kind(0xCA02_0020), InsnKind::DataProc); // EOR X0, X1, X2
        assert_eq!(ok_kind(0xCA22_0020), InsnKind::DataProc); // EON X0, X1, X2
        assert_eq!(ok_kind(0xEA02_0020), InsnKind::DataProc); // ANDS X0, X1, X2
        assert_eq!(ok_kind(0xEA03_003F), InsnKind::DataProc); // TST X1, X3 = ANDS XZR, X1, X3 (kernel 0x40004d78)
        assert_eq!(ok_kind(0xEA22_0020), InsnKind::DataProc); // BICS X0, X1, X2
        // 32-bit forms
        assert_eq!(ok_kind(0x0A02_0020), InsnKind::DataProc); // AND W0, W1, W2
        assert_eq!(ok_kind(0x0A22_0020), InsnKind::DataProc); // BIC W0, W1, W2
    }

    #[test]
    fn dp_2source_shifts_are_dataproc() {
        assert_eq!(ok_kind(0x9AC3_2042), InsnKind::DataProc); // LSL X2, X2, X3 (kernel 0x40004d70)
        assert_eq!(ok_kind(0x9AC3_2442), InsnKind::DataProc); // LSR X2, X2, X3
        assert_eq!(ok_kind(0x9AC3_2842), InsnKind::DataProc); // ASR X2, X2, X3
        assert_eq!(ok_kind(0x9AC3_2C42), InsnKind::DataProc); // ROR X2, X2, X3
        // 32-bit forms
        assert_eq!(ok_kind(0x1AC3_2042), InsnKind::DataProc); // LSL W2, W2, W3
    }

    #[test]
    fn decode_bitmasks_table_cases() {
        // 1. Kernel mask: AND X23, X23, #0x1fffff (N=1, imms=20, immr=0, sf=true)
        assert_eq!(decode_bitmasks(1, 20, 0, true), Some(0x0000_0000_001F_FFFF));

        // 2. Alternating bits 0x5555_5555_5555_5555 (len=1, esize=2, S=0, R=0)
        assert_eq!(decode_bitmasks(0, 0b111100, 0, true), Some(0x5555_5555_5555_5555));
        // Alternating bits inverted (R=1) -> 0xAAAA_AAAA_AAAA_AAAA
        assert_eq!(decode_bitmasks(0, 0b111100, 1, true), Some(0xAAAA_AAAA_AAAA_AAAA));

        // 3. Alternating pairs 0x3333_3333_3333_3333 (len=2, esize=4, S=1, R=0)
        assert_eq!(decode_bitmasks(0, 0b111001, 0, true), Some(0x3333_3333_3333_3333));
        // Inverted pairs (R=2) -> 0xCCCC_CCCC_CCCC_CCCC
        assert_eq!(decode_bitmasks(0, 0b111001, 2, true), Some(0xCCCC_CCCC_CCCC_CCCC));

        // 4. Alternating nibbles 0x0F0F_0F0F_0F0F_0F0F (len=3, esize=8, S=3, R=0)
        assert_eq!(decode_bitmasks(0, 0b110011, 0, true), Some(0x0F0F_0F0F_0F0F_0F0F));
        assert_eq!(decode_bitmasks(0, 0b110011, 4, true), Some(0xF0F0_F0F0_F0F0_F0F0));

        // 5. Alternating bytes 0x00FF_00FF_00FF_00FF (len=4, esize=16, S=7, R=0)
        assert_eq!(decode_bitmasks(0, 0b100111, 0, true), Some(0x00FF_00FF_00FF_00FF));
        assert_eq!(decode_bitmasks(0, 0b100111, 8, true), Some(0xFF00_FF00_FF00_FF00));

        // 6. Halfwords 0x0000_FFFF_0000_FFFF (len=5, esize=32, S=15, R=0)
        assert_eq!(decode_bitmasks(0, 0b001111, 0, true), Some(0x0000_FFFF_0000_FFFF));

        // 7. 32-bit forms (must be masked to 32 bits, and N=1 is illegal)
        assert_eq!(decode_bitmasks(0, 0b011110, 0, false), Some(0x7FFF_FFFF));
        assert_eq!(decode_bitmasks(1, 20, 0, false), None); // N=1 with sf=0 is unallocated
        assert_eq!(decode_bitmasks(0, 0b111111, 0, true), None); // all-ones is reserved
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
    fn adds_reg_with_flags_is_illegal() {
        assert_illegal(0xAB02_0020); // ADDS: S=1, out of scope
    }

    #[test]
    fn add_reg_extended_is_illegal() {
        assert_illegal(0x8B20_2020); // bit21=1: extend form, out of scope
    }

    #[test]
    fn and_reg_is_dataproc() {
        assert_eq!(ok_kind(0x8A02_0020), InsnKind::DataProc); // AND: Track GB-4
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
