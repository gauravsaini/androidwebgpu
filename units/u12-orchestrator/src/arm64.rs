//! Pure AArch64 execution helpers: condition evaluation, NZCV flag updates,
//! and bitwise / shift operations.
//!
//! Follows LLD Box Pure Function & Explicit Contract Rule:
//! All functions here are deterministic, pure transformations with explicit contracts.

pub const FLAG_N: u64 = 1 << 31;
pub const FLAG_Z: u64 = 1 << 30;
pub const FLAG_C: u64 = 1 << 29;
pub const FLAG_V: u64 = 1 << 28;
pub const FLAGS_NZCV_MASK: u64 = 0xF000_0000;

/// Pure evaluation of AArch64 condition codes against PSTATE NZCV flags.
///
/// Implements all 14 architectural condition codes plus AL/NV (14/15).
pub fn condition_holds(cond: u8, pstate: u64) -> bool {
    let n = ((pstate >> 31) & 1) != 0;
    let z = ((pstate >> 30) & 1) != 0;
    let c = ((pstate >> 29) & 1) != 0;
    let v = ((pstate >> 28) & 1) != 0;
    match cond & 0xF {
        0b0000 => z,              // EQ: Z == 1
        0b0001 => !z,             // NE: Z == 0
        0b0010 => c,              // CS / HS: C == 1
        0b0011 => !c,             // CC / LO: C == 0
        0b0100 => n,              // MI: N == 1
        0b0101 => !n,             // PL: N == 0
        0b0110 => v,              // VS: V == 1
        0b0111 => !v,             // VC: V == 0
        0b1000 => c && !z,        // HI: C == 1 and Z == 0
        0b1001 => !c || z,        // LS: !(C == 1 and Z == 0)
        0b1010 => n == v,         // GE: N == V
        0b1011 => n != v,         // LT: N != V
        0b1100 => !z && (n == v), // GT: Z == 0 and N == V
        0b1101 => z || (n != v),  // LE: !(Z == 0 and N == V)
        0b1110 | 0b1111 => true,  // AL / NV: Always
        _ => unreachable!(),
    }
}

/// Compute updated NZCV flags for 64-bit addition (ADDS / CMN).
pub fn nzcv_add64(a: u64, b: u64) -> u64 {
    let res = a.wrapping_add(b);
    let n = (res >> 63) & 1;
    let z = if res == 0 { 1 } else { 0 };
    let c = if res < a { 1 } else { 0 };
    let v = (((!(a ^ b)) & (a ^ res)) >> 63) & 1;
    (n << 31) | (z << 30) | (c << 29) | (v << 28)
}

/// Compute updated NZCV flags for 32-bit addition (ADDS / CMN).
pub fn nzcv_add32(a: u32, b: u32) -> u64 {
    let res = a.wrapping_add(b);
    let n = ((res >> 31) & 1) as u64;
    let z = if res == 0 { 1 } else { 0 };
    let c = if res < a { 1 } else { 0 };
    let v = (((!(a ^ b)) & (a ^ res)) >> 31) as u64 & 1;
    (n << 31) | (z << 30) | (c << 29) | (v << 28)
}

/// Compute updated NZCV flags for 64-bit subtraction (SUBS / CMP).
pub fn nzcv_sub64(a: u64, b: u64) -> u64 {
    let res = a.wrapping_sub(b);
    let n = (res >> 63) & 1;
    let z = if res == 0 { 1 } else { 0 };
    let c = if a >= b { 1 } else { 0 };
    let v = (((a ^ b) & (a ^ res)) >> 63) & 1;
    (n << 31) | (z << 30) | (c << 29) | (v << 28)
}

/// Compute updated NZCV flags for 32-bit subtraction (SUBS / CMP).
pub fn nzcv_sub32(a: u32, b: u32) -> u64 {
    let res = a.wrapping_sub(b);
    let n = ((res >> 31) & 1) as u64;
    let z = if res == 0 { 1 } else { 0 };
    let c = if a >= b { 1 } else { 0 };
    let v = (((a ^ b) & (a ^ res)) >> 31) as u64 & 1;
    (n << 31) | (z << 30) | (c << 29) | (v << 28)
}

/// Compute updated NZCV flags for 64-bit logical AND (ANDS / TST).
pub fn nzcv_and64(res: u64) -> u64 {
    let n = (res >> 63) & 1;
    let z = if res == 0 { 1 } else { 0 };
    (n << 31) | (z << 30)
}

