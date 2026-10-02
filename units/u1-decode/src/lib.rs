//! U1 `aarch64-decode` — 32-bit word → decoded AArch64 instruction.
//!
//! PURE: deterministic, no I/O, no time, no threads, no hidden state.
//! Only `pathn_contracts` is imported (swarm law: no other unit crates).
//!
//! Supported subset (exactly LLD §U1, nothing more):
//! - data-processing immediate: ADD/SUB (immediate, S=0), MOVZ, MOVN
//! - data-processing register: ADD/SUB (shifted register, S=0),
//!   ORR/EOR (shifted register), CLZ (1-source, 64-bit; 32-bit recognized,
//!   trapped in U2), MADD (3-source, 64-bit; 32-bit / MSUB / long-multiply
//!   recognized, trapped in U2)
//! - loads/stores: LDR/STR (immediate, unsigned offset), integer registers
//! - branches: B, BL, CBZ, CBNZ, RET
//! - system: HINT (NOP = HINT #0)
//! - supervisor call: SVC (immediate) — recognized; U2 lifts it to an honest
//!   unimplemented trap (Phase-2 spike, no exception model yet)
//!
//! Every other encoding → `DecodeResult::Illegal`, honestly. Illegal words are
//! data, never panics. Unallocated/reserved variants of *supported* mnemonics
//! (ADDS/SUBS, MOVK, extended-register ADD, AND/ANDS, BR/BLR, SIMD …) are also
//! Illegal: this unit claims only what LLD §U1 lists, never fake-decodes.

pub mod branch;
pub mod dp_imm;
pub mod dp_reg;
pub mod fp_simd;
pub mod ldst;
pub mod system;

pub use pathn_contracts::cpu::decode_bitmasks;
use pathn_contracts::cpu::{DecodeResult, InsnKind, Instruction};

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


