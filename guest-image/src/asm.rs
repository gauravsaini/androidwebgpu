//! Mini-assembler: AArch64 instruction encoders, hand-derived from the ARM ARM.
//!
//! Every encoder has a golden test with a hand-computed word below. Where a
//! u1-decode test vector overlaps (ADD imm `0x91004420`, B `0x14000001`, RET
//! `0xD65F03C0`, NOP `0xD503201F`), the formula is additionally cross-checked
//! against it in comments.
//!
//! The guest shell needs byte-equality tests (`w0 == 10` etc.) but this set
//! has no CMP/SUB. The [`eq_const`] helper documents the trick used instead:
//! for a byte `v` in `x0` and a static constant `c`,
//! `x1 = v + (256 - c)` lies in `[1, 511]`; `x1 << 56` (via ORR) is zero
//! (mod 2^64) iff `x1 ≡ 0 (mod 256)` iff `x1 == 256` iff `v == c`.
//! `image_asm_eq_trick_exhaustive` proves this for every byte value in Rust.

/// ADRP Xd, #page_off: `1 | immlo[30:29] | 10000[28:24] | immhi[23:5] | Rd`.
///
/// `page_off` = target_page - pc_page, signed 21-bit (i.e. ±1 MiB of pages).
pub fn enc_adrp(rd: u8, page_off: i32) -> u32 {
    assert!(rd < 32, "rd out of range");
    assert!(
        ((-1 << 20)..(1 << 20)).contains(&page_off),
        "page_off out of signed 21-bit range"
    );
    let u = page_off as u32;
    let immlo = u & 0x3;
    let immhi = (u >> 2) & 0x7_FFFF;
    (1 << 31) | (immlo << 29) | (0b10000 << 24) | (immhi << 5) | u32::from(rd)
}

/// ADD (immediate), 64-bit: `sf=1 | 0 | 0 | 10001 | sh | imm12 | Rn | Rd`.
pub fn enc_add_imm(rd: u8, rn: u8, imm12: u16, shift12: bool) -> u32 {
    assert!(rd < 32 && rn < 32, "register out of range");
    assert!(imm12 < 4096, "imm12 out of range");
    (1 << 31)
        | (0b10001 << 24)
        | (u32::from(shift12) << 22)
        | (u32::from(imm12) << 10)
        | (u32::from(rn) << 5)
        | u32::from(rd)
}

/// LDR (immediate, unsigned offset), 64-bit:
/// `11 | 111001 | 01 | imm12 | Rn | Rt`, imm12 scaled by 8.
pub fn enc_ldr_uimm64(rt: u8, rn: u8, byte_off: u16) -> u32 {
    assert!(rt < 32 && rn < 32, "register out of range");
    assert!(
        byte_off.is_multiple_of(8),
        "LDR uimm64 offset not 8-aligned"
    );
    let imm12 = byte_off / 8;
    assert!(imm12 < 4096, "imm12 out of range");
    (0b11 << 30)
        | (0b111001 << 24)
        | (0b01 << 22)
        | (u32::from(imm12) << 10)
        | (u32::from(rn) << 5)
        | u32::from(rt)
}

/// STR (immediate, unsigned offset), 64-bit:
/// `11 | 111001 | 00 | imm12 | Rn | Rt`, imm12 scaled by 8.
pub fn enc_str_uimm64(rt: u8, rn: u8, byte_off: u16) -> u32 {
    assert!(rt < 32 && rn < 32, "register out of range");
    assert!(
        byte_off.is_multiple_of(8),
        "STR uimm64 offset not 8-aligned"
    );
    let imm12 = byte_off / 8;
    assert!(imm12 < 4096, "imm12 out of range");
    (0b11 << 30)
        | (0b111001 << 24)
        | (u32::from(imm12) << 10)
        | (u32::from(rn) << 5)
        | u32::from(rt)
}

/// LDRB (immediate, unsigned offset): `00 | 111001 | 01 | imm12 | Rn | Rt`.
/// Byte accesses are unscaled.
pub fn enc_ldrb(rt: u8, rn: u8, byte_off: u16) -> u32 {
    assert!(rt < 32 && rn < 32, "register out of range");
    assert!(byte_off < 4096, "imm12 out of range");
    (0b111001 << 24)
        | (0b01 << 22)
        | (u32::from(byte_off) << 10)
        | (u32::from(rn) << 5)
        | u32::from(rt)
}

/// STRB (immediate, unsigned offset): `00 | 111001 | 00 | imm12 | Rn | Rt`.
pub fn enc_strb(rt: u8, rn: u8, byte_off: u16) -> u32 {
    assert!(rt < 32 && rn < 32, "register out of range");
    assert!(byte_off < 4096, "imm12 out of range");
    (0b111001 << 24) | (u32::from(byte_off) << 10) | (u32::from(rn) << 5) | u32::from(rt)
}

