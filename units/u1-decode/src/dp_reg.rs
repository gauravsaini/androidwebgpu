//! Data-processing (register) decoder family.

use pathn_contracts::cpu::InsnKind;

/// Data-processing (register). Key = bits[28:24].
pub fn decode(word: u32) -> Option<InsnKind> {
    match (word >> 24) & 0x1F {
        // Add/subtract (shifted register): sf op S 01011 shift 0 Rm imm6 Rn Rd.
        0b01011 => {
            let shifted = (word >> 21) & 1 == 0;
            let shift = (word >> 22) & 0x3;
            if shifted && shift < 0b11 {
                Some(InsnKind::DataProc) // ADD / SUB / ADDS / SUBS (including CMP / CMN reg)
            } else {
                None // extended register / reserved shift: out of scope
            }
        }
        // Logical (shifted register): sf opc 01010 shift N Rm imm6 Rn Rd.
        0b01010 => {
            let sf = (word >> 31) & 1;
            let opc = (word >> 29) & 0x3;
            let n = (word >> 21) & 1;
            let shift = (word >> 22) & 0x3;
            let imm6 = (word >> 10) & 0x3F;
            // shift==0b11 (ROR) is reserved; 32-bit shift amount >= 32 is
            // unallocated; 32-bit ORR with N==1 is reserved (GB-4 refinement).
            // N==1 itself is valid: it selects BIC / ORN / EON / BICS.
            let encoding_valid = shift < 0b11
                && (sf == 1 || (imm6 & 0x20) == 0)
                && (sf == 1 || opc != 0b01 || n == 0);
            if encoding_valid {
                Some(InsnKind::DataProc) // AND / BIC / ORR / ORN / EOR / EON / ANDS (TST) / BICS
            } else {
                None
            }
        }
        // Data-processing (2 source): sf 0 S 11010 110 Rm 0010 op2 Rn Rd (GB-4).
        // LSLV / LSRV / ASRV / RORV. bit30 == 0 is what separates this class
        // from data-processing (1 source); pinning it keeps CLZ (bit30 == 1)
        // from ever being misread here.
        // Data-processing (1 source): sf 1 S 11010 110 00000 opcode Rn Rd (GB-7).
        // CLZ only (opcode == 0b000100, S == 0); RBIT/REV*/CLS stay Illegal.
        // The 32-bit form (sf == 0) is recognized and trapped in U2.
        0b11010 => {
            // Conditional select: sf op S 11010100 Rm cond op2 Rn Rd
            // (GB-15, encoding corrected GB-26). bits[30:21] ==
            // 0xD4 (op=0: CSEL/CSINC) or 0x2D4 (op=1: CSINV/CSNEG);
            // bit29 (S) is 0, op2 (bits[11:10]) picks within the pair.
            // GB-15 only matched op=0, so real CSINV/CSNEG words (op=1,
            // e.g. 0xDA80202A = csinv x10, x1, x0, hs, measured kernel
            // halt at step 1249127) fell through to Illegal. This check
            // must precede the 2-source check below: a CSEL with
            // cond == 0b0010 would otherwise alias the 2-source class
            // (opcode2 >> 2 == cond).
            let b30_21 = (word >> 21) & 0x3FF;
            if b30_21 == 0xD4 || b30_21 == 0x2D4 {
                return Some(InsnKind::DataProc);
            }
            let bit30 = (word >> 30) & 1;
            let bit29 = (word >> 29) & 1;
            let bit21 = (word >> 21) & 1;
            let opcode2 = (word >> 10) & 0x3F;
            if bit30 == 0 && bit29 == 0 && bit21 == 0 && ((opcode2 >> 2) == 0b0010) {
                Some(InsnKind::DataProc)
            } else if bit30 == 1
                && bit29 == 0
                && ((word >> 21) & 0x7) == 0b110
                && ((word >> 16) & 0x1F) == 0
                && opcode2 == 0b000100
            {
                Some(InsnKind::DataProc) // CLZ
            } else {
                None
            }
        }
        // Data-processing (3 source): sf op54 11011 op31 Rm o0 Ra Rn Rd (GB-8).
        // The defined multiply class (op54 == 00/01/10) is recognized and the
        // lifter sorts it out: 64-bit MADD lifts, 32-bit MADD / MSUB /
        // long-multiply trap with explicit reasons. op54 == 11 is unallocated.
        0b11011 => {
            let op54 = (word >> 29) & 0x3;
            if op54 < 0b11 {
                Some(InsnKind::DataProc) // MADD / MSUB / SMADDL / UMADDL / ...
            } else {
                None
            }
        }
        _ => None,
    }
}
