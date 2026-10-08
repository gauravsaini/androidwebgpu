//! Load/store decoder family.

use pathn_contracts::cpu::InsnKind;

/// Loads/stores (GB-1 scope):
/// - Load/store pair: STP, LDP, LDPSW, STNP, LDNP
/// - Load/store literal: LDR (32/64-bit), LDRSW
/// - Load/store register (immediate, unsigned offset): LDR, STR, LDRSW
/// - Load/store register (immediate pre/post-indexed): LDR, STR, LDRSW
/// - Load/store register (register offset): LDR, STR, LDRSW
pub fn decode(word: u32) -> Option<InsnKind> {
    if decode_asimd_structure(word) {
        return Some(InsnKind::LoadStore);
    }

    // 1. Load/store pair (STP, LDP, LDPSW, STNP, LDNP):
    // opc 101 V 0 index L imm7 Rt2 Rn Rt. V selects integer (0) or
    // SIMD&FP (1) registers.
    let pair_encoding = (word >> 25) & 0x1F;
    if pair_encoding == 0b10100 || pair_encoding == 0b10110 {
        let opc = (word >> 30) & 0x3;
        let mode = (word >> 23) & 0x3;
        let is_load = (word >> 22) & 1 == 1;
        if pair_encoding == 0b10110 {
            return (opc <= 2).then_some(InsnKind::LoadStore); // S/D/Q pairs
        }
        return match opc {
            0b00 => Some(InsnKind::LoadStore), // 32-bit STP / LDP
            0b01 if is_load && mode != 0 => Some(InsnKind::LoadStore), // LDPSW
            0b10 => Some(InsnKind::LoadStore), // 64-bit STP / LDP
            _ => None, // LDPSW store/non-temporal and opc=11 are unallocated
        };
    }

    // 2. Load/store literal:
    // opc 011 V 00 imm19 Rt with bits[29:24] == 0b011000 (V=0).
    if (word >> 24) & 0x3F == 0b011000 {
        let opc = (word >> 30) & 0x3;
        return match opc {
            0b00 | 0b01 | 0b10 | 0b11 => Some(InsnKind::LoadStore), // 32-bit LDR, 64-bit LDR, LDRSW, PRFM
            _ => None,
        };
    }

    // Load-acquire/store-release unscaled immediate (LDAPUR/STLUR).
    // Unlike LDTR/STTR, these use bits[29:24] == 011001.
    if (word >> 24) & 0x3F == 0b011001 {
        if ((word >> 21) & 1) != 0 || (word >> 10) & 0x3 != 0 {
            return None;
        }
        let size = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        return match (size, opc) {
            (_, 0 | 1) => Some(InsnKind::LoadStore), // STLUR / LDAPUR
            (0..=2, 2) => Some(InsnKind::LoadStore), // LDAPURSB/H/W (64-bit result)
            (0..=1, 3) => Some(InsnKind::LoadStore), // LDAPURSB/H (32-bit result)
            _ => None,
        };
    }

    // 3. Load/store register (immediate, unsigned offset):
    // size 111 V 01 opc imm12 Rn Rt with bits[29:24] == 0b111001 (V=0).
    if (word >> 24) & 0x3F == 0b111001 {
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        return match (size_bits, opc) {
            (_, 0b00 | 0b01) => Some(InsnKind::LoadStore), // STR, LDR (B, H, W, X)
            (_, 0b10) => Some(InsnKind::LoadStore),        // LDRSB Xt, LDRSH Xt, LDRSW, PRFM
            (0 | 1, 0b11) => Some(InsnKind::LoadStore),    // LDRSB Wt, LDRSH Wt
            _ => None,
        };
    }

    // 4. Load/store register (immediate pre/post-indexed, and unscaled):
    // size 111 V 00 opc 0 imm9 type Rn Rt
    // with bits[29:24] == 0b111000, bit 21 == 0, and type in {0b01 (post),
    // 0b11 (pre), 0b00 (unscaled LDUR/STUR)}.
    // (GB-26: type 0b00 was kept out of scope until the kernel hit STUR
    // at step 1248988.)
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 0 {
        let idx_type = (word >> 10) & 0x3;
        // 00 = unscaled (LDUR/STUR/PRFUM), 01 = post-index, 10 = unprivileged (LDTR/STTR), 11 = pre-index
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        return match (size_bits, opc) {
            (_, 0b00 | 0b01) => Some(InsnKind::LoadStore),
            (3, 0b10) if idx_type == 0b00 => Some(InsnKind::LoadStore), // PRFUM (unscaled only)
            (0..=2, 0b10) => Some(InsnKind::LoadStore),                 // LDRSB/H/W (64-bit)
            (0..=1, 0b11) => Some(InsnKind::LoadStore),                 // LDRSB/H (32-bit)
            _ => None,
        };
    }

    // 5. Load/store register (register offset), plus LDRAA/LDRAB:
    // size 111 V 00 opc 1 Rm option S 10 Rn Rt
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 {
        let sub_op = (word >> 10) & 0x3;
        let size_bits = (word >> 30) & 0x3;
        let atomic_op = (word >> 12) & 0xF;
        if sub_op == 0 {
            if atomic_op <= 0b1000 {
                return Some(InsnKind::LoadStore); // LSE atomics: LDADD..LDUMIN and SWP
            }
            if atomic_op == 0b1100 && (word >> 21) & 0x7 == 0b101 && (word >> 16) & 0x1F == 0x1F {
                return Some(InsnKind::LoadStore); // LDAPR(B/H/W/X)
            }
            return None;
        }
        if sub_op == 0b10 {
            let opc = (word >> 22) & 0x3;
            let option = (word >> 13) & 0x7;
            if !matches!(option, 0b010 | 0b011 | 0b110 | 0b111) {
                return None;
            }
            return match (size_bits, opc) {
                (_, 0b00 | 0b01) => Some(InsnKind::LoadStore),
                (_, 0b10) => Some(InsnKind::LoadStore), // LDRSB Xt, LDRSH Xt, LDRSW, PRFM
                (0 | 1, 0b11) => Some(InsnKind::LoadStore), // LDRSB Wt, LDRSH Wt
                _ => None,
            };
        }
        // LDRAA/LDRA(B) use this group with size=11 and pre/post-index
        // sub-ops. Other non-register-offset sub-ops are unallocated here.
        if size_bits == 0b11 && matches!(sub_op, 0b01 | 0b11) {
            return Some(InsnKind::LoadStore);
        }
        return None;
    }

    // 6. Load/store exclusive, load-acquire / store-release, and compare-and-swap:
    // bits[29:24] == 0b001000 (V=0)
    if (word >> 24) & 0x3F == 0b001000 {
        let size = (word >> 30) & 0x3;
        let op = (word >> 21) & 0x7;
        let rs = (word >> 16) & 0x1F;
        let rt2 = (word >> 10) & 0x1F;
        let valid = match op {
            0 => true,                          // STXR / STLXR (single-register store)
            1 => size >= 0b10,                  // STXP / STLXP
            2 => rs == 0x1F && rt2 == 0x1F,     // LDXR / LDAXR
            3 => size >= 0b10,                  // LDXP / LDAXP
            4 | 6 => rs == 0x1F && rt2 == 0x1F, // STLR / LDAR family
            5 | 7 => rt2 == 0x1F,               // CAS family
            _ => false,
        };
        return if valid {
            Some(InsnKind::LoadStore)
        } else {
            None
        };
    }

    None
}

