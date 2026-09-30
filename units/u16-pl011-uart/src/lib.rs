//! U16 `pl011-uart` — ARM PrimeCell PL011 UART device model (Phase-5 prep).
//!
//! Purity: EXPLICIT-STATE. All device state lives in [`Pl011`]; the model
//! performs no I/O, spawns no threads, and reads no clocks — guest-visible
//! behavior is fully deterministic from the access sequence.
//!
//! # Model
//! - A 16-deep TX window mirrors the PL011's 16-entry transmit FIFO purely
//!   for `FR` flag fidelity (`TXFE` set when empty, `TXFF` when full). Every
//!   byte written to `DR` while `TXE` is set in `CR` is captured into the
//!   output buffer *instantly* (transmit = instant push), so `BUSY` is
//!   asserted during the `write` call and cleared before it returns — never
//!   observable on a later `FR` read. On window overflow the oldest entry is
//!   evicted; output capture never loses bytes.
//! - No RX path exists (no injection source in this model): `RXFE` is always
//!   set, `RXFF` never sets, and `DR` reads return 0.
//! - Interrupts: `TXRIS` (`RIS` bit 5) is set on every completed `DR`
//!   transmit; `MIS = RIS & IMSC`; `ICR` is write-1-to-clear against `RIS`.
//! - Out-of-range offsets: writes are ignored, reads return 0.
//!
//! # Owned constants
//! [`PL011_BASE`] (`0x09000000`) is the QEMU `virt` machine's default UART
//! address, owned here so Phase-5 integration can relocate it in one place.
//!
//! # Intended integration point (Phase 5)
//! The orchestrator (`units/u12-orchestrator`) MMIO dispatch will map the
//! guest-physical window `[PL011_BASE, PL011_BASE + 0x1000)` onto a `Pl011`
//! instance, routing each access to `read(offset)` / `write(offset, val)`
//! with `offset = pa - PL011_BASE`, and will expose `take_output()` as the
//! guest console sink for earlycon/kernel printk output. This unit is
//! deliberately not wired in yet — device model only.

use std::collections::VecDeque;

/// Base address of the PL011 UART on the QEMU `virt` machine.
///
/// Owned by this unit; relocate the device by changing this single constant.
pub const PL011_BASE: u64 = 0x0900_0000;

/// Size in bytes of the MMIO window this model decodes.
pub const PL011_SIZE: u64 = 0x1000;

/// Depth of the modeled transmit FIFO (matches the PL011 hardware FIFO).
pub const TX_FIFO_DEPTH: usize = 16;

// ---- Register offsets (bytes from base) ----
/// Data register (reads return 0: no RX path in this model).
pub const UART_DR: u64 = 0x000;
/// Receive status (read) / error clear (write); always 0 here.
pub const UART_RSR: u64 = 0x004;
/// Flag register (read-only).
pub const UART_FR: u64 = 0x018;
/// IrDA low-power counter (stored, unused).
pub const UART_ILPR: u64 = 0x020;
/// Integer baud-rate divisor.
pub const UART_IBRD: u64 = 0x024;
/// Fractional baud-rate divisor.
pub const UART_FBRD: u64 = 0x028;
/// Line control.
pub const UART_LCR_H: u64 = 0x02C;
/// Control register.
pub const UART_CR: u64 = 0x030;
/// Interrupt FIFO level select.
pub const UART_IFLS: u64 = 0x034;
/// Interrupt mask.
pub const UART_IMSC: u64 = 0x038;
/// Raw interrupt status.
pub const UART_RIS: u64 = 0x03C;
/// Masked interrupt status.
pub const UART_MIS: u64 = 0x040;
/// Interrupt clear (write-1-to-clear).
pub const UART_ICR: u64 = 0x044;
/// DMA control.
pub const UART_DMACR: u64 = 0x048;
/// PrimeCell ID registers.
pub const UART_PID0: u64 = 0xFE0;
pub const UART_PID1: u64 = 0xFE4;
pub const UART_PID2: u64 = 0xFE8;
pub const UART_PID3: u64 = 0xFEC;
/// Component ID registers.
pub const UART_CID0: u64 = 0xFF0;
pub const UART_CID1: u64 = 0xFF4;
pub const UART_CID2: u64 = 0xFF8;
pub const UART_CID3: u64 = 0xFFC;

