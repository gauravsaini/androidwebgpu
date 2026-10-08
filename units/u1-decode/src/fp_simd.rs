//! Floating-point and SIMD instruction decoder family.
//!
//! Follows LLD Box Pure Function & Explicit Contract Rule:
//! Pure, deterministic mapping from 32-bit word -> Option<InsnKind>.
//! No hidden internal state, no mutable shared state, no covert side effects.

use pathn_contracts::cpu::InsnKind;

/// Decode Floating-point and SIMD instructions.
///
/// Dispatched from `lib.rs` when bits[28:25] in {0b0111, 0b1111, 0b0110, 0b1110}.
pub fn decode(word: u32) -> Option<InsnKind> {
    if decode_q_pair(word) {
        return Some(InsnKind::LoadStore);
    }
    if decode_q_single(word) {
        return Some(InsnKind::LoadStore);
    }

    // Three-same arithmetic, DUP, and EXT from the characterized NEON subset.
    if decode_neon_three_same(word)
        || decode_dup(word)
        || decode_ext(word)
        || decode_vector_mov(word)
        || decode_movi_16b(word)
    {
        return Some(InsnKind::DataProc);
    }

    // 1. Conversion between floating-point and integer (ARM ARM C7.2.14)
    // sf 00 11110 type 1 rmode opcode Rn Rd
    if let Some(kind) = decode_float_int_conversion(word) {
        return Some(kind);
    }

    // 2. Floating-point immediate (ARM ARM C7.2.13)
    // 000 11110 type 1 imm8 100 00 Rd
    if let Some(kind) = decode_fp_imm(word) {
        return Some(kind);
    }

    // 3. Floating-point compare & conditional compare (ARM ARM C7.2.10)
    // FCMP:  000 11110 type 1 Rm 00100 Rn 0 op 00
    // FCCMP: 000 11110 type 1 Rm cond  01 Rn 0 op nzcv
    if let Some(kind) = decode_fp_compare(word) {
        return Some(kind);
    }

    // 4. Floating-point conditional select (ARM ARM C7.2.9)
    // 000 11110 type 1 Rm cond 11 Rn Rd
    if let Some(kind) = decode_fp_cond_select(word) {
        return Some(kind);
    }

    // 5. Floating-point data-processing (1 source) (ARM ARM C7.2.12)
    // 000 11110 type 1 0000 opcode Rn Rd
    if let Some(kind) = decode_fp_data_proc_1source(word) {
        return Some(kind);
    }

    // 6. Floating-point data-processing (2 sources) (ARM ARM C7.2.11)
    // 000 11110 type 1 Rm opcode 10 Rn Rd
    if let Some(kind) = decode_fp_data_proc_2source(word) {
        return Some(kind);
    }

    // 7. Floating-point data-processing (3 sources) (ARM ARM C7.2.8)
    // 000 11111 type 0 o1 Rm o0 Ra Rn Rd
    if let Some(kind) = decode_fp_data_proc_3source(word) {
        return Some(kind);
    }

    None
}

fn decode_q_pair(word: u32) -> bool {
    // V=1 pair load/store. Q registers require opc=10; non-temporal mode 00
    // is intentionally outside the characterized LDP/STP subset.
    ((word >> 25) & 0x1f) == 0b10110 && ((word >> 30) & 0x3) == 0b10 && ((word >> 23) & 0x3) != 0
}

fn decode_q_single(word: u32) -> bool {
    let opc = (word >> 22) & 0x3;
    let size = (word >> 30) & 0x3;
    let simd_imm = (word >> 24) & 0x3f;
    if size != 0 || !matches!(opc, 0b10 | 0b11) || ((word >> 21) & 1) != 0 {
        return false;
    }

    // Q LDR/STR unsigned offset (scaled by 16 bytes).
    if simd_imm == 0b111101 {
        return true;
    }

    // Q LDR/STR unscaled, pre-indexed, and post-indexed forms.
    simd_imm == 0b111100 && matches!((word >> 10) & 0x3, 0 | 1 | 3)
}

fn decode_neon_three_same(word: u32) -> bool {
    if ((word >> 24) & 0x1f) != 0b01110 || ((word >> 21) & 1) != 1 {
        return false;
    }
    let q = (word >> 30) & 1;
    let size = (word >> 22) & 0x3;
    let op = (word >> 11) & 0x1f;
    match op {
        0b10000 => q == 1 || size != 0b11, // ADD/SUB: 8B..2D
        0b10011 => size != 0b11,           // MUL: no 2D arrangement
        _ => false,
    }
}

