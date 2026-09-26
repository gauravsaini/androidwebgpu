//! Bare-metal `pathn-sh` shell guest, assembled by the in-crate mini-assembler.
//!
//! Memory layout: code at [`crate::platform::GUEST_LOAD_ADDR`], data section
//! at [`crate::platform::DATA_BASE`] (one page above the load address; the
//! image blob zero-pads code up to it).
//!
//! Register convention (bare metal, no ABI):
//! - `x10`: data-section base. Set once at entry; subroutines never clobber.
//! - `x11`: line-buffer write cursor; `x12`: line length.
//! - `x13`: first-word buffer write cursor; `x14`: first-word length.
//! - `x15`: `space_seen` flag (0/1).
//! - `x0`: byte value / string pointer for calls; `x1`, `x2`: scratch.
//!
//! The assembler's instruction set has no CMP/SUB, so byte equality uses the
//! trick proven in `asm::image_asm_eq_trick_exhaustive`: for a value `v` in
//! `[0, 255]` and a static constant `c`, `v + (256 - c)` is in `[1, 511]` and
//! `(v + (256 - c)) << 56` is zero (mod 2^64) iff the sum is 256 iff `v == c`.
//! [`Asm::eq_byte`] emits exactly that sequence.

use crate::asm::*;
use crate::platform::{DATA_BASE, GUEST_LOAD_ADDR};
use std::collections::HashMap;

/// Data-section offsets from `DATA_BASE`.
pub const D_LINE: u64 = 0x000; // line buffer, 256 bytes
pub const D_WORD: u64 = 0x100; // first-word buffer, 256 bytes
pub const D_PROMPT: u64 = 0x200; // "pathn-sh> \0"
pub const D_HELP: u64 = 0x210; // help text
pub const D_UNK: u64 = 0x230; // "unknown cmd: \0"
pub const D_NL: u64 = 0x240; // "\n\0"
pub const D_LONG: u64 = 0x250; // "line too long\n\0"
pub const DATA_LEN: usize = 0x260;

/// Assembled guest: little-endian machine-code words + data-section bytes.
pub struct ShellImage {
    pub code: Vec<u32>,
    pub data: Vec<u8>,
}

#[derive(Clone)]
enum FixupKind {
    B,
    Bl,
    Cbz(u8),
    Cbnz(u8),
    Adrp(u8, u64),     // rd, data offset
    AdrpLo12(u8, u64), // rd, data offset
}

struct Fixup {
    pos: usize,
    kind: FixupKind,
    label: String,
}

/// Two-pass label assembler over the [`crate::asm`] encoders.
pub struct Asm {
    code: Vec<u32>,
    labels: HashMap<String, usize>,
    fixups: Vec<Fixup>,
}

impl Default for Asm {
    fn default() -> Self {
        Self::new()
    }
}

impl Asm {
    pub fn new() -> Self {
        Asm {
            code: Vec::new(),
            labels: HashMap::new(),
            fixups: Vec::new(),
        }
    }

    pub fn label(&mut self, name: &str) {
        assert!(
            self.labels
                .insert(name.to_string(), self.code.len())
                .is_none(),
            "duplicate label {name}"
        );
    }

    pub fn emit(&mut self, word: u32) {
        self.code.push(word);
    }

    fn jump(&mut self, kind: FixupKind, label: &str) {
        let pos = self.code.len();
        self.code.push(0); // patched in finish()
        self.fixups.push(Fixup {
            pos,
            kind,
            label: label.to_string(),
        });
    }

    pub fn b(&mut self, label: &str) {
        self.jump(FixupKind::B, label);
    }
    pub fn bl(&mut self, label: &str) {
        self.jump(FixupKind::Bl, label);
    }
    pub fn cbz(&mut self, rt: u8, label: &str) {
        self.jump(FixupKind::Cbz(rt), label);
    }
    pub fn cbnz(&mut self, rt: u8, label: &str) {
        self.jump(FixupKind::Cbnz(rt), label);
    }

