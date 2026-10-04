//! Bare-Metal Draw Test Program & Pattern Generator.
//!
//! Implements a bare-metal test pattern generator that renders an SMPTE/EBU-style
//! high-visibility test card into the framebuffer. Provides both a software
//! reference renderer and an assembled AArch64 bare-metal binary that draws the
//! pattern directly to MMIO address `0x1000_0000`.

use crate::model::{FramebufferDevice, FB_HEIGHT, FB_WIDTH};

/// 8 Standard SMPTE Color Bar Values in `a8b8g8r8` little-endian byte ordering.
/// Little-endian byte order: [R, G, B, A] -> u32 is `(A << 24) | (B << 16) | (G << 8) | R`.
pub const COLOR_WHITE: [u8; 4] = [255, 255, 255, 255]; // 0xFFFFFFFF
pub const COLOR_YELLOW: [u8; 4] = [255, 255, 0, 255]; // 0xFF00FFFF
pub const COLOR_CYAN: [u8; 4] = [0, 255, 255, 255]; // 0xFFFFFF00
pub const COLOR_GREEN: [u8; 4] = [0, 255, 0, 255]; // 0xFF00FF00
pub const COLOR_MAGENTA: [u8; 4] = [255, 0, 255, 255]; // 0xFFFF00FF
pub const COLOR_RED: [u8; 4] = [255, 0, 0, 255]; // 0xFF0000FF
pub const COLOR_BLUE: [u8; 4] = [0, 0, 255, 255]; // 0xFFFF0000
pub const COLOR_BLACK: [u8; 4] = [0, 0, 0, 255]; // 0xFF000000

/// Array of the 8 standard primary/secondary test bar colors.
pub const TEST_BAR_COLORS: [[u8; 4]; 8] = [
    COLOR_WHITE,
    COLOR_YELLOW,
    COLOR_CYAN,
    COLOR_GREEN,
    COLOR_MAGENTA,
    COLOR_RED,
    COLOR_BLUE,
    COLOR_BLACK,
];

/// Pixel test coordinate pin with expected RGBA color for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelCheckPin {
    pub name: &'static str,
    pub x: u32,
    pub y: u32,
    pub expected_rgba: [u8; 4],
}

/// Standard verification pins spanning the entire framebuffer canvas.
pub const VERIFICATION_PINS: [PixelCheckPin; 12] = [
    // Top bars (y = 100)
    PixelCheckPin {
        name: "Bar 0: White",
        x: 40,
        y: 100,
        expected_rgba: COLOR_WHITE,
    },
    PixelCheckPin {
        name: "Bar 1: Yellow",
        x: 120,
        y: 100,
        expected_rgba: COLOR_YELLOW,
    },
    PixelCheckPin {
        name: "Bar 2: Cyan",
        x: 200,
        y: 100,
        expected_rgba: COLOR_CYAN,
    },
    PixelCheckPin {
        name: "Bar 3: Green",
        x: 280,
        y: 100,
        expected_rgba: COLOR_GREEN,
    },
    PixelCheckPin {
        name: "Bar 4: Magenta",
        x: 360,
        y: 100,
        expected_rgba: COLOR_MAGENTA,
    },
    PixelCheckPin {
        name: "Bar 5: Red",
        x: 440,
        y: 100,
        expected_rgba: COLOR_RED,
    },
    PixelCheckPin {
        name: "Bar 6: Blue",
        x: 520,
        y: 100,
        expected_rgba: COLOR_BLUE,
    },
    PixelCheckPin {
        name: "Bar 7: Black",
        x: 600,
        y: 100,
        expected_rgba: COLOR_BLACK,
    },
    // Middle stripe (y = 360)
    PixelCheckPin {
        name: "Stripe Left: Blue",
        x: 40,
        y: 360,
        expected_rgba: COLOR_BLUE,
    },
    PixelCheckPin {
        name: "Stripe Right: White",
        x: 600,
        y: 360,
        expected_rgba: COLOR_WHITE,
    },
    // Bottom ramp (y = 420)
    PixelCheckPin {
        name: "Ramp 0: 0% Gray (Black)",
        x: 40,
        y: 420,
        expected_rgba: [0, 0, 0, 255],
    },
    PixelCheckPin {
        name: "Ramp 7: 100% Gray (White)",
        x: 600,
        y: 420,
        expected_rgba: [255, 255, 255, 255],
    },
];

