//! System & exception instruction decoder family.

use pathn_contracts::cpu::InsnKind;

/// Decode system and exception instructions:
/// - Exceptions: SVC, HVC, SMC, BRK, HLT, ERET
/// - System instructions: HINT/NOP/barriers, MSR, MRS, SYS, SYSL (DC, IC, TLBI)
pub fn decode(word: u32) -> Option<InsnKind> {
    // Exception generation instructions: bits[31:24] == 0b11010100 (0xD4).
    // opc = bits[23:21], LL = bits[1:0].
    if (word >> 24) == 0xD4 {
        let opc = (word >> 21) & 0x7;
        let ll = word & 0x3;
        match opc {
            0b000 => match ll {
                0b01 => return Some(InsnKind::Svc),
                0b10 | 0b11 => return Some(InsnKind::System), // HVC, SMC
                _ => {}
            },
            0b001 if (word & 0x1F) == 0 => return Some(InsnKind::System), // BRK
            0b010 if (word & 0x1F) == 0 => return Some(InsnKind::System), // HLT
            _ => {}
        }
    }
    // ERET: 1101 0110 100 11111 0000 00 11111 00000 (0xD69F03E0)
    if word == 0xD69F_03E0 {
        return Some(InsnKind::System);
    }
    // System instructions: bits[31:22] == 0b1101_0101_00 (0x354).
    // Covers barriers (DMB, DSB, ISB), HINTs (NOP, WFI), MSR, MRS, SYS ops (DC, IC, TLBI).
    if (word >> 22) & 0x3FF == 0x354 {
        return Some(InsnKind::System);
    }
    None
}