    /// `ADRP Rd, <data symbol>` + `ADD Rd, Rd, #lo12`: Rd = DATA_BASE + off.
    pub fn adrp_data(&mut self, rd: u8, data_off: u64) {
        self.jump(FixupKind::Adrp(rd, data_off), "__data__");
        self.jump(FixupKind::AdrpLo12(rd, data_off), "__data__");
    }

    /// Branch to `label` iff the low byte of `rn` equals `c`.
    /// Precondition: `rn` (full 64 bits) holds a value in `[0, 255]`.
    /// Clobbers `x1`, `x2`.
    pub fn eq_byte(&mut self, rn: u8, c: u8, label: &str) {
        let k = 256u16 - u16::from(c); // [1, 256]
        self.emit(enc_add_imm(1, rn, k, false));
        self.emit(enc_orr_shift(2, 31, 1, 0, 56));
        self.cbz(2, label);
    }

    /// Branch to `label` iff `rn == k`. Precondition: both in `[0, 255]`.
    /// Clobbers `x1`, `x2`.
    pub fn eq_count(&mut self, rn: u8, k: u8, label: &str) {
        self.eq_byte(rn, k, label);
    }

    /// Resolve all fixups against the load address. Panics on undefined labels.
    pub fn finish(mut self) -> Vec<u32> {
        // Pseudo-label for data symbols (address computed from DATA_BASE).
        self.labels.insert("__data__".to_string(), usize::MAX);
        for f in &self.fixups {
            let pc = GUEST_LOAD_ADDR + (f.pos as u64) * 4;
            let word = match &f.kind {
                FixupKind::B | FixupKind::Bl | FixupKind::Cbz(_) | FixupKind::Cbnz(_) => {
                    let tgt_pos = *self
                        .labels
                        .get(&f.label)
                        .unwrap_or_else(|| panic!("undefined label {}", f.label));
                    assert!(
                        tgt_pos != usize::MAX,
                        "data pseudo-label used as branch target"
                    );
                    let target = GUEST_LOAD_ADDR + (tgt_pos as u64) * 4;
                    let off = ((target as i64 - pc as i64) / 4) as i32;
                    match &f.kind {
                        FixupKind::B => enc_b(off),
                        FixupKind::Bl => enc_bl(off),
                        FixupKind::Cbz(rt) => enc_cbz(*rt, off),
                        FixupKind::Cbnz(rt) => enc_cbnz(*rt, off),
                        _ => unreachable!(),
                    }
                }
                FixupKind::Adrp(rd, data_off) => {
                    let target = DATA_BASE + data_off;
                    let page_off = (target >> 12) as i64 - (pc >> 12) as i64;
                    assert!(
                        ((-1 << 20)..(1 << 20)).contains(&page_off),
                        "ADRP out of range"
                    );
                    enc_adrp(*rd, page_off as i32)
                }
                FixupKind::AdrpLo12(rd, data_off) => {
                    let lo12 = ((DATA_BASE + data_off) & 0xFFF) as u16;
                    enc_add_imm(*rd, *rd, lo12, false)
                }
            };
            self.code[f.pos] = word;
        }
        self.code
    }
}

/// Build the data section: buffers (zeroed) + NUL-terminated strings.
pub fn build_data() -> Vec<u8> {
    let mut d = vec![0u8; DATA_LEN];
    let mut put = |off: u64, s: &[u8]| {
        let o = off as usize;
        d[o..o + s.len()].copy_from_slice(s);
    };
    put(D_PROMPT, b"pathn-sh> \0");
    put(D_HELP, b"commands: echo <args> | help\n\0");
    put(D_UNK, b"unknown cmd: \0");
    put(D_NL, b"\n\0");
    put(D_LONG, b"line too long\n\0");
    d
}