fn decode_dup(word: u32) -> bool {
    if ((word >> 24) & 0x1f) != 0b01110 || ((word >> 21) & 1) != 0 || (word >> 29) & 1 != 0 {
        return false;
    }
    let imm5 = (word >> 16) & 0x1f;
    match (word >> 10) & 0x3f {
        0b000011 => matches!(imm5, 1 | 2 | 4 | 8), // DUP from W/X register
        0b000001 => {
            if imm5 == 0 {
                return false;
            }
            let element_log2 = imm5.trailing_zeros();
            if element_log2 > 3 {
                return false;
            }
            let lane = imm5 >> (element_log2 + 1);
            lane < (16 >> element_log2)
        }
        _ => false,
    }
}

fn decode_ext(word: u32) -> bool {
    ((word >> 24) & 0x1f) == 0b01110
        && ((word >> 29) & 1) == 1
        && ((word >> 21) & 1) == 0
        && ((word >> 10) & 0x3) == 0b10
        && (((word >> 30) & 1) == 1 || ((word >> 11) & 0xf) <= 7)
}

fn decode_vector_mov(word: u32) -> bool {
    // MOV Vd.16B, Vn.16B is ORR Vd.16B, Vn.16B, Vn.16B.
    word & !((0x1f << 16) | (0x1f << 5) | 0x1f) == 0x4ea0_1c00
        && ((word >> 16) & 0x1f) == ((word >> 5) & 0x1f)
}

fn decode_movi_16b(word: u32) -> bool {
    // MOVI Vd.16B, #imm8 (the immediate form used to seed T112's illegal-MUL control).
    word & !((0x7 << 16) | (0x1f << 5) | 0x1f) == 0x4f00_e400
}

/// Decode conversion between floating-point and integer:
/// SCVTF, UCVTF, FCVTZS, FCVTZU, FCVTAS, FCVTAU, FCVTPS, FCVTPU, FCVTMS, FCVTMU, FCVTNS, FCVTNU,
/// and FMOV (general <-> FP register).
fn decode_float_int_conversion(word: u32) -> Option<InsnKind> {
    // Bits[30:24] == 0b0011110, bit[21] == 1
    if (word >> 24) & 0x7F != 0b0011110 || ((word >> 21) & 1) != 1 {
        return None;
    }

    let sf = (word >> 31) & 1;
    let type_bits = (word >> 22) & 0x3;
    // type: 00 = Single, 01 = Double. 10 / 11 are reserved.
    if type_bits > 1 {
        return None;
    }

    let rmode = (word >> 19) & 0x3;
    let opcode = (word >> 16) & 0x7;
    let scale = (word >> 10) & 0x3F;

    // Integer conversion: scale == 0 (fixed-point scale is not yet modeled).
    if scale != 0 {
        return None;
    }

    match rmode {
        // rmode == 0b00:
        // 000: FCVTNS, 001: FCVTNU, 010: SCVTF, 011: UCVTF, 100: FCVTAS, 101: FCVTAU,
        // 110: FMOV (FP to general), 111: FMOV (general to FP)
        0b00 => match opcode {
            0b000 | 0b001 | 0b010 | 0b011 | 0b100 | 0b101 => Some(InsnKind::DataProc),
            // FMOV int <-> FP:
            // 32-bit (sf=0) requires Single (type=0); 64-bit (sf=1) requires Double (type=1).
            0b110 | 0b111 => {
                if sf == type_bits {
                    Some(InsnKind::DataProc)
                } else {
                    None // sf != type is unallocated / top-half move
                }
            }
            _ => None,
        },
        // rmode == 0b01: FCVTPS (000), FCVTPU (001)
        0b01 => match opcode {
            0b000 | 0b001 => Some(InsnKind::DataProc),
            _ => None,
        },
        // rmode == 0b10: FCVTMS (000), FCVTMU (001)
        0b10 => match opcode {
            0b000 | 0b001 => Some(InsnKind::DataProc),
            _ => None,
        },
        // rmode == 0b11: FCVTZS (000), FCVTZU (001) - round toward zero
        0b11 => match opcode {
            0b000 | 0b001 => Some(InsnKind::DataProc),
            _ => None,
        },
        _ => None,
    }
}

