//! Branch decoder family.

use pathn_contracts::cpu::InsnKind;

pub fn decode(word: u32) -> Option<InsnKind> {
    // B / BL: 000101 / 100101 imm26.
    match (word >> 26) & 0x3F {
        0b000101 | 0b100101 => return Some(InsnKind::Branch),
        _ => {}
    }
    // B.cond: 0101010 0 imm19 0 cond.
    // bits[31:24] == 0x54, bit 4 == 0, cond is bits[3:0] (0..15, all legal; AL/NV always true).
    if (word >> 24) == 0x54 && (word & 0x10) == 0 {
        return Some(InsnKind::Branch);
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
    // the Rn field, so any RET <Xn> matches.
    if word & 0xFFFF_FC1F == 0xD65F_0000 {
        return Some(InsnKind::Branch);
    }
    // BR: 1101011 0 000 11111 000000 Rn 00000.
    // BLR: 1101011 0 001 11111 000000 Rn 00000.
    // Masks clear only the Rn field, so any register matches.
    if word & 0xFFFF_FC1F == 0xD61F_0000 || word & 0xFFFF_FC1F == 0xD63F_0000 {
        return Some(InsnKind::Branch);
    }
    None
}
