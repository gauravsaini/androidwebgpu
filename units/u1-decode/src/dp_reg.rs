//! Data-processing (register) decoder family.

use pathn_contracts::cpu::InsnKind;

/// Data-processing (register). Key = bits[28:24].
pub fn decode(word: u32) -> Option<InsnKind> {
    match (word >> 24) & 0x1F {
        // Add/subtract (shifted register): sf op S 01011 shift 0 Rm imm6 Rn Rd.
        // Add/subtract (extended register): sf op S 01011 opt 1 Rm option imm3 Rn Rd.
        0b01011 => {
            let bit21 = (word >> 21) & 1;
            if bit21 == 0 {
                // Shifted register form.
                let shift = (word >> 22) & 0x3;
                let sf = (word >> 31) & 1;
                let imm6 = (word >> 10) & 0x3F;
                if shift < 0b11 && (sf == 1 || (imm6 & 0x20) == 0) {
                    Some(InsnKind::DataProc) // ADD / SUB / ADDS / SUBS (including CMP / CMN reg)
                } else {
                    None // ROR and 32-bit shifts >= 32 are unallocated here
                }
            } else {
                // Extended register form requires bits[23:22] == 00 and
                // imm3 in 0..=4. Implemented unsigned extends lift; signed
                // extends trap honestly.
                let option_prefix = (word >> 22) & 0x3;
                let imm3 = (word >> 10) & 0x7;
                if option_prefix == 0 && imm3 <= 4 {
                    Some(InsnKind::DataProc)
                } else {
                    None
                }
            }
        }
        // Logical (shifted register): sf opc 01010 shift N Rm imm6 Rn Rd.
        0b01010 => {
            let sf = (word >> 31) & 1;
            let imm6 = (word >> 10) & 0x3F;
            // All four shift types (LSL/LSR/ASR/ROR) are defined. In the
            // 32-bit form, shift amounts >= 32 are unallocated. N complements
            // the shifted operand for BIC / ORN / EON / BICS.
            let encoding_valid = sf == 1 || (imm6 & 0x20) == 0;
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
        // Recognize the audited CLZ/RBIT/REV*/CLS operations, plus AUTIA's
        // fixed key selector. The 32-bit forms are recognized and trapped in U2.
        0b11010 => {
            // ADC/SBC (with carry): sf op S 11010000 Rm 000000 Rn Rd.
            // bits[28:21] == 0xD0, bits[15:10] == 0. The lifter traps these
            // with an explicit carry-flag reason (NZCV not in IrOp).
            if (word >> 21) & 0xFF == 0xD0 && (word >> 10) & 0x3F == 0 {
                return Some(InsnKind::DataProc);
            }
            // Conditional compare: sf op 1 11010010 Rm cond o2 0 Rn 0 nzcv.
            // op selects CCMN/CCMP; o2 selects register or immediate form.
            let b30_21 = (word >> 21) & 0x3FF;
            let compare_form = b30_21 == 0x1D2 || b30_21 == 0x3D2;
            let compare_o2 = (word >> 10) & 0x3;
            if compare_form && (compare_o2 == 0 || compare_o2 == 0b10) && (word & 0x10) == 0 {
                return Some(InsnKind::DataProc); // CCMN / CCMP (register or immediate)
            }
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
            if (b30_21 == 0xD4 || b30_21 == 0x2D4) && (word >> 10) & 0x3 < 2 && (word & 0x10) == 0 {
                return Some(InsnKind::DataProc);
            }
            let bit30 = (word >> 30) & 1;
            let bit29 = (word >> 29) & 1;
            let sf = (word >> 31) & 1;
            let op3 = (word >> 21) & 0x7;
            let rm = (word >> 16) & 0x1F;
            let opcode2 = (word >> 10) & 0x3F;

            // RMIF: sf=1, bits[30:21]=0b0111010000, fixed op bits[14:10]=1,
            // and bit[4]=0. The rotation and NZCV mask occupy their own fields.
            if sf == 1
                && (word >> 21) & 0x3FF == 0x1D0
                && (word >> 10) & 0x1F == 0b00001
                && (word & 0x10) == 0
            {
                return Some(InsnKind::DataProc);
            }

            if op3 == 0b110 {
                if bit30 == 0 && bit29 == 0 {
                    let is_crc32 = (0b010000..=0b010111).contains(&opcode2)
                        && ((sf == 1) == (opcode2 & 0b11 == 0b11));
                    let is_64bit_extension = sf == 1 && matches!(opcode2, 0 | 4 | 5 | 12);
                    if matches!(opcode2, 2 | 3 | 8..=11) || is_crc32 || is_64bit_extension {
                        return Some(InsnKind::DataProc); // div/shift, CRC32, MTE, PACGA
                    }
                }
                if bit30 == 0 && bit29 == 1 && sf == 1 && opcode2 == 0 {
                    return Some(InsnKind::DataProc); // SUBPS
                }
                if bit30 == 1 && bit29 == 0 {
                    if rm == 0 {
                        let valid_one_source = matches!(opcode2, 0 | 1 | 4 | 5)
                            || (opcode2 == 2)
                            || (sf == 1 && opcode2 == 3);
                        if valid_one_source {
                            return Some(InsnKind::DataProc); // RBIT / REV* / CLZ / CLS
                        }
                    } else if sf == 1 && rm == 1 && opcode2 == 4 {
                        return Some(InsnKind::DataProc); // AUTIA
                    }
                }
            }

            None
        }
        // Data-processing (3 source): sf op54 11011 op31 Rm o0 Ra Rn Rd (GB-8).
        // MADD/MSUB and long-multiply encodings use op54 == 00. op31 selects
        // the operation; long-multiply and high-multiply forms require sf=1.
        0b11011 => {
            let op54 = (word >> 29) & 0x3;
            let op31 = (word >> 21) & 0x7;
            let sf = (word >> 31) & 1;
            let o0 = (word >> 15) & 1;
            let valid_op = match op31 {
                0b000 => true,                       // MADD / MSUB
                0b001 | 0b101 => sf == 1,            // SMADDL/SMSUBL, UMADDL/UMSUBL
                0b010 | 0b110 => sf == 1 && o0 == 0, // SMULH / UMULH
                _ => false,
            };
            if op54 == 0 && valid_op {
                Some(InsnKind::DataProc) // MADD / MSUB / SMADDL / UMADDL / ...
            } else {
                None
            }
        }
        _ => None,
    }
}