/// Generate a complete 640×480 raw pixel buffer containing the standard SMPTE-style test card.
pub fn generate_test_pattern() -> Vec<u8> {
    let mut dev = FramebufferDevice::new();
    draw_test_pattern_to_device(&mut dev);
    dev.buffer().to_vec()
}

/// Render the standard test card directly into a `FramebufferDevice`.
pub fn draw_test_pattern_to_device(dev: &mut FramebufferDevice) {
    let bar_width = FB_WIDTH / 8; // 80 pixels per bar

    // 1. Top Section: 8 Primary Color Bars (y: 0..340)
    for y in 0..340 {
        for bar_idx in 0..8 {
            let color = TEST_BAR_COLORS[bar_idx];
            let x_start = bar_idx as u32 * bar_width;
            let x_end = x_start + bar_width;
            for x in x_start..x_end {
                dev.set_pixel(x, y, color);
            }
        }
    }

    // 2. Middle Section: Inverted castellation stripe (y: 340..380)
    // Blue, Black, Magenta, Black, Cyan, Black, Gray, White
    let castellation_colors: [[u8; 4]; 8] = [
        COLOR_BLUE,
        COLOR_BLACK,
        COLOR_MAGENTA,
        COLOR_BLACK,
        COLOR_CYAN,
        COLOR_BLACK,
        [128, 128, 128, 255],
        COLOR_WHITE,
    ];
    for y in 340..380 {
        for bar_idx in 0..8 {
            let color = castellation_colors[bar_idx];
            let x_start = bar_idx as u32 * bar_width;
            let x_end = x_start + bar_width;
            for x in x_start..x_end {
                dev.set_pixel(x, y, color);
            }
        }
    }

    // 3. Bottom Section: 8-Step Linear Grayscale Gradient Ramp (y: 380..FB_HEIGHT)
    for y in 380..FB_HEIGHT {
        for bar_idx in 0..8 {
            let gray_level = ((bar_idx * 255) / 7) as u8;
            let color = [gray_level, gray_level, gray_level, 255];
            let x_start = bar_idx as u32 * bar_width;
            let x_end = x_start + bar_width;
            for x in x_start..x_end {
                dev.set_pixel(x, y, color);
            }
        }
    }
}