/// MOVZ (wide immediate), 64-bit: `1 | 10 | 100101 | hw | imm16 | Rd`.
/// `hw` selects the 16-bit lane: 0, 1, 2, 3 (shift = hw * 16).
pub fn enc_movz(rd: u8, imm16: u16, hw: u8) -> u32 {
    assert!(rd < 32, "rd out of range");
    assert!(hw < 4, "hw out of range");
    (1 << 31)
        | (0b10 << 29)
        | (0b100101 << 23)
        | (u32::from(hw) << 21)
        | (u32::from(imm16) << 5)
        | u32::from(rd)
}

/// ORR (shifted register), 64-bit:
/// `1 | 01 | 01010 | shift | 0 | Rm | imm6 | Rn | Rd`.
/// shift: 0 = LSL, 1 = LSR, 2 = ASR, 3 = ROR.
pub fn enc_orr_shift(rd: u8, rn: u8, rm: u8, shift: u8, amount: u8) -> u32 {
    assert!(rd < 32 && rn < 32 && rm < 32, "register out of range");
    assert!(shift < 4, "shift out of range");
    assert!(amount < 64, "shift amount out of range");
    (1 << 31)
        | (0b01 << 29)
        | (0b01010 << 24)
        | (u32::from(shift) << 22)
        | (u32::from(rm) << 16)
        | (u32::from(amount) << 10)
        | (u32::from(rn) << 5)
        | u32::from(rd)
}

/// CBZ, 64-bit: `1 | 0 | 1 | 10100 | imm19 | Rt`. `off` is the signed
/// byte offset divided by 4.
pub fn enc_cbz(rt: u8, off: i32) -> u32 {
    assert!(rt < 32, "rt out of range");
    assert!(
        ((-1 << 18)..(1 << 18)).contains(&off),
        "CBZ offset out of range"
    );
    (1 << 31) | (1 << 29) | (0b10100 << 24) | (((off as u32) & 0x7_FFFF) << 5) | u32::from(rt)
}

/// CBNZ, 64-bit: `1 | 0 | 1 | 10101 | imm19 | Rt`.
pub fn enc_cbnz(rt: u8, off: i32) -> u32 {
    assert!(rt < 32, "rt out of range");
    assert!(
        ((-1 << 18)..(1 << 18)).contains(&off),
        "CBNZ offset out of range"
    );
    (1 << 31) | (1 << 29) | (0b10101 << 24) | (((off as u32) & 0x7_FFFF) << 5) | u32::from(rt)
}

/// B: `0 | 0 | 0 | 101 | imm26`. `off` is the signed byte offset / 4.
pub fn enc_b(off: i32) -> u32 {
    assert!(
        ((-1 << 25)..(1 << 25)).contains(&off),
        "B offset out of range"
    );
    (0b000101 << 26) | ((off as u32) & 0x03FF_FFFF)
}

/// BL: `1 | 0 | 0 | 101 | imm26`.
pub fn enc_bl(off: i32) -> u32 {
    assert!(
        ((-1 << 25)..(1 << 25)).contains(&off),
        "BL offset out of range"
    );
    (0b100101 << 26) | ((off as u32) & 0x03FF_FFFF)
}

/// RET <Xn>: `1101011001011111000000 | Rn | 00000`.
pub fn enc_ret(rn: u8) -> u32 {
    assert!(rn < 32, "rn out of range");
    0xD65F_0000 | (u32::from(rn) << 5)
}