/// Decode floating-point immediate: FMOV (immediate).
fn decode_fp_imm(word: u32) -> Option<InsnKind> {
    // Bits[31:24] == 0x1E, bit[21] == 1, bits[12:10] == 0b100, bits[9:5] == 0b00000
    if (word >> 24) != 0x1E || ((word >> 21) & 1) != 1 {
        return None;
    }
    let type_bits = (word >> 22) & 0x3;
    if type_bits > 1 {
        return None;
    }
    if ((word >> 10) & 0x7) != 0b100 {
        return None;
    }
    if ((word >> 5) & 0x1F) != 0 {
        return None;
    }

    Some(InsnKind::DataProc)
}

/// Decode floating-point compare (FCMP/FCMPE) and conditional compare (FCCMP/FCCMPE).
fn decode_fp_compare(word: u32) -> Option<InsnKind> {
    // Bits[31:24] == 0x1E, bit[21] == 1
    if (word >> 24) != 0x1E || ((word >> 21) & 1) != 1 {
        return None;
    }
    let type_bits = (word >> 22) & 0x3;
    if type_bits > 1 {
        return None;
    }

    // FCMP / FCMPE: bits[15:10] == 0b001000, bits[2:0] == 0b000
    if ((word >> 10) & 0x3F) == 0b001000 && (word & 0x7) == 0 {
        let is_zero = ((word >> 3) & 1) == 1;
        let rm = (word >> 16) & 0x1F;
        // If compare against zero, Rm must be 0b00000
        if is_zero && rm != 0 {
            return None;
        }
        return Some(InsnKind::DataProc);
    }

    // FCCMP / FCCMPE: bits[11:10] == 0b01, bit[4] == 0
    if ((word >> 10) & 0x3) == 0b01 && ((word >> 4) & 1) == 0 {
        return Some(InsnKind::DataProc);
    }

    None
}

/// Decode floating-point conditional select: FCSEL.
fn decode_fp_cond_select(word: u32) -> Option<InsnKind> {
    // Bits[31:24] == 0x1E, bit[21] == 1, bits[11:10] == 0b11
    if (word >> 24) != 0x1E || ((word >> 21) & 1) != 1 {
        return None;
    }
    let type_bits = (word >> 22) & 0x3;
    if type_bits > 1 {
        return None;
    }
    if ((word >> 10) & 0x3) != 0b11 {
        return None;
    }

    Some(InsnKind::DataProc)
}

/// Decode floating-point data-processing (1 source):
/// FMOV, FABS, FNEG, FSQRT, FCVT, FRINTA, FRINTN, FRINTP, FRINTM, FRINTZ, FRINTI, FRINTX.
fn decode_fp_data_proc_1source(word: u32) -> Option<InsnKind> {
    // Bits[31:24] == 0x1E, bit[21] == 1, bits[14:10] == 0b10000
    if (word >> 24) != 0x1E || ((word >> 21) & 1) != 1 {
        return None;
    }
    let type_bits = (word >> 22) & 0x3;
    if type_bits > 1 {
        return None;
    }
    if ((word >> 10) & 0x1F) != 0b10000 {
        return None;
    }

    let opcode = (word >> 15) & 0x3F;
    match opcode {
        0b000000 => Some(InsnKind::DataProc), // FMOV (register)
        0b000001 => Some(InsnKind::DataProc), // FABS
        0b000010 => Some(InsnKind::DataProc), // FNEG
        0b000011 => Some(InsnKind::DataProc), // FSQRT
        0b000100 => Some(InsnKind::DataProc), // FCVT (single <-> double)
        0b000101 | 0b000111 => Some(InsnKind::DataProc), // FCVT (half precision)
        0b001000 => Some(InsnKind::DataProc), // FRINTN
        0b001001 => Some(InsnKind::DataProc), // FRINTP
        0b001010 => Some(InsnKind::DataProc), // FRINTM
        0b001011 => Some(InsnKind::DataProc), // FRINTZ
        0b001100 => Some(InsnKind::DataProc), // FRINTA
        0b001110 => Some(InsnKind::DataProc), // FRINTX
        0b001111 => Some(InsnKind::DataProc), // FRINTI
        _ => None,
    }
}