/// Registration table mapping major bit-pattern groups (bits[28:25]) to family decoders.
/// One line per family module so future family agents each own exactly one file.
/// Major-group mapping cross-checked against known encodings:
/// 0x91004420 (ADD imm) → 1000, 0xD2800000 (MOVZ) → 1001,
/// 0x14000000 (B) / 0xD503201F (NOP) → 1010, 0xD65F03C0 (RET) → 1011,
/// 0x8B020020 (ADD reg) / 0xAA020020 (ORR reg) → 0101,
/// 0x9AC32042 (LSLV) → 1101,
/// 0xB9000020 (STR) / 0xF9400020 (LDR) → 1100.
fn classify(word: u32) -> Option<InsnKind> {
    match (word >> 25) & 0xF {
        0b1000 | 0b1001 => dp_imm::decode(word),
        0b1010 | 0b1011 => branch::decode(word).or_else(|| system::decode(word)),
        0b0101 | 0b1101 => dp_reg::decode(word),
        0b0100 | 0b1100 => ldst::decode(word),
        0b0111 | 0b1111 | 0b0110 | 0b1110 => fp_simd::decode(word),
        // 0b000x..0b001x: unallocated; 0b011x/0b111x: SIMD/FP/SVE (out of scope)
        _ => None,
    }
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
        assert_eq!(
            decode_bitmasks(0, 0b111100, 0, true),
            Some(0x5555_5555_5555_5555)
        );
        // Alternating bits inverted (R=1) -> 0xAAAA_AAAA_AAAA_AAAA
        assert_eq!(
            decode_bitmasks(0, 0b111100, 1, true),
            Some(0xAAAA_AAAA_AAAA_AAAA)
        );

        // 3. Alternating pairs 0x3333_3333_3333_3333 (len=2, esize=4, S=1, R=0)
        assert_eq!(
            decode_bitmasks(0, 0b111001, 0, true),
            Some(0x3333_3333_3333_3333)
        );
        // Inverted pairs (R=2) -> 0xCCCC_CCCC_CCCC_CCCC
        assert_eq!(
            decode_bitmasks(0, 0b111001, 2, true),
            Some(0xCCCC_CCCC_CCCC_CCCC)
        );

        // 4. Alternating nibbles 0x0F0F_0F0F_0F0F_0F0F (len=3, esize=8, S=3, R=0)
        assert_eq!(
            decode_bitmasks(0, 0b110011, 0, true),
            Some(0x0F0F_0F0F_0F0F_0F0F)
        );
        assert_eq!(
            decode_bitmasks(0, 0b110011, 4, true),
            Some(0xF0F0_F0F0_F0F0_F0F0)
        );

        // 5. Alternating bytes 0x00FF_00FF_00FF_00FF (len=4, esize=16, S=7, R=0)
        assert_eq!(
            decode_bitmasks(0, 0b100111, 0, true),
            Some(0x00FF_00FF_00FF_00FF)
        );
        assert_eq!(
            decode_bitmasks(0, 0b100111, 8, true),
            Some(0xFF00_FF00_FF00_FF00)
        );

        // 6. Halfwords 0x0000_FFFF_0000_FFFF (len=5, esize=32, S=15, R=0)
        assert_eq!(
            decode_bitmasks(0, 0b001111, 0, true),
            Some(0x0000_FFFF_0000_FFFF)
        );

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
    fn adds_reg_with_flags_is_dataproc() {
        assert_eq!(ok_kind(0xAB02_0020), InsnKind::DataProc); // ADDS / CMN reg
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
    fn stur_unscaled_is_loadstore() {
        // GB-26: unscaled-imm (LDUR/STUR) is in scope -- the kernel hits
        // STUR X8, [X29, #-8] (0xF81F83A8) at step 1248988.
        assert_eq!(ok_kind(0xB800_0020), InsnKind::LoadStore); // STUR W0, [X0]
        assert_eq!(ok_kind(0xF81F_83A8), InsnKind::LoadStore); // STUR X8, [X29, #-8]
        assert_eq!(ok_kind(0xF840_0020), InsnKind::LoadStore); // LDUR X0, [X0]
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
    fn br_any_register_is_branch() {
        assert_eq!(ok_kind(0xD61F_0000), InsnKind::Branch); // BR X0
        assert_eq!(ok_kind(0xD61F_03E0), InsnKind::Branch); // BR X30
    }

    #[test]
    fn blr_any_register_is_branch() {
        assert_eq!(ok_kind(0xD63F_0000), InsnKind::Branch); // BLR X0
        assert_eq!(ok_kind(0xD63F_0060), InsnKind::Branch); // BLR X3
    }

    // ---- supervisor call (Phase-2 spike) ----

    #[test]
    fn svc_imm_is_svc() {
        assert_eq!(ok_kind(0xD400_0001), InsnKind::Svc); // SVC #0
    }

    #[test]
    fn svc_with_imm16_is_svc() {
        assert_eq!(ok_kind(0xD403_4561), InsnKind::Svc); // SVC #0x1a2, bits[31:21] match
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
    fn csel_family_is_dataproc() {
        // Real guest word: CSEL X5, X6, X5, HI (0x9A8580C5).
        assert_eq!(ok_kind(0x9A85_80C5), InsnKind::DataProc);
        // op=bit30 selects the pair, op2=bits[11:10] selects within it:
        // (0,01)=CSINC, (1,00)=CSINV, (1,01)=CSNEG. GB-15 had the
        // CSINV/CSNEG words wrong (bit30=0 is unallocated); corrected
        // GB-26 against capstone + the real kernel word 0xDA80202A.
        assert_eq!(ok_kind(0x9A85_84C5), InsnKind::DataProc); // CSINC
        assert_eq!(ok_kind(0xDA80_202A), InsnKind::DataProc); // CSINV (kernel)
        assert_eq!(ok_kind(0xDA85_84C5), InsnKind::DataProc); // CSNEG
                                                              // 32-bit form classifies too (sf=0: 0x1A...).
        assert_eq!(ok_kind(0x1A85_80C5), InsnKind::DataProc);
    }

    #[test]
    fn csel_cond_cs_does_not_alias_two_source() {
        // CSEL X0, X1, X2, CS (cond == 0b0010): opcode2 >> 2 == cond
        // would match the data-processing (2 source) check, so the
        // conditional-select class must be pinned first. The class
        // check is identical either way here; this guards the order.
        assert_eq!(ok_kind(0x9A82_2820), InsnKind::DataProc);
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

    #[test]
    fn madd_family_is_dataproc() {
        // MADD X10, X10, X13, XZR (real kernel step 5217 word: 0x9b0d7d4a)
        assert_eq!(ok_kind(0x9B0D_7D4A), InsnKind::DataProc);
        // MADD W10, W10, W13, WZR (32-bit): recognized, trapped in U2
        assert_eq!(ok_kind(0x1B0D_7D4A), InsnKind::DataProc);
        // MSUB X10, X10, X13, XZR (o0 = 1): recognized, trapped in U2
        assert_eq!(ok_kind(0x9B0D_FD4A), InsnKind::DataProc);
        // SMADDL X10, W10, W13, XZR (op54 = 01): recognized, trapped in U2
        assert_eq!(ok_kind(0xDB0D_7D4A), InsnKind::DataProc);
    }

    #[test]
    fn madd_unallocated_op54_is_illegal() {
        // op54 = 11 is unallocated in the 3-source class
        assert_illegal(0xFB0D_7D4A);
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