/// Recognize Advanced SIMD structure transfers using their fixed encoding
/// fields. The masks leave register, element-size, and vector-length fields
/// variable while rejecting unused structure opcodes.
fn decode_asimd_structure(word: u32) -> bool {
    const NO_OFFSET: [u32; 14] = [
        0x0C40_7000,
        0x0C40_A000,
        0x0C40_6000,
        0x0C40_2000, // LD1
        0x0C40_8000,
        0x0C40_4000,
        0x0C40_0000, // LD2..LD4
        0x0C00_7000,
        0x0C00_A000,
        0x0C00_6000,
        0x0C00_2000, // ST1
        0x0C00_8000,
        0x0C00_4000,
        0x0C00_0000, // ST2..ST4
    ];
    let no_offset = NO_OFFSET.iter().any(|&value| word & 0xBFFF_F000 == value);

    const POST_INDEX: [u32; 14] = [
        0x0CC0_7000,
        0x0CC0_A000,
        0x0CC0_6000,
        0x0CC0_2000, // LD1
        0x0CC0_8000,
        0x0CC0_4000,
        0x0CC0_0000, // LD2..LD4
        0x0C80_7000,
        0x0C80_A000,
        0x0C80_6000,
        0x0C80_2000, // ST1
        0x0C80_8000,
        0x0C80_4000,
        0x0C80_0000, // ST2..ST4
    ];
    let post_index = POST_INDEX.iter().any(|&value| word & 0xBFE0_F000 == value);

    if no_offset || post_index {
        let structure_count = (word >> 12) & 0xF;
        let q = (word >> 30) & 1;
        let size = (word >> 10) & 0x3;
        // LD/ST2, LD/ST3, and LD/ST4 have no 1D arrangement. The one
        // register LD/ST1 forms do support it.
        return !matches!(structure_count, 0 | 4 | 8) || q == 1 || size != 3;
    }

    // LD4R, including no-offset, immediate post-index, and register
    // post-index forms.
    (word & 0xBFFF_F000 == 0x0D60_E000)
        || (word & 0xBFFF_F000 == 0x0DFF_E000)
        || (word & 0xBFE0_F000 == 0x0DE0_E000)
}