/// Decode floating-point data-processing (2 sources):
/// FADD, FSUB, FMUL, FDIV, FMAX, FMIN, FMAXNM, FMINNM, FNMUL.
fn decode_fp_data_proc_2source(word: u32) -> Option<InsnKind> {
    // Bits[31:24] == 0x1E, bit[21] == 1, bits[11:10] == 0b10
    if (word >> 24) != 0x1E || ((word >> 21) & 1) != 1 {
        return None;
    }
    let type_bits = (word >> 22) & 0x3;
    if type_bits > 1 {
        return None;
    }
    if ((word >> 10) & 0x3) != 0b10 {
        return None;
    }

    let opcode = (word >> 12) & 0xF;
    match opcode {
        0b0000 => Some(InsnKind::DataProc), // FMUL
        0b0001 => Some(InsnKind::DataProc), // FDIV
        0b0010 => Some(InsnKind::DataProc), // FADD
        0b0011 => Some(InsnKind::DataProc), // FSUB
        0b0100 => Some(InsnKind::DataProc), // FMAX
        0b0101 => Some(InsnKind::DataProc), // FMIN
        0b0110 => Some(InsnKind::DataProc), // FMAXNM
        0b0111 => Some(InsnKind::DataProc), // FMINNM
        0b1000 => Some(InsnKind::DataProc), // FNMUL
        _ => None,
    }
}

