//! System & exception instruction decoder family.

use pathn_contracts::cpu::InsnKind;

/// Decode system and exception instructions: SVC, HINT/NOP/barriers, MSR, MRS, SYS.
pub fn decode(word: u32) -> Option<InsnKind> {
    // SVC (immediate): 11010100 000 imm16 00001. bits[31:21] == 0b11010100000.
    // Phase-2 spike: recognized so a future SVC halt is instantly
    // identifiable in traces; U2 lifts it to an honest unimplemented trap.
    if (word >> 21) & 0x7FF == 0b11010100000 {
        return Some(InsnKind::Svc);
    }
    // System instructions: bits[31:22] == 0b1101_0101_00 (0x354).
    // Covers barriers (DMB, DSB, ISB), HINTs (NOP, WFI), MSR, MRS, SYS ops (DC, IC, TLBI).
    if (word >> 22) & 0x3FF == 0x354 {
        return Some(InsnKind::System);
    }
    None
}
