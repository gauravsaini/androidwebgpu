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
    if let Some(kind) = decode_fp_simd_load_store(word) {
        return Some(kind);
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

    if let Some(kind) = decode_asimd_three_same(word) {
        return Some(kind);
    }

    if let Some(kind) = decode_asimd_scalar_three_same(word) {
        return Some(kind);
    }

    // CMGT (vector, zero) uses the two-register miscellaneous encoding.
    if word & 0xFFFF_FC00 == 0x0E20_8800 {
        return Some(InsnKind::DataProc);
    }

    if let Some(kind) = decode_simd_crypto(word) {
        return Some(kind);
    }

    None
}

/// Decode the Advanced SIMD three-same-register class.
///
/// This class uses the size, U, Q, and opcode fields together. The table
/// records the legal field combinations for the operations implemented by
/// this decoder; combinations that are reserved for an operation stay
/// Illegal.
fn decode_asimd_three_same(word: u32) -> Option<InsnKind> {
    if word & 0x9F20_0400 != 0x0E20_0400 {
        return None;
    }

    // Index = size[1:0] | U << 2 | Q << 3. These combinations cover the
    // three-same integer and FP vector operations used in the audited image.
    const VALID_FIELDS: [u16; 32] = [
        0x7777, 0xFF77, 0x7777, 0xFFFF, 0x7777, 0xFF77, 0xFF77, 0xFF77, 0xFF77, 0xFF75, 0xFF77,
        0xFF77, 0x7777, 0x7777, 0x7777, 0x7777, 0xFF77, 0xFF77, 0x7777, 0x1717, 0x7777, 0x7777,
        0x6666, 0x0F07, 0xFF55, 0x5F55, 0xFF55, 0x3311, 0xF351, 0xF555, 0xFF55, 0x3D15,
    ];
    let opcode = ((word >> 11) & 0x1F) as usize;
    let size = (word >> 22) & 0x3;
    let u = (word >> 29) & 1;
    let q = (word >> 30) & 1;
    let field = size | (u << 2) | (q << 3);
    ((VALID_FIELDS[opcode] >> field) & 1 != 0).then_some(InsnKind::DataProc)
}