/// Assemble the full `pathn-sh` guest. See the module docs for the algorithm.
pub fn assemble_shell() -> ShellImage {
    let mut a = Asm::new();

    // ---- entry: x10 = data base ----
    a.label("entry");
    a.adrp_data(10, 0);
    a.b("prompt");

    // ---- print_cstr: x0 = NUL-terminated string; clobbers x0,x1,x2 ----
    a.label("print_cstr");
    a.emit(enc_movz(1, 0x900, 1)); // x1 = CONSOLE_TX
    a.label("pcs_loop");
    a.emit(enc_ldrb(2, 0, 0)); // w2 = [x0]
    a.emit(enc_add_imm(0, 0, 1, false)); // x0++
    a.cbz(2, "pcs_done");
    a.emit(enc_strb(2, 1, 0)); // TX = w2
    a.b("pcs_loop");
    a.label("pcs_done");
    a.emit(enc_ret(30));

    // ---- print_char: w0 = char; preserves w0; clobbers x1 ----
    a.label("print_char");
    a.emit(enc_movz(1, 0x900, 1));
    a.emit(enc_strb(0, 1, 0));
    a.emit(enc_ret(30));

    // ---- read_char -> w0 (0 = no byte); clobbers x1 ----
    a.label("read_char");
    a.emit(enc_movz(1, 0x900, 1)); // x1 = CONSOLE_TX
    a.emit(enc_add_imm(1, 1, 8, false)); // x1 = CONSOLE_RX
    a.emit(enc_ldrb(0, 1, 0));
    a.emit(enc_ret(30));

    // ---- prompt ----
    a.label("prompt");
    a.adrp_data(0, D_PROMPT);
    a.bl("print_cstr");
    a.emit(enc_orr_shift(11, 31, 10, 0, 0)); // x11 = line cursor = x10
    a.adrp_data(13, D_WORD); // x13 = word cursor
    a.emit(enc_movz(12, 0, 0)); // x12 = len = 0
    a.emit(enc_movz(14, 0, 0)); // x14 = wordlen = 0
    a.emit(enc_movz(15, 0, 0)); // x15 = space_seen = 0

    // ---- read_loop ----
    a.label("read_loop");
    a.bl("read_char"); // w0 = byte
    a.cbnz(0, "got_byte");
    a.emit(enc_wfi());
    a.b("read_loop");

    // ---- got_byte ----
    a.label("got_byte");
    a.eq_count(12, 255, "too_long"); // line full?
    a.bl("print_char"); // echo (preserves w0)
    a.eq_byte(0, 10, "eol"); // '\n'
    a.eq_byte(0, 13, "eol"); // '\r'
    a.emit(enc_strb(0, 11, 0)); // line[x12] = w0
    a.emit(enc_add_imm(11, 11, 1, false)); // x11++
    a.emit(enc_add_imm(12, 12, 1, false)); // x12++
    a.cbnz(15, "read_loop"); // space already seen?
    a.eq_byte(0, 32, "mark_space"); // ' '
    a.emit(enc_strb(0, 13, 0)); // word[x14] = w0
    a.emit(enc_add_imm(13, 13, 1, false)); // x13++
    a.emit(enc_add_imm(14, 14, 1, false)); // x14++
    a.b("read_loop");
    a.label("mark_space");
    a.emit(enc_movz(15, 1, 0)); // space_seen = 1
    a.b("read_loop");

    // ---- too_long ----
    a.label("too_long");
    a.adrp_data(0, D_NL);
    a.bl("print_cstr");
    a.adrp_data(0, D_LONG);
    a.bl("print_cstr");
    a.b("prompt");

    // ---- eol ----
    a.label("eol");
    a.emit(enc_strb(31, 11, 0)); // NUL-terminate line
    a.emit(enc_strb(31, 13, 0)); // NUL-terminate word
    a.bl("process_line");
    a.b("prompt");

    // ---- process_line ----
    a.label("process_line");
    a.cbz(12, "pl_ret"); // empty line
    a.cbz(14, "pl_ret"); // whitespace-only line
    a.eq_count(14, 4, "m_echo"); // wordlen == 4?
    a.b("try_help");
    // match "echo"
    a.label("m_echo");
    a.emit(enc_ldrb(1, 10, (D_WORD) as u16));
    a.eq_byte(1, b'e', "e_e1");
    a.b("try_help");
    a.label("e_e1");
    a.emit(enc_ldrb(1, 10, (D_WORD + 1) as u16));
    a.eq_byte(1, b'c', "e_e2");
    a.b("try_help");
    a.label("e_e2");
    a.emit(enc_ldrb(1, 10, (D_WORD + 2) as u16));
    a.eq_byte(1, b'h', "e_e3");
    a.b("try_help");
    a.label("e_e3");
    a.emit(enc_ldrb(1, 10, (D_WORD + 3) as u16));
    a.eq_byte(1, b'o', "do_echo");
    a.b("try_help");
    // match "help"
    a.label("try_help");
    a.eq_count(14, 4, "m_help");
    a.b("unknown");
    a.label("m_help");
    a.emit(enc_ldrb(1, 10, D_WORD as u16));
    a.eq_byte(1, b'h', "h_e1");
    a.b("unknown");
    a.label("h_e1");
    a.emit(enc_ldrb(1, 10, (D_WORD + 1) as u16));
    a.eq_byte(1, b'e', "h_e2");
    a.b("unknown");
    a.label("h_e2");
    a.emit(enc_ldrb(1, 10, (D_WORD + 2) as u16));
    a.eq_byte(1, b'l', "h_e3");
    a.b("unknown");
    a.label("h_e3");
    a.emit(enc_ldrb(1, 10, (D_WORD + 3) as u16));
    a.eq_byte(1, b'p', "do_help");
    a.b("unknown");
    // unknown command
    a.label("unknown");
    a.adrp_data(0, D_UNK);
    a.bl("print_cstr");
    a.adrp_data(0, D_WORD);
    a.bl("print_cstr");
    a.adrp_data(0, D_NL);
    a.bl("print_cstr");
    a.b("pl_ret");
    // echo builtin: print args (line+5, NUL-terminated) + newline
    a.label("do_echo");
    a.emit(enc_orr_shift(0, 31, 10, 0, 0)); // x0 = x10
    a.emit(enc_add_imm(0, 0, 5, false)); // x0 += 5
    a.bl("print_cstr");
    a.adrp_data(0, D_NL);
    a.bl("print_cstr");
    a.b("pl_ret");
    // help builtin
    a.label("do_help");
    a.adrp_data(0, D_HELP);
    a.bl("print_cstr");
    a.b("pl_ret");
    a.label("pl_ret");
    a.emit(enc_ret(30));

    let code = a.finish();
    ShellImage {
        code,
        data: build_data(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::DATA_OFFSET;

    #[test]
    fn image_guest_code_fits_below_data() {
        let img = assemble_shell();
        let bytes = img.code.len() * 4;
        assert!(
            bytes < DATA_OFFSET as usize - 64,
            "code {bytes} bytes overflows the reserved page"
        );
        assert!(!img.code.is_empty());
    }

    #[test]
    fn image_guest_entry_is_first_word() {
        // Entry word must be the ADRP x10 instruction (0xB000000A pattern
        // family: ADRP with a small positive page offset).
        let img = assemble_shell();
        assert_eq!(img.code[0] & 0x9F00_001F, 0x9000_000A & 0x9F00_001F);
        assert_eq!(img.code[0] >> 31, 1); // ADRP, not ADR
    }

    #[test]
    fn image_guest_prompt_present() {
        let img = assemble_shell();
        let d = &img.data;
        let want = b"pathn-sh> \0";
        assert!(
            d.windows(want.len()).any(|w| w == want),
            "prompt missing from data section"
        );
        let help = b"commands: echo <args> | help\n\0";
        assert!(
            d.windows(help.len()).any(|w| w == help),
            "help text missing"
        );
    }

    #[test]
    fn image_guest_data_offsets_consistent() {
        let d = build_data();
        assert_eq!(d.len(), DATA_LEN);
        assert_eq!(
            &d[D_PROMPT as usize..D_PROMPT as usize + 11],
            b"pathn-sh> \0"
        );
        // Buffers start zeroed.
        assert!(d[0..0x100].iter().all(|b| *b == 0));
        assert!(d[0x100..0x200].iter().all(|b| *b == 0));
    }

    #[test]
    fn image_guest_determinism() {
        let a = assemble_shell();
        let b = assemble_shell();
        assert_eq!(a.code, b.code);
        assert_eq!(a.data, b.data);
    }
}