/// Decode floating-point data-processing (3 sources):
/// FMADD, FMSUB, FNMADD, FNMSUB.
fn decode_fp_data_proc_3source(word: u32) -> Option<InsnKind> {
    // Bits[31:24] == 0x1F
    if (word >> 24) != 0x1F {
        return None;
    }
    let type_bits = (word >> 22) & 0x3;
    if type_bits > 1 {
        return None;
    }

    Some(InsnKind::DataProc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_fmov_imm() {
        assert_eq!(decode(0x1E2E_1000), Some(InsnKind::DataProc)); // FMOV S0, #1.0
        assert_eq!(decode(0x1E6E_1000), Some(InsnKind::DataProc)); // FMOV D0, #1.0
    }

    #[test]
    fn test_decode_fmov_reg() {
        assert_eq!(decode(0x1E20_4020), Some(InsnKind::DataProc)); // FMOV S0, S1
        assert_eq!(decode(0x1E60_4020), Some(InsnKind::DataProc)); // FMOV D0, D1
        assert_eq!(decode(0x1E26_0020), Some(InsnKind::DataProc)); // FMOV W0, S1
        assert_eq!(decode(0x9E66_0020), Some(InsnKind::DataProc)); // FMOV X0, D1
        assert_eq!(decode(0x1E27_0020), Some(InsnKind::DataProc)); // FMOV S0, W1
        assert_eq!(decode(0x9E67_0020), Some(InsnKind::DataProc)); // FMOV D0, X1
    }

    #[test]
    fn test_decode_fadd_fsub() {
        assert_eq!(decode(0x1E22_2820), Some(InsnKind::DataProc)); // FADD S0, S1, S2
        assert_eq!(decode(0x1E62_2820), Some(InsnKind::DataProc)); // FADD D0, D1, D2
        assert_eq!(decode(0x1E22_3820), Some(InsnKind::DataProc)); // FSUB S0, S1, S2
        assert_eq!(decode(0x1E62_3820), Some(InsnKind::DataProc)); // FSUB D0, D1, D2
    }

    #[test]
    fn test_decode_fmul_fdiv() {
        assert_eq!(decode(0x1E22_0820), Some(InsnKind::DataProc)); // FMUL S0, S1, S2
        assert_eq!(decode(0x1E62_0820), Some(InsnKind::DataProc)); // FMUL D0, D1, D2
        assert_eq!(decode(0x1E22_1820), Some(InsnKind::DataProc)); // FDIV S0, S1, S2
        assert_eq!(decode(0x1E62_1820), Some(InsnKind::DataProc)); // FDIV D0, D1, D2
    }

    #[test]
    fn test_decode_fcmp_fccmp() {
        assert_eq!(decode(0x1E21_2000), Some(InsnKind::DataProc)); // FCMP S0, S1
        assert_eq!(decode(0x1E21_2010), Some(InsnKind::DataProc)); // FCMPE S0, S1
        assert_eq!(decode(0x1E61_2000), Some(InsnKind::DataProc)); // FCMP D0, D1
        assert_eq!(decode(0x1E61_2010), Some(InsnKind::DataProc)); // FCMPE D0, D1
        assert_eq!(decode(0x1E20_2008), Some(InsnKind::DataProc)); // FCMP S0, #0.0
        assert_eq!(decode(0x1E60_2008), Some(InsnKind::DataProc)); // FCMP D0, #0.0
        assert_eq!(decode(0x1E21_0400), Some(InsnKind::DataProc)); // FCCMP S0, S1, #0, EQ
        assert_eq!(decode(0x1E61_140F), Some(InsnKind::DataProc)); // FCCMP D0, D1, #15, NE
    }

    #[test]
    fn test_decode_scvtf_ucvtf() {
        assert_eq!(decode(0x1E22_0020), Some(InsnKind::DataProc)); // SCVTF S0, W1
        assert_eq!(decode(0x9E62_0020), Some(InsnKind::DataProc)); // SCVTF D0, X1
        assert_eq!(decode(0x9E22_0020), Some(InsnKind::DataProc)); // SCVTF S0, X1
        assert_eq!(decode(0x1E62_0020), Some(InsnKind::DataProc)); // SCVTF D0, W1
        assert_eq!(decode(0x1E23_0020), Some(InsnKind::DataProc)); // UCVTF S0, W1
        assert_eq!(decode(0x9E63_0020), Some(InsnKind::DataProc)); // UCVTF D0, X1
    }

    #[test]
    fn test_decode_fcvtzs_fcvtzu() {
        assert_eq!(decode(0x1E38_0020), Some(InsnKind::DataProc)); // FCVTZS W0, S1
        assert_eq!(decode(0x9E78_0020), Some(InsnKind::DataProc)); // FCVTZS X0, D1
        assert_eq!(decode(0x1E78_0020), Some(InsnKind::DataProc)); // FCVTZS W0, D1
        assert_eq!(decode(0x9E38_0020), Some(InsnKind::DataProc)); // FCVTZS X0, S1
        assert_eq!(decode(0x1E39_0020), Some(InsnKind::DataProc)); // FCVTZU W0, S1
        assert_eq!(decode(0x9E79_0020), Some(InsnKind::DataProc)); // FCVTZU X0, D1
    }

    #[test]
    fn test_decode_frinta() {
        assert_eq!(decode(0x1E26_4020), Some(InsnKind::DataProc)); // FRINTA S0, S1
        assert_eq!(decode(0x1E66_4020), Some(InsnKind::DataProc)); // FRINTA D0, D1
    }

    #[test]
    fn test_decode_fcsel() {
        assert_eq!(decode(0x1E22_0C20), Some(InsnKind::DataProc)); // FCSEL S0, S1, S2, EQ
        assert_eq!(decode(0x1E62_1C20), Some(InsnKind::DataProc)); // FCSEL D0, D1, D2, NE
    }

    #[test]
    fn test_decode_fma() {
        assert_eq!(decode(0x1F02_0C20), Some(InsnKind::DataProc)); // FMADD S0, S1, S2, S3
        assert_eq!(decode(0x1F42_0C20), Some(InsnKind::DataProc)); // FMADD D0, D1, D2, D3
        assert_eq!(decode(0x1F02_8C20), Some(InsnKind::DataProc)); // FMSUB S0, S1, S2, S3
        assert_eq!(decode(0x1F42_8C20), Some(InsnKind::DataProc)); // FMSUB D0, D1, D2, D3
        assert_eq!(decode(0x1F22_0C20), Some(InsnKind::DataProc)); // FNMADD S0, S1, S2, S3
        assert_eq!(decode(0x1F62_0C20), Some(InsnKind::DataProc)); // FNMADD D0, D1, D2, D3
        assert_eq!(decode(0x1F22_8C20), Some(InsnKind::DataProc)); // FNMSUB S0, S1, S2, S3
        assert_eq!(decode(0x1F62_8C20), Some(InsnKind::DataProc)); // FNMSUB D0, D1, D2, D3
    }

    #[test]
    fn test_qemu_verified_vectors() {
        // QEMU-differential verified exact instruction words:
        // SCVTF S0, W1 (W1=42 => S0=0x42280000)
        assert_eq!(decode(0x1E22_0020), Some(InsnKind::DataProc));
        // SCVTF D0, X1 (X1=-100 => D0=0xC059000000000000)
        assert_eq!(decode(0x9E62_0020), Some(InsnKind::DataProc));
        // UCVTF S0, W1 (W1=0xB2D05E00 => S0=0x4F32D05E)
        assert_eq!(decode(0x1E23_0020), Some(InsnKind::DataProc));
        // UCVTF D0, X1 (X1=1<<63 => D0=0x43E0000000000000)
        assert_eq!(decode(0x9E63_0020), Some(InsnKind::DataProc));
        // FCVTZS X2, D1 (D1=42.75 => X2=42)
        assert_eq!(decode(0x9E78_0022), Some(InsnKind::DataProc));
        // FCVTZS W2, S1 (S1=-5.9 => W2=-5 = 0xFFFFFFFB)
        assert_eq!(decode(0x1E38_0022), Some(InsnKind::DataProc));
        // FCVTZU X2, D1 (D1=100.9 => X2=100)
        assert_eq!(decode(0x9E79_0022), Some(InsnKind::DataProc));
        // FMOV S0, #1.0 (S0=0x3F800000)
        assert_eq!(decode(0x1E2E_1000), Some(InsnKind::DataProc));
        // FMOV D0, #1.0 (D0=0x3FF0000000000000)
        assert_eq!(decode(0x1E6E_1000), Some(InsnKind::DataProc));
        // FMOV D0, #31.0 (D0=0x403F000000000000)
        assert_eq!(decode(0x1E6F_1000), Some(InsnKind::DataProc));
        // FADD D0, D1, D2 (1.5 + 2.5 = 4.0 = 0x4010000000000000)
        assert_eq!(decode(0x1E62_2820), Some(InsnKind::DataProc));
        // FSUB S0, S1, S2 (5.0 - 2.0 = 3.0 = 0x40400000)
        assert_eq!(decode(0x1E22_3820), Some(InsnKind::DataProc));
        // FMUL D0, D1, D2 (3.0 * 7.0 = 21.0 = 0x4035000000000000)
        assert_eq!(decode(0x1E62_0820), Some(InsnKind::DataProc));
        // FDIV S0, S1, S2 (10.0 / 4.0 = 2.5 = 0x40200000)
        assert_eq!(decode(0x1E22_1820), Some(InsnKind::DataProc));
        // FCMP D0, D1 (1.0 vs 2.0 => NZCV=0x8)
        assert_eq!(decode(0x1E61_2000), Some(InsnKind::DataProc));
        // FCMP D0, #0.0 (3.0 vs 0.0 => NZCV=0x2)
        assert_eq!(decode(0x1E60_2008), Some(InsnKind::DataProc));
        // FRINTA D0, D1 (2.5 => 3.0 = 0x4008000000000000)
        assert_eq!(decode(0x1E66_4020), Some(InsnKind::DataProc));
    }

    #[test]
    fn test_t112_q_pair_and_neon_encodings() {
        for word in [
            0xAD40_0520,
            0xACC1_0520,
            0xADC0_8520,
            0xAD40_07E0,
            0xACC1_07E0,
            0xAD00_0520,
            0xAD80_8520,
            0xAD01_0520,
            0xAC81_0520,
        ] {
            assert_eq!(decode(word), Some(InsnKind::LoadStore), "{word:#010x}");
        }

        for word in [
            0x3DC0_0120, // LDR Q0, [X9]
            0x3DC0_0521, // LDR Q1, [X9, #16]
            0x3D80_0A60, // STR Q0, [X19, #32]
            0x3C81_0662, // STR Q2, [X19], #16
        ] {
            assert_eq!(decode(word), Some(InsnKind::LoadStore), "{word:#010x}");
        }

        for word in [
            0x0E21_8402,
            0x4E21_8402,
            0x0E61_8402,
            0x4E61_8402,
            0x0EA1_8402,
            0x4EA1_8402,
            0x4EE1_8402,
            0x2E21_8402,
            0x6E21_8402,
            0x2E61_8402,
            0x6E61_8402,
            0x2EA1_8402,
            0x6EA1_8402,
            0x6EE1_8402,
            0x0E21_9C02,
            0x4E21_9C02,
            0x0E61_9C02,
            0x4E61_9C02,
            0x0EA1_9C02,
            0x4EA1_9C02,
            0x4E01_0C22,
            0x4E0E_0402,
            0x4E18_0402,
            0x4E08_0CA2,
            0x4EA0_1C08,
            0x4F00_E440,
            0x4F00_E461,
            0x6E01_3802,
            0x6E01_7802,
        ] {
            assert_eq!(decode(word), Some(InsnKind::DataProc), "{word:#010x}");
        }
        assert_eq!(decode(0x4EE1_9C02), None); // MUL Vd.2D is unallocated
    }
}
