//! Data-processing (immediate) decoder family.

use pathn_contracts::cpu::{decode_bitmasks, InsnKind};

pub fn decode(word: u32) -> Option<InsnKind> {
    match (word >> 22) & 0x7F {
        // Add/subtract (immediate): sf op S 10001 sh imm12 Rn Rd, sh ∈ {00,01}.
        0b1000100 | 0b1000101 => {
            Some(InsnKind::DataProc) // ADD / SUB / ADDS / SUBS (including CMP / CMN imm)
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
        // Logical (immediate): sf opc 100100 N immr imms Rn Rd.
        // Validated via decode_bitmasks (GB-4): unallocated encodings are Illegal.
        0b1001000 | 0b1001001 => {
            let sf = (word >> 31) & 1 == 1;
            let n = ((word >> 22) & 1) as u8;
            let immr = ((word >> 16) & 0x3F) as u8;
            let imms = ((word >> 10) & 0x3F) as u8;
            if decode_bitmasks(n, imms, immr, sf).is_some() {
                Some(InsnKind::DataProc) // AND / ORR / EOR / ANDS (including TST imm)
            } else {
                None
            }
        }
        // Bitfield, extract: out of scope.
        _ => None,
    }
}