/// Compute updated NZCV flags for 32-bit logical AND (ANDS / TST).
pub fn nzcv_and32(res: u32) -> u64 {
    let n = ((res >> 31) & 1) as u64;
    let z = if res == 0 { 1 } else { 0 };
    (n << 31) | (z << 30)
}

/// Evaluate shifted register operand (64-bit).
pub fn eval_shift64(val: u64, shift: u8, amount: u8) -> u64 {
    let amt = (amount & 63) as u32;
    match shift {
        0 => val.wrapping_shl(amt),
        1 => val.wrapping_shr(amt),
        2 => ((val as i64) >> amt) as u64,
        _ => val,
    }
}

/// Evaluate shifted register operand (32-bit).
pub fn eval_shift32(val: u32, shift: u8, amount: u8) -> u32 {
    let amt = (amount & 31) as u32;
    match shift {
        0 => val.wrapping_shl(amt),
        1 => val.wrapping_shr(amt),
        2 => ((val as i32) >> amt) as u32,
        _ => val,
    }
}

/// Decode AArch64 logical bitmask immediate (DecodeBitMasks per ARM ARM).
pub fn decode_logical_immediate(sf: u8, n: u8, immr: u8, imms: u8) -> Option<u64> {
    let len: u32 = match (n, imms) {
        (1, _) => 6,
        (0, s) if s & 0b111110 == 0b111100 => 5,
        (0, s) if s & 0b111100 == 0b111000 => 4,
        (0, s) if s & 0b111000 == 0b110000 => 3,
        (0, s) if s & 0b110000 == 0b100000 => 2,
        (0, s) if s & 0b100000 == 0b000000 => 1,
        _ => return None,
    };
    if sf == 0 && len == 6 {
        return None;
    }
    let esize: u32 = 1 << len;
    let levels: u8 = (esize - 1) as u8;
    let s = (imms & levels) as u32;
    let r = (immr & levels) as u32;
    if s == levels as u32 {
        return None;
    }
    let ones = if s + 1 == 64 {
        u64::MAX
    } else {
        (1u64 << (s + 1)) - 1
    };
    let ror = if r == 0 {
        ones
    } else {
        let mask = if esize == 64 {
            u64::MAX
        } else {
            (1u64 << esize) - 1
        };
        let lo = (ones & mask) >> r;
        let hi = (ones & mask) << (esize - r);
        (lo | hi) & mask
    };
    let mut val = ror;
    let mut width = esize;
    while width < 64 {
        val |= val << width;
        width *= 2;
    }
    if sf == 0 {
        val &= 0xFFFF_FFFF;
    }
    Some(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_codes_eq_ne() {
        assert!(condition_holds(0b0000, FLAG_Z)); // EQ with Z=1
        assert!(!condition_holds(0b0000, 0)); // EQ with Z=0
        assert!(condition_holds(0b0001, 0)); // NE with Z=0
        assert!(!condition_holds(0b0001, FLAG_Z)); // NE with Z=1
    }

    #[test]
    fn test_condition_codes_cs_cc() {
        assert!(condition_holds(0b0010, FLAG_C)); // CS with C=1
        assert!(!condition_holds(0b0010, 0)); // CS with C=0
        assert!(condition_holds(0b0011, 0)); // CC with C=0
        assert!(!condition_holds(0b0011, FLAG_C)); // CC with C=1
    }

    #[test]
    fn test_condition_codes_mi_pl() {
        assert!(condition_holds(0b0100, FLAG_N)); // MI with N=1
        assert!(!condition_holds(0b0100, 0)); // MI with N=0
        assert!(condition_holds(0b0101, 0)); // PL with N=0
        assert!(!condition_holds(0b0101, FLAG_N)); // PL with N=1
    }

    #[test]
    fn test_condition_codes_vs_vc() {
        assert!(condition_holds(0b0110, FLAG_V)); // VS with V=1
        assert!(!condition_holds(0b0110, 0)); // VS with V=0
        assert!(condition_holds(0b0111, 0)); // VC with V=0
        assert!(!condition_holds(0b0111, FLAG_V)); // VC with V=1
    }

    #[test]
    fn test_condition_codes_hi_ls() {
        assert!(condition_holds(0b1000, FLAG_C)); // HI: C=1 and Z=0
        assert!(!condition_holds(0b1000, FLAG_C | FLAG_Z)); // HI: C=1 and Z=1 -> false
        assert!(!condition_holds(0b1000, 0)); // HI: C=0 -> false

        assert!(condition_holds(0b1001, 0)); // LS: C=0 -> true
        assert!(condition_holds(0b1001, FLAG_C | FLAG_Z)); // LS: Z=1 -> true
        assert!(!condition_holds(0b1001, FLAG_C)); // LS: C=1 and Z=0 -> false
    }

    #[test]
    fn test_condition_codes_ge_lt() {
        assert!(condition_holds(0b1010, 0)); // GE: N=0, V=0 -> true
        assert!(condition_holds(0b1010, FLAG_N | FLAG_V)); // GE: N=1, V=1 -> true
        assert!(!condition_holds(0b1010, FLAG_N)); // GE: N=1, V=0 -> false
        assert!(!condition_holds(0b1010, FLAG_V)); // GE: N=0, V=1 -> false

        assert!(condition_holds(0b1011, FLAG_N)); // LT: N=1, V=0 -> true
        assert!(condition_holds(0b1011, FLAG_V)); // LT: N=0, V=1 -> true
        assert!(!condition_holds(0b1011, 0)); // LT: N=0, V=0 -> false
    }

    #[test]
    fn test_condition_codes_gt_le() {
        assert!(condition_holds(0b1100, 0)); // GT: Z=0, N=0, V=0 -> true
        assert!(!condition_holds(0b1100, FLAG_Z)); // GT: Z=1 -> false
        assert!(!condition_holds(0b1100, FLAG_N)); // GT: N!=V -> false

        assert!(condition_holds(0b1101, FLAG_Z)); // LE: Z=1 -> true
        assert!(condition_holds(0b1101, FLAG_N)); // LE: N!=V -> true
        assert!(!condition_holds(0b1101, 0)); // LE: Z=0, N==V -> false
    }

    #[test]
    fn test_condition_codes_al() {
        assert!(condition_holds(0b1110, 0));
        assert!(condition_holds(0b1110, FLAGS_NZCV_MASK));
        assert!(condition_holds(0b1111, 0));
    }

    #[test]
    fn test_nzcv_sub64_cmp() {
        // 5 - 5 == 0 -> Z=1, C=1 (no borrow), N=0, V=0
        let flags = nzcv_sub64(5, 5);
        assert_eq!(flags, FLAG_Z | FLAG_C);

        // 3 - 5 == -2 -> N=1, C=0 (borrow), Z=0, V=0
        let flags = nzcv_sub64(3, 5);
        assert_eq!(flags, FLAG_N);

        // 5 - 3 == 2 -> C=1, N=0, Z=0, V=0
        let flags = nzcv_sub64(5, 3);
        assert_eq!(flags, FLAG_C);

        // Signed overflow: 0x7FFF_FFFF_FFFF_FFFF - (-1)
        let flags = nzcv_sub64(0x7FFF_FFFF_FFFF_FFFF, (-1i64) as u64);
        assert_eq!(flags & FLAG_V, FLAG_V);
    }

    #[test]
    fn test_nzcv_add64_cmn() {
        // 5 + (-5) == 0 -> Z=1, C=1 (carry out)
        let flags = nzcv_add64(5, (-5i64) as u64);
        assert_eq!(flags & (FLAG_Z | FLAG_C), FLAG_Z | FLAG_C);

        // 0xFFFF_FFFF_FFFF_FFFF + 1 == 0 -> C=1, Z=1
        let flags = nzcv_add64(u64::MAX, 1);
        assert_eq!(flags, FLAG_Z | FLAG_C);
    }

    #[test]
    fn test_nzcv_and64_tst() {
        // 0xF0 & 0x0F == 0 -> Z=1
        let flags = nzcv_and64(0xF0 & 0x0F);
        assert_eq!(flags, FLAG_Z);

        // 0xF0 & 0x80 -> N=0, Z=0
        let flags = nzcv_and64(0xF0 & 0x80);
        assert_eq!(flags, 0);

        // High bit set -> N=1
        let flags = nzcv_and64(0x8000_0000_0000_0000);
        assert_eq!(flags, FLAG_N);
    }

    #[test]
    fn test_decode_logical_imm() {
        // TST X0, #1 -> N=1, imms=0, immr=0 -> mask = 1
        let mask = decode_logical_immediate(1, 1, 0, 0);
        assert_eq!(mask, Some(1));
    }
}