/// Decode scalar Advanced SIMD three-same-register forms verified in the
/// pinned kernel image. Scalar forms share the opcode and size fields with
/// vector forms but use a distinct encoding class.
fn decode_asimd_scalar_three_same(word: u32) -> Option<InsnKind> {
    if word & 0xDF20_0400 != 0x5E00_0400 {
        return None;
    }

    let opcode = ((word >> 11) & 0x1F) as usize;
    let size = (word >> 22) & 0x3;
    let u = (word >> 29) & 1;
    let q = (word >> 30) & 1;
    let field = size | (u << 2) | (q << 3);
    const VALID_FIELDS: [u16; 32] = [
        0x0100, 0, 0x8000, 0x0200, 0xA200, 0xA000, 0, 0x0A00, 0, 0, 0, 0, 0, 0, 0, 0, 0x6000,
        0x6000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    ((VALID_FIELDS[opcode] >> field) & 1 != 0).then_some(InsnKind::DataProc)
}

/// Decode common two-register Advanced SIMD cryptography instructions.
fn decode_simd_crypto(word: u32) -> Option<InsnKind> {
    let encoding = word & 0xFFFF_FC00;
    matches!(
        encoding,
        0x4E28_4800 // AESE
            | 0x4E28_5800 // AESD
            | 0x4E28_6800 // AESMC
            | 0x4E28_7800 // AESIMC
            | 0x5E28_2800 // SHA256SU0
    )
    .then_some(InsnKind::DataProc)
}

/// Decode scalar and vector FP/SIMD single-register memory forms.
///
/// These share the load/store encodings with integer registers, with V=1.
/// Pair transfers are handled by the load/store family decoder.
fn decode_fp_simd_load_store(word: u32) -> Option<InsnKind> {
    let encoding = (word >> 24) & 0x3F; // bits[29:24]
    let size = (word >> 30) & 0x3;
    let opc = (word >> 22) & 0x3;

    // LDR (literal), SIMD&FP: opc selects S, D, or Q; opc=3 is reserved.
    if encoding == 0b011100 {
        return (((word >> 30) & 0x3) < 3).then_some(InsnKind::LoadStore);
    }

    // Unsigned immediate offset: size selects B/H/S/D, while size=00 and
    // opc=10/11 select Q store/load respectively.
    if encoding == 0b111101 {
        return (opc <= 1 || size == 0).then_some(InsnKind::LoadStore);
    }

    if encoding != 0b111100 {
        return None;
    }

    if (word >> 21) & 1 == 0 {
        // Unscaled, pre-indexed, and post-indexed forms. The 10 index mode
        // is reserved for SIMD/FP registers.
        let index_mode = (word >> 10) & 0x3;
        if index_mode != 0b10 && (opc <= 1 || size == 0) {
            return Some(InsnKind::LoadStore);
        }
        return None;
    }

    // Register offset: options are UX[BHWX] or SX[BHWX], and Q transfers use
    // size=00 with opc=10/11 as in the unsigned-offset form.
    let sub_op = (word >> 10) & 0x3;
    let option = (word >> 13) & 0x7;
    if sub_op == 0b10 && matches!(option, 0b010 | 0b011 | 0b110 | 0b111) && (opc <= 1 || size == 0)
    {
        Some(InsnKind::LoadStore)
    } else {
        None
    }
}

/// Decode conversion between floating-point and integer:
/// SCVTF, UCVTF, FCVTZS, FCVTZU, FCVTAS, FCVTAU, FCVTPS, FCVTPU, FCVTMS, FCVTMU, FCVTNS, FCVTNU,
/// and FMOV (general <-> FP register).
fn decode_float_int_conversion(word: u32) -> Option<InsnKind> {
    // Bits[30:24] == 0b0011110. Bit[21] selects integer or fixed-point form.
    if (word >> 24) & 0x7F != 0b0011110 {
        return None;
    }

    let sf = (word >> 31) & 1;
    let type_bits = (word >> 22) & 0x3;
    // type: 00 = Single, 01 = Double, 11 = Half. 10 is reserved.
    if type_bits == 2 {
        return None;
    }

    let rmode = (word >> 19) & 0x3;
    let opcode = (word >> 16) & 0x7;
    let scale = (word >> 10) & 0x3F;
    let fixed_point = (word >> 21) & 1 == 0;

    // Integer conversions require a zero scale field. Fixed-point forms
    // support SCVTF/UCVTF and FCVTZS/FCVTZU only.
    if (!fixed_point && scale != 0)
        || (fixed_point
            && (!matches!((rmode, opcode), (0, 2 | 3) | (3, 0 | 1))
                // A 32-bit integer result has at most 32 fractional bits.
                || (sf == 0 && scale < 32)))
    {
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
    if type_bits == 2 {
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
    if type_bits == 2 {
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

    // FCCMP / FCCMPE: bits[11:10] == 0b01. Bit 4 selects FCCMP vs FCCMPE.
    if ((word >> 10) & 0x3) == 0b01 {
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
    if type_bits == 2 {
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
    if type_bits == 2 {
        return None;
    }
    if ((word >> 10) & 0x1F) != 0b10000 {
        return None;
    }

    let opcode = (word >> 15) & 0x3F;
    // This type/opcode combination is reserved for half precision.
    if type_bits == 3 && opcode == 0b000111 {
        return None;
    }
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
    if type_bits == 2 {
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
    if type_bits == 2 {
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
    fn test_decode_fp_simd_single_register_load_store() {
        assert_eq!(decode(0x5CB0_A9DC), Some(InsnKind::LoadStore)); // LDR D28, literal
        assert_eq!(decode(0x3D23_C2A6), Some(InsnKind::LoadStore)); // STR B6, [X21, #0x8f0]
        assert_eq!(decode(0x3DC0_0000), Some(InsnKind::LoadStore)); // LDR Q0, [X0]
        assert_eq!(decode(0x3D80_0000), Some(InsnKind::LoadStore)); // STR Q0, [X0]
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
        assert_eq!(decode(0x1E61_2000), Some(InsnKind::DataProc)); // FCMP D0, D1
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
    fn test_fixed_point_conversion_reserved_fields() {
        assert_eq!(decode(0x9E58_68C5), Some(InsnKind::DataProc)); // FCVTZS X5, D6, #38
        assert_eq!(decode(0x1E18_033C), None); // 32-bit conversion with scale > 32
        assert_eq!(decode(0x1EE3_C166), None); // Reserved half-precision conversion
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
    fn test_decode_asimd_crypto_and_scalar_fp16() {
        assert_eq!(decode(0x4EA0_8616), Some(InsnKind::DataProc)); // ADD V22.4S, V16.4S, V0.4S
        assert_eq!(decode(0x5E28_2A30), Some(InsnKind::DataProc)); // SHA256SU0 V16.4S, V17.4S
        assert_eq!(decode(0x1FF4_BB76), Some(InsnKind::DataProc)); // FNMSUB H22, H27, H20, H14
        assert_eq!(decode(0x7EC7_2702), Some(InsnKind::DataProc)); // FCMGT H2, H24, H7
        assert_eq!(decode(0x7E44_84A6), Some(InsnKind::DataProc)); // SQRDMLAH H6, H5, H4
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
}