// ---- FR flag bits ----
// Bit 6 (RXFF) never sets: no RX path. Bit 3 (BUSY) is never observable:
// transmit is an instant push, so it clears before `write` returns. Bits 2:0
// (modem status) are 0 in this model.
const FR_TXFE: u32 = 1 << 7;
const FR_TXFF: u32 = 1 << 5;
const FR_RXFE: u32 = 1 << 4;

// ---- CR bits ----
/// Transmit enable.
const CR_TXE: u32 = 1 << 8;

// ---- RIS bits ----
/// Transmit interrupt: set on every completed `DR` transmit.
pub const RIS_TXRIS: u32 = 1 << 5;

// ---- Reset values ----
const CR_RESET: u32 = 0x300; // TXE | RXE
const IFLS_RESET: u32 = 0x12; // TX/RX trigger at 1/2-full (PL011 reset value)

// ---- PrimeCell ID values (PL011) ----
const PID0_VAL: u32 = 0x11;
const PID1_VAL: u32 = 0x10;
const PID2_VAL: u32 = 0x14;
const PID3_VAL: u32 = 0x00;
const CID0_VAL: u32 = 0x0D;
const CID1_VAL: u32 = 0xF0;
const CID2_VAL: u32 = 0x05;
const CID3_VAL: u32 = 0xB1;

/// ARM PrimeCell PL011 UART device model.
///
/// Construct with [`Pl011::new`], drive with [`Pl011::read`] /
/// [`Pl011::write`], drain guest console bytes with [`Pl011::take_output`].
pub struct Pl011 {
    /// 16-deep TX window driving `FR.TXFE`/`FR.TXFF` (see module docs).
    tx_window: VecDeque<u8>,
    /// Captured guest console output; drained by the integrator.
    output: Vec<u8>,
    ilpr: u32,
    ibrd: u32,
    fbrd: u32,
    lcr_h: u32,
    cr: u32,
    ifls: u32,
    imsc: u32,
    ris: u32,
    dmacr: u32,
}

impl Pl011 {
    /// Fresh device in its power-on reset state.
    pub fn new() -> Self {
        Self {
            tx_window: VecDeque::with_capacity(TX_FIFO_DEPTH),
            output: Vec::new(),
            ilpr: 0,
            ibrd: 0,
            fbrd: 0,
            lcr_h: 0,
            cr: CR_RESET,
            ifls: IFLS_RESET,
            imsc: 0,
            ris: 0,
            dmacr: 0,
        }
    }

    /// MMIO read at `offset` bytes from the device base.
    ///
    /// Out-of-range offsets return 0.
    pub fn read(&self, offset: u64) -> u32 {
        match offset {
            UART_DR => 0,  // no RX path: no data ever present
            UART_RSR => 0, // no error injection in this model
            UART_FR => self.flag_register(),
            UART_ILPR => self.ilpr,
            UART_IBRD => self.ibrd,
            UART_FBRD => self.fbrd,
            UART_LCR_H => self.lcr_h,
            UART_CR => self.cr,
            UART_IFLS => self.ifls,
            UART_IMSC => self.imsc,
            UART_RIS => self.ris,
            UART_MIS => self.ris & self.imsc,
            UART_ICR => 0, // write-only
            UART_DMACR => self.dmacr,
            UART_PID0 => PID0_VAL,
            UART_PID1 => PID1_VAL,
            UART_PID2 => PID2_VAL,
            UART_PID3 => PID3_VAL,
            UART_CID0 => CID0_VAL,
            UART_CID1 => CID1_VAL,
            UART_CID2 => CID2_VAL,
            UART_CID3 => CID3_VAL,
            _ => 0,
        }
    }

