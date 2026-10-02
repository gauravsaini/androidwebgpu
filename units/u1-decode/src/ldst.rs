//! Load/store decoder family.

use pathn_contracts::cpu::InsnKind;

/// Loads/stores (GB-1 scope):
/// - Load/store pair: STP, LDP, LDPSW, STNP, LDNP
/// - Load/store literal: LDR (32/64-bit), LDRSW
/// - Load/store register (immediate, unsigned offset): LDR, STR, LDRSW
/// - Load/store register (immediate pre/post-indexed): LDR, STR, LDRSW
/// - Load/store register (register offset): LDR, STR, LDRSW
pub fn decode(word: u32) -> Option<InsnKind> {
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
            _ => None,                                    // 01 with store or 11: unallocated
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

    // 5. Load/store register (register offset) and atomic memory operations:
    // size 111 V 00 opc 1 Rm option S 10 Rn Rt
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 {
        let sub_op = (word >> 10) & 0x3;
        if sub_op == 0b10 {
            let size_bits = (word >> 30) & 0x3;
            let opc = (word >> 22) & 0x3;
            return match (size_bits, opc) {
                (_, 0b00 | 0b01) => Some(InsnKind::LoadStore),
                (_, 0b10) => Some(InsnKind::LoadStore), // LDRSB Xt, LDRSH Xt, LDRSW, PRFM
                (0 | 1, 0b11) => Some(InsnKind::LoadStore), // LDRSB Wt, LDRSH Wt
                _ => None,
            };
        }
        // Atomic memory operations (LSE: SWP, LDADD, STADD, etc.) and LDAPR:
        return Some(InsnKind::LoadStore);
    }

    // 6. Load/store exclusive, load-acquire / store-release, and compare-and-swap:
    // bits[29:24] == 0b001000 (V=0)
    if (word >> 24) & 0x3F == 0b001000 {
        return Some(InsnKind::LoadStore);
    }

    None
}