/// WFI: HINT #8, `0xD503207F`.
pub fn enc_wfi() -> u32 {
    0xD503_207F
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_asm_adrp() {
        // Hand-computed: ADRP X10, page+1 -> 1|01|10000|0..0|1010
        assert_eq!(enc_adrp(10, 1), 0xB000_000A);
        // Hand-computed: ADRP X0, page-1 -> immlo=3, immhi=0x7FFFF
        assert_eq!(enc_adrp(0, -1), 0xF0FF_FFE0);
        // Zero offset is the identity page.
        assert_eq!(enc_adrp(5, 0), 0x9000_0005);
    }

    #[test]
    fn image_asm_add_imm() {
        // Hand-computed: ADD X1, X2, #0x123
        assert_eq!(enc_add_imm(1, 2, 0x123, false), 0x9104_8C41);
        // Cross-check vs u1-decode vector 0x91004420 = ADD X0, X1, #0x11.
        assert_eq!(enc_add_imm(0, 1, 0x11, false), 0x9100_4420);
        // LSL #12 variant: ADD X0, X0, #1, LSL #12
        assert_eq!(enc_add_imm(0, 0, 1, true), 0x9140_0400);
    }

    #[test]
    fn image_asm_ldr_uimm64() {
        // Hand-computed: LDR X1, [X2, #0x28] (imm12 = 5)
        assert_eq!(enc_ldr_uimm64(1, 2, 0x28), 0xF940_1441);
        // Canonical: LDR X0, [X1] = 0xF9400020.
        assert_eq!(enc_ldr_uimm64(0, 1, 0), 0xF940_0020);
    }

    #[test]
    fn image_asm_str_uimm64() {
        // Hand-computed: STR X1, [X2, #0x28]
        assert_eq!(enc_str_uimm64(1, 2, 0x28), 0xF900_1441);
        // Canonical: STR X0, [X1] = 0xF9000020.
        assert_eq!(enc_str_uimm64(0, 1, 0), 0xF900_0020);
    }

    #[test]
    fn image_asm_ldrb() {
        // Hand-computed: LDRB W1, [X2, #5]
        assert_eq!(enc_ldrb(1, 2, 5), 0x3940_1441);
        // Canonical: LDRB W0, [X1] = 0x39400020.
        assert_eq!(enc_ldrb(0, 1, 0), 0x3940_0020);
    }

    #[test]
    fn image_asm_strb() {
        // Hand-computed: STRB W1, [X2, #5]
        assert_eq!(enc_strb(1, 2, 5), 0x3900_1441);
        // Canonical: STRB W0, [X1] = 0x39000020.
        assert_eq!(enc_strb(0, 1, 0), 0x3900_0020);
        // WZR store (used for NUL termination): STRB WZR, [X11]
        // = 0x39000000 | (11<<5) | 31
        assert_eq!(enc_strb(31, 11, 0), 0x3900_017F);
    }

    #[test]
    fn image_asm_movz() {
        // Hand-computed: MOVZ X1, #0x1234
        assert_eq!(enc_movz(1, 0x1234, 0), 0xD282_4681);
        // Cross-check vs u1-decode vector 0xD2800000 = MOVZ X0, #0.
        assert_eq!(enc_movz(0, 0, 0), 0xD280_0000);
        // Console base: MOVZ X1, #0x900, LSL #16 -> 0x09000000
        // = 0xD2A00000 | (0x900 << 5) | 1
        assert_eq!(enc_movz(1, 0x900, 1), 0xD2A1_2001);
    }

    #[test]
    fn image_asm_orr_shift() {
        // Hand-computed: ORR X1, X2, X3, LSL #4
        assert_eq!(enc_orr_shift(1, 2, 3, 0, 4), 0xAA03_1041);
        // Canonical: ORR X0, X1, X2 = 0xAA020020.
        assert_eq!(enc_orr_shift(0, 1, 2, 0, 0), 0xAA02_0020);
        // Register copy idiom: ORR X11, XZR, X10
        assert_eq!(enc_orr_shift(11, 31, 10, 0, 0), 0xAA0A_03EB);
        // Equality-trick shift: ORR X2, XZR, X1, LSL #56
        assert_eq!(enc_orr_shift(2, 31, 1, 0, 56), 0xAA01_E3E2);
    }

    #[test]
    fn image_asm_cbz() {
        // Hand-computed: CBZ X1, #32 (off = 8 words)
        assert_eq!(enc_cbz(1, 8), 0xB400_0101);
        // Canonical: CBZ X0, #8 -> 0xB4000040.
        assert_eq!(enc_cbz(0, 2), 0xB400_0040);
    }

    #[test]
    fn image_asm_cbnz() {
        // Hand-computed: CBNZ X1, #-16 (off = -4 words)
        assert_eq!(enc_cbnz(1, -4), 0xB5FF_FF81);
        // Canonical: CBNZ X0, #8 -> 0xB5000040.
        assert_eq!(enc_cbnz(0, 2), 0xB500_0040);
    }

    #[test]
    fn image_asm_b() {
        // Hand-computed: B #64 (off = 16 words)
        assert_eq!(enc_b(16), 0x1400_0010);
        // Cross-check vs u1-decode vector 0x14000001 = B #4.
        assert_eq!(enc_b(1), 0x1400_0001);
        // Backwards: B #-8 (off = -2)
        assert_eq!(enc_b(-2), 0x17FF_FFFE);
    }

    #[test]
    fn image_asm_bl() {
        // Hand-computed: BL #64
        assert_eq!(enc_bl(16), 0x9400_0010);
        // BL #4
        assert_eq!(enc_bl(1), 0x9400_0001);
    }

    #[test]
    fn image_asm_ret() {
        // Cross-check vs u1-decode vector 0xD65F03C0 = RET X30.
        assert_eq!(enc_ret(30), 0xD65F_03C0);
        assert_eq!(enc_ret(0), 0xD65F_0000);
    }

    #[test]
    fn image_asm_wfi() {
        // WFI = HINT #8; same family as u1's NOP vector 0xD503201F.
        assert_eq!(enc_wfi(), 0xD503_207F);
    }

    /// Exhaustive proof of the byte-equality trick used by the guest shell:
    /// for a byte value `v` in x0 and static constant `c`,
    /// `x1 = v + (256 - c)` in `[1, 511]`; `(x1 << 56) mod 2^64 == 0`
    /// iff `x1 == 256` iff `v == c`.
    #[test]
    fn image_asm_eq_trick_exhaustive() {
        for v in 0u16..=255 {
            for c in [0u16, 10, 13, 32, 4, 5, 98, 99, 101, 104, 108, 111, 112, 255] {
                let x1 = v + (256 - c);
                assert!((1..=511).contains(&x1));
                let x2 = (x1 as u64).wrapping_shl(56);
                assert_eq!(x2 == 0, v == c, "trick failed for v={v} c={c}");
            }
        }
    }
}