    /// MMIO write of `val` at `offset` bytes from the device base.
    ///
    /// Out-of-range offsets are ignored.
    pub fn write(&mut self, offset: u64, val: u32) {
        match offset {
            UART_DR => self.transmit_data(val),
            UART_RSR => {} // ECR: write clears RSR; RSR is always 0 here
            UART_FR | UART_RIS | UART_MIS => {} // read-only
            UART_ILPR => self.ilpr = val,
            UART_IBRD => self.ibrd = val,
            UART_FBRD => self.fbrd = val,
            UART_LCR_H => self.lcr_h = val,
            UART_CR => self.cr = val,
            UART_IFLS => self.ifls = val,
            UART_IMSC => self.imsc = val,
            UART_ICR => self.ris &= !val, // write-1-to-clear
            UART_DMACR => self.dmacr = val,
            UART_PID0 | UART_PID1 | UART_PID2 | UART_PID3 | UART_CID0 | UART_CID1
            | UART_CID2 | UART_CID3 => {} // read-only
            _ => {} // out of range: ignored
        }
    }

    /// Guest console bytes captured so far (borrowed; use [`Pl011::take_output`]
    /// to drain).
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Drain and return all captured guest console bytes.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    fn flag_register(&self) -> u32 {
        let mut fr = 0u32;
        if self.tx_window.is_empty() {
            fr |= FR_TXFE;
        }
        if self.tx_window.len() >= TX_FIFO_DEPTH {
            fr |= FR_TXFF;
        }
        // RXFE always set: no RX injection source in this model.
        fr | FR_RXFE
    }

    fn transmit_data(&mut self, val: u32) {
        if self.cr & CR_TXE == 0 {
            return; // transmitter disabled: byte dropped, no interrupt
        }
        let byte = (val & 0xFF) as u8;
        // BUSY would assert here on real hardware; transmit is an instant
        // push in this model, so it clears before `write` returns.
        if self.tx_window.len() >= TX_FIFO_DEPTH {
            self.tx_window.pop_front(); // window eviction; output unaffected
        }
        self.tx_window.push_back(byte);
        self.output.push(byte);
        self.ris |= RIS_TXRIS;
    }
}