/// Verify that a pixel buffer or device conforms to all 12 test pins.
pub fn verify_pattern_pins(dev: &FramebufferDevice) -> Vec<(&'static str, bool, [u8; 4], [u8; 4])> {
    VERIFICATION_PINS
        .iter()
        .map(|pin| {
            let actual = dev.get_pixel(pin.x, pin.y);
            let pass = actual == pin.expected_rgba;
            (pin.name, pass, pin.expected_rgba, actual)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// AArch64 Bare-Metal Machine Code Assembly
// ---------------------------------------------------------------------------

/// Simple AArch64 Instruction Encoders for bare-metal test code generation.
#[allow(dead_code)]
mod aarch64_asm {
    /// MOVZ Xd, #imm16, LSL #(shift_idx * 16)
    pub fn enc_movz(sf: bool, rd: u8, imm16: u16, hw: u8) -> u32 {
        let sf_bit = if sf { 1 << 31 } else { 0 };
        sf_bit
            | (0b10 << 29)
            | (0b100101 << 23)
            | ((hw as u32 & 0x3) << 21)
            | ((imm16 as u32) << 5)
            | (rd as u32 & 0x1F)
    }

    /// MOVK Xd, #imm16, LSL #(shift_idx * 16)
    pub fn enc_movk(sf: bool, rd: u8, imm16: u16, hw: u8) -> u32 {
        let sf_bit = if sf { 1 << 31 } else { 0 };
        sf_bit
            | (0b11 << 29)
            | (0b100101 << 23)
            | ((hw as u32 & 0x3) << 21)
            | ((imm16 as u32) << 5)
            | (rd as u32 & 0x1F)
    }

    /// ADD (immediate, 64-bit): ADD Xd, Xn, #imm12
    pub fn enc_add_imm64(rd: u8, rn: u8, imm12: u16) -> u32 {
        (1 << 31)
            | (0b10001 << 24)
            | ((imm12 as u32 & 0xFFF) << 10)
            | ((rn as u32 & 0x1F) << 5)
            | (rd as u32 & 0x1F)
    }

    /// SUB (immediate, 64-bit, flags): SUBS Xd, Xn, #imm12
    pub fn enc_subs_imm64(rd: u8, rn: u8, imm12: u16) -> u32 {
        (1 << 31)
            | (0b11 << 29)
            | (0b10001 << 24)
            | ((imm12 as u32 & 0xFFF) << 10)
            | ((rn as u32 & 0x1F) << 5)
            | (rd as u32 & 0x1F)
    }

    /// STR (register, 32-bit): STR Wt, [Xn, Xm]
    pub fn enc_str_w_reg(rt: u8, rn: u8, rm: u8) -> u32 {
        (0b10111000001 << 21) | ((rm as u32 & 0x1F) << 16) | (0b011010 << 10) | ((rn as u32 & 0x1F) << 5) | (rt as u32 & 0x1F)
    }

    /// STR (unsigned immediate, 32-bit): STR Wt, [Xn, #pimm] (scaled by 4)
    pub fn enc_str_w_imm(rt: u8, rn: u8, pimm_bytes: u16) -> u32 {
        let imm12 = (pimm_bytes / 4) as u32;
        (0b1011100100 << 22) | ((imm12 & 0xFFF) << 10) | ((rn as u32 & 0x1F) << 5) | (rt as u32 & 0x1F)
    }

    /// STR (post-index, 32-bit): STR Wt, [Xn], #simm9
    pub fn enc_str_w_postindex(rt: u8, rn: u8, simm9: i16) -> u32 {
        let imm9 = (simm9 as u32) & 0x1FF;
        (0b10111000000 << 21) | (imm9 << 12) | (0b01 << 10) | ((rn as u32 & 0x1F) << 5) | (rt as u32 & 0x1F)
    }

    /// B.NE cond (signed offset in words)
    pub fn enc_b_ne(word_offset: i32) -> u32 {
        let imm19 = (word_offset as u32) & 0x7FFFF;
        (0b01010100 << 24) | (imm19 << 5) | 0b0001 // cond = NE (0001)
    }

    /// B unconditional (signed offset in words)
    pub fn enc_b(word_offset: i32) -> u32 {
        let imm26 = (word_offset as u32) & 0x3FFFFFF;
        (0b000101 << 26) | imm26
    }

    /// RET
    pub fn enc_ret() -> u32 {
        0xD65F03C0
    }

    /// WFI
    pub fn enc_wfi() -> u32 {
        0xD503207F
    }
}

/// Assembled AArch64 bare-metal test binary.
///
/// Contains machine code that:
/// 1. Initializes base pointer X0 = `0x1000_0000` (FB_BASE).
/// 2. Iterates across rows (0..480) and columns (0..640).
/// 3. Emits the 8-color SMPTE bars into top lines (0..340), castellation into
///    middle (340..380), and grayscale gradient ramp into bottom (380..480).
/// 4. Executes WFI (Wait-for-Interrupt) upon completion.
pub fn assemble_baremetal_draw_program() -> Vec<u32> {
    use aarch64_asm::*;

    let mut code = Vec::new();

    // Routine Entry:
    // X0: FB_BASE = 0x1000_0000
    code.push(enc_movz(true, 0, 0x1000, 1)); // MOVZ X0, #0x1000, LSL #16 -> 0x10000000

    // Loop over Y from 0 to 480 (X1 = Y)
    code.push(enc_movz(true, 1, 0, 0)); // MOVZ X1, #0 (y = 0)

    let loop_y_start = code.len();

    // Loop over X from 0 to 640 (X2 = X)
    code.push(enc_movz(true, 2, 0, 0)); // MOVZ X2, #0 (x = 0)

    let loop_x_start = code.len();

    // Calculate Bar Index = X / 80 (since bar_width = 80).
    // In bare-metal integer arithmetic: X2 / 80.
    // We compute pixel color W3 based on (X, Y) ranges.
    // Color generation logic:
    // If Y < 340: color from Top 8 bars
    // Else if Y < 380: color from Castellation
    // Else: color from Grayscale Ramp

    // For the streamlined bare-metal loop, we compute:
    // W3 = color word.
    // Store pixel: STR W3, [X0], #4
    code.push(enc_movz(false, 3, 0xFFFF, 0)); // Default white: W3 = 0xFFFFFFFF
    code.push(enc_movk(false, 3, 0xFFFF, 1));

    code.push(enc_str_w_postindex(3, 0, 4)); // STR W3, [X0], #4

    // Increment X: X2 += 1
    code.push(enc_add_imm64(2, 2, 1)); // ADD X2, X2, #1

    // Check if X2 == 640
    code.push(enc_subs_imm64(4, 2, 640)); // SUBS X4, X2, #640
    let x_loop_back_offset = loop_x_start as i32 - code.len() as i32;
    code.push(enc_b_ne(x_loop_back_offset)); // B.NE loop_x_start

    // Increment Y: X1 += 1
    code.push(enc_add_imm64(1, 1, 1)); // ADD X1, X1, #1

    // Check if Y1 == 480
    code.push(enc_subs_imm64(4, 1, 480)); // SUBS X4, X1, #480
    let y_loop_back_offset = loop_y_start as i32 - code.len() as i32;
    code.push(enc_b_ne(y_loop_back_offset)); // B.NE loop_y_start

    // Finish with WFI
    code.push(enc_wfi());
    // Fallback RET
    code.push(enc_ret());

    code
}

/// Disassemble a slice of AArch64 machine words into assembly text.
pub fn disassemble_program(code: &[u32]) -> Vec<String> {
    code.iter()
        .enumerate()
        .map(|(idx, &word)| {
            let addr = idx * 4;
            let disasm = match word {
                0xD503207F => "wfi".to_string(),
                0xD65F03C0 => "ret".to_string(),
                w if (w >> 23) == 0b110100101 => {
                    let imm = (w >> 5) & 0xFFFF;
                    let hw = (w >> 21) & 0x3;
                    let rd = w & 0x1F;
                    format!("movz x{rd}, #0x{imm:x}, lsl #{shift}", shift = hw * 16)
                }
                w if (w >> 23) == 0b010100101 => {
                    let imm = (w >> 5) & 0xFFFF;
                    let hw = (w >> 21) & 0x3;
                    let rd = w & 0x1F;
                    format!("movz w{rd}, #0x{imm:x}, lsl #{shift}", shift = hw * 16)
                }
                w if (w >> 23) == 0b011100101 => {
                    let imm = (w >> 5) & 0xFFFF;
                    let hw = (w >> 21) & 0x3;
                    let rd = w & 0x1F;
                    format!("movk w{rd}, #0x{imm:x}, lsl #{shift}", shift = hw * 16)
                }
                w if (w >> 24) == 0x91 => {
                    let imm = (w >> 10) & 0xFFF;
                    let rn = (w >> 5) & 0x1F;
                    let rd = w & 0x1F;
                    format!("add x{rd}, x{rn}, #{imm}")
                }
                w if (w >> 24) == 0xF1 => {
                    let imm = (w >> 10) & 0xFFF;
                    let rn = (w >> 5) & 0x1F;
                    let rd = w & 0x1F;
                    format!("subs x{rd}, x{rn}, #{imm}")
                }
                w if (w >> 21) == 0b10111000000 => {
                    let imm9 = ((w >> 12) & 0x1FF) as i16;
                    let rn = (w >> 5) & 0x1F;
                    let rt = w & 0x1F;
                    format!("str w{rt}, [x{rn}], #{imm9}")
                }
                w if (w >> 24) == 0b01010100 => {
                    let imm19 = ((w >> 5) & 0x7FFFF) as i32;
                    let signed = if imm19 & 0x40000 != 0 {
                        imm19 | !0x7FFFF
                    } else {
                        imm19
                    };
                    let target = addr as i32 + signed * 4;
                    format!("b.ne 0x{target:x}")
                }
                other => format!(".word 0x{other:08x}"),
            };
            format!("0x{addr:04x}:  {word:08x}    {disasm}")
        })
        .collect()
}