impl Default for Pl011 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_defaults() {
        let u = Pl011::new();
        assert_eq!(u.read(UART_FR), 0x90, "FR = TXFE|RXFE after reset");
        assert_eq!(u.read(UART_CR), 0x300, "CR = TXE|RXE after reset");
        assert_eq!(u.read(UART_IMSC), 0);
        assert_eq!(u.read(UART_RIS), 0);
        assert_eq!(u.read(UART_MIS), 0);
        assert_eq!(u.read(UART_IBRD), 0);
        assert_eq!(u.read(UART_FBRD), 0);
        assert_eq!(u.read(UART_LCR_H), 0);
        assert_eq!(u.read(UART_IFLS), 0x12);
        assert_eq!(u.read(UART_DMACR), 0);
        assert!(u.output().is_empty());
    }

    #[test]
    fn tx_hello_captured() {
        let mut u = Pl011::new();
        for b in b"hello" {
            u.write(UART_DR, *b as u32);
        }
        assert_eq!(u.output(), b"hello");
        assert_eq!(u.take_output(), b"hello");
        assert!(u.output().is_empty(), "take_output drains the buffer");
        // Only the low byte of the written value is transmitted.
        u.write(UART_DR, 0x1_41);
        assert_eq!(u.output(), b"A");
    }

    #[test]
    fn fr_txff_after_sixteen_bytes() {
        let mut u = Pl011::new();
        assert_eq!(u.read(UART_FR) & FR_TXFE, FR_TXFE, "TXFE set when empty");
        for i in 0..16u32 {
            u.write(UART_DR, i);
        }
        let fr = u.read(UART_FR);
        assert_eq!(fr & FR_TXFF, FR_TXFF, "TXFF set at 16-deep window");
        assert_eq!(fr & FR_TXFE, 0, "TXFE clear once bytes transmitted");
        // 17th byte: window evicts oldest, output captures everything.
        u.write(UART_DR, 0xFF);
        assert_eq!(u.read(UART_FR) & FR_TXFF, FR_TXFF);
        assert_eq!(u.output().len(), 17, "no output bytes lost on overflow");
        assert_eq!(u.output()[16], 0xFF);
    }

    #[test]
    fn fr_rx_flags_constant() {
        let mut u = Pl011::new();
        assert_eq!(u.read(UART_FR) & FR_RXFE, FR_RXFE, "RXFE always set");
        u.write(UART_DR, b'x' as u32);
        let fr = u.read(UART_FR);
        assert_eq!(fr & FR_RXFE, FR_RXFE, "RXFE still set after TX");
        assert_eq!(fr & 0x08, 0, "BUSY never observable (instant transmit)");
    }

    #[test]
    fn tx_disabled_drops_bytes() {
        let mut u = Pl011::new();
        u.write(UART_CR, 0x200); // RXE only: TXE cleared
        u.write(UART_DR, b'z' as u32);
        assert!(u.output().is_empty(), "DR write with TXE clear is dropped");
        assert_eq!(u.read(UART_RIS) & RIS_TXRIS, 0, "no TXRIS when dropped");
        // Re-enable and confirm transmission resumes.
        u.write(UART_CR, 0x300);
        u.write(UART_DR, b'z' as u32);
        assert_eq!(u.output(), b"z");
    }

    #[test]
    fn config_registers_round_trip() {
        let mut u = Pl011::new();
        let regs = [
            (UART_ILPR, 0x00AB),
            (UART_IBRD, 0x001B),
            (UART_FBRD, 0x0022),
            (UART_LCR_H, 0x0070),
            (UART_CR, 0x0301),
            (UART_IFLS, 0x0036),
            (UART_IMSC, 0x07F0),
            (UART_DMACR, 0x0003),
        ];
        for (off, val) in regs {
            u.write(off, val);
        }
        for (off, val) in regs {
            assert_eq!(u.read(off), val, "round-trip failed at {:#x}", off);
        }
    }

    #[test]
    fn id_registers() {
        let u = Pl011::new();
        let expected = [
            (UART_PID0, 0x11),
            (UART_PID1, 0x10),
            (UART_PID2, 0x14),
            (UART_PID3, 0x00),
            (UART_CID0, 0x0D),
            (UART_CID1, 0xF0),
            (UART_CID2, 0x05),
            (UART_CID3, 0xB1),
        ];
        for (off, val) in expected {
            assert_eq!(u.read(off), val, "ID register {:#x}", off);
        }
        // ID registers are read-only.
        let mut u = u;
        u.write(UART_PID0, 0xFF);
        assert_eq!(u.read(UART_PID0), 0x11, "PID0 write ignored");
    }

    #[test]
    fn icr_clears_interrupts() {
        let mut u = Pl011::new();
        u.write(UART_IMSC, RIS_TXRIS);
        u.write(UART_DR, b'q' as u32);
        assert_eq!(u.read(UART_RIS) & RIS_TXRIS, RIS_TXRIS, "TXRIS set");
        assert_eq!(
            u.read(UART_MIS) & RIS_TXRIS,
            RIS_TXRIS,
            "MIS = RIS & IMSC"
        );
        u.write(UART_ICR, RIS_TXRIS);
        assert_eq!(u.read(UART_RIS), 0, "ICR write-1-to-clear on RIS");
        assert_eq!(u.read(UART_MIS), 0, "MIS follows RIS");
    }

    #[test]
    fn mis_masked_when_imsc_clear() {
        let mut u = Pl011::new();
        u.write(UART_DR, b'q' as u32);
        assert_eq!(u.read(UART_RIS) & RIS_TXRIS, RIS_TXRIS);
        assert_eq!(u.read(UART_MIS), 0, "MIS masked with IMSC=0");
    }

    #[test]
    fn dr_and_rsr_reads() {
        let u = Pl011::new();
        assert_eq!(u.read(UART_DR), 0, "no RX data in this model");
        assert_eq!(u.read(UART_RSR), 0, "no error injection");
        assert_eq!(u.read(UART_ICR), 0, "ICR is write-only");
    }

    #[test]
    fn out_of_range_ignored() {
        let mut u = Pl011::new();
        u.write(0x1000, 0xDEAD_BEEF);
        u.write(0xFFFF_FFFF, 0x1234);
        assert_eq!(u.read(0x1000), 0);
        assert_eq!(u.read(0xFFFF_FFFF), 0);
        assert_eq!(u.read(UART_FR), 0x90, "state undisturbed");
        assert!(u.output().is_empty());
    }

    #[test]
    fn base_and_size_consts() {
        assert_eq!(PL011_BASE, 0x0900_0000, "QEMU virt default UART base");
        assert_eq!(PL011_SIZE, 0x1000);
    }
}
