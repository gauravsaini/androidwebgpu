//! ARM Generic Interrupt Controller v2 (GICv2) Device Model.
//!
//! Purity: EXPLICIT-STATE. All state lives inside [`Gic`].
//! Performs no I/O, no threading, no time reads — fully deterministic.
//!
//! # Architecture Reference
//! - ARM Generic Interrupt Controller Architecture Specification v2.0 (ARM IHI 0048B)
//! - Distributor (GICD) base: `0x0800_0000`, size: `0x10000` (QEMU virt default)
//! - CPU Interface (GICC) base: `0x0801_0000`, size: `0x10000`
//!
//! # Supported Interrupt Ranges
//! - INTID 0..15: Software Generated Interrupts (SGIs)
//! - INTID 16..31: Private Peripheral Interrupts (PPIs)
//!   - PPI 27: Virtual Timer (`CNTV`)
//!   - PPI 30: Physical Timer (`CNTP`)
//! - INTID 32..1019: Shared Peripheral Interrupts (SPIs)
//!   - SPI 33: PL011 UART (SPI 1)
//!   - SPI 48+: Virtio MMIO devices
//! - INTID 1020..1023: Special / Spurious IDs (1023 = no pending interrupt)

pub const GICD_BASE: u64 = 0x0800_0000;
pub const GICD_SIZE: u64 = 0x10000;
pub const GICC_BASE: u64 = 0x0801_0000;
pub const GICC_SIZE: u64 = 0x10000;

pub const MAX_IRQS: usize = 128; // Standard 128 IRQs (4 x 32-bit banks)
pub const SPURIOUS_IRQ: u32 = 1023;

// ---- Distributor Register Offsets ----
pub const GICD_CTLR: u32 = 0x000;
pub const GICD_TYPER: u32 = 0x004;
pub const GICD_IIDR: u32 = 0x008;
pub const GICD_IGROUPR_START: u32 = 0x080;
pub const GICD_IGROUPR_END: u32 = 0x0FC;
pub const GICD_ISENABLER_START: u32 = 0x100;
pub const GICD_ISENABLER_END: u32 = 0x17C;
pub const GICD_ICENABLER_START: u32 = 0x180;
pub const GICD_ICENABLER_END: u32 = 0x1FC;
pub const GICD_ISPENDR_START: u32 = 0x200;
pub const GICD_ISPENDR_END: u32 = 0x27C;
pub const GICD_ICPENDR_START: u32 = 0x280;
pub const GICD_ICPENDR_END: u32 = 0x2FC;
pub const GICD_ISACTIVER_START: u32 = 0x300;
pub const GICD_ISACTIVER_END: u32 = 0x37C;
pub const GICD_ICACTIVER_START: u32 = 0x380;
pub const GICD_ICACTIVER_END: u32 = 0x3FC;
pub const GICD_IPRIORITYR_START: u32 = 0x400;
pub const GICD_IPRIORITYR_END: u32 = 0x7F8;
pub const GICD_ITARGETSR_START: u32 = 0x800;
pub const GICD_ITARGETSR_END: u32 = 0xBF8;
pub const GICD_ICFGR_START: u32 = 0xC00;
pub const GICD_ICFGR_END: u32 = 0xCFC;
pub const GICD_SGIR: u32 = 0xF00;

// ---- CPU Interface Register Offsets ----
pub const GICC_CTLR: u32 = 0x0000;
pub const GICC_PMR: u32 = 0x0004;
pub const GICC_BPR: u32 = 0x0008;
pub const GICC_IAR: u32 = 0x000C;
pub const GICC_EOIR: u32 = 0x0010;
pub const GICC_RPR: u32 = 0x0014;
pub const GICC_HPPIR: u32 = 0x0018;
pub const GICC_DIR: u32 = 0x1000;

/// Trigger configuration: level-sensitive vs edge-triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Level = 0,
    Edge = 1,
}

/// Explicit state of the ARM GICv2 interrupt controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gic {
    // ---- Distributor Registers ----
    pub d_ctlr: u32,
    pub group: [u32; MAX_IRQS / 32],
    pub enabled: [u32; MAX_IRQS / 32],
    pub pending: [u32; MAX_IRQS / 32],
    pub active: [u32; MAX_IRQS / 32],
    pub priority: [u8; MAX_IRQS],
    pub target: [u8; MAX_IRQS],
    pub edge_triggered: [u32; MAX_IRQS / 32],
    /// Level line input states from physical/virtual sources
    pub line_level: [u32; MAX_IRQS / 32],

    // ---- CPU Interface Registers ----
    pub c_ctlr: u32,
    pub pmr: u32,
    pub bpr: u32,
    pub running_priority: Vec<u8>,
}

impl Default for Gic {
    fn default() -> Self {
        Self::new()
    }
}

impl Gic {
    /// Creates a newly initialized GICv2 model in reset state.
    pub fn new() -> Self {
        let mut target = [0u8; MAX_IRQS];
        // SGI and PPI targets are CPU 0 (bit 0)
        for t in target.iter_mut().take(32) {
            *t = 0x01;
        }
        // Default SPI targets also point to CPU 0
        for t in target.iter_mut().skip(32) {
            *t = 0x01;
        }

        Self {
            d_ctlr: 0,
            group: [0; MAX_IRQS / 32],
            enabled: [0; MAX_IRQS / 32],
            pending: [0; MAX_IRQS / 32],
            active: [0; MAX_IRQS / 32],
            priority: [0xa0; MAX_IRQS], // default moderate priority (160)
            target,
            edge_triggered: [0; MAX_IRQS / 32], // level-triggered by default
            line_level: [0; MAX_IRQS / 32],

            c_ctlr: 0,
            pmr: 0, // masks everything initially until software configures PMR
            bpr: 2,
            running_priority: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Hardware IRQ Line Interface (for peripherals e.g. Timer, UART, VirtIO)
    // -----------------------------------------------------------------------

    /// Sets or clears an external interrupt line level (level-triggered or edge-triggered).
    pub fn set_irq_level(&mut self, intid: u32, level: bool) {
        if intid as usize >= MAX_IRQS {
            return;
        }
        let bank = (intid / 32) as usize;
        let bit = 1u32 << (intid % 32);
        let was_high = (self.line_level[bank] & bit) != 0;

        if level {
            self.line_level[bank] |= bit;
        } else {
            self.line_level[bank] &= !bit;
        }

        let is_edge = (self.edge_triggered[bank] & bit) != 0;
        if is_edge {
            // Edge-triggered: set pending on rising edge (0 -> 1)
            if !was_high && level {
                self.pending[bank] |= bit;
            }
        } else {
            // Level-sensitive: pending follows the line as long as line is high
            if level {
                self.pending[bank] |= bit;
            } else {
                // If line drops and interrupt is not active, pending clears
                if (self.active[bank] & bit) == 0 {
                    self.pending[bank] &= !bit;
                }
            }
        }
    }

    /// Triggers a Software Generated Interrupt (SGI).
    pub fn trigger_sgi(&mut self, sgi_intid: u32) {
        if sgi_intid < 16 {
            let bit = 1u32 << sgi_intid;
            self.pending[0] |= bit;
        }
    }

    /// Whether an interrupt is currently asserted on the CPU interface.
    pub fn cpu_irq_asserted(&self) -> bool {
        // Distributor and CPU interface must be enabled
        if (self.d_ctlr & 0x01) == 0 || (self.c_ctlr & 0x01) == 0 {
            return false;
        }
        let best_id = self.highest_pending_irq();
        if best_id == SPURIOUS_IRQ {
            return false;
        }

        let prio = self.priority[best_id as usize];
        // Must be higher priority than PMR (lower numerical value)
        if (prio as u32) >= (self.pmr & 0xFF) {
            return false;
        }

        // Must be higher priority than current running priority
        if let Some(&curr_prio) = self.running_priority.last() {
            if prio >= curr_prio {
                return false;
            }
        }

        true
    }

    /// Finds the highest priority pending interrupt (lowest numeric priority value).
    pub fn highest_pending_irq(&self) -> u32 {
        let mut best_id = SPURIOUS_IRQ;
        let mut best_prio = 256u32;

        for bank in 0..(MAX_IRQS / 32) {
            let candidate_mask = self.pending[bank] & self.enabled[bank] & !self.active[bank];
            if candidate_mask == 0 {
                continue;
            }
            for bit in 0..32 {
                if (candidate_mask & (1 << bit)) != 0 {
                    let id = (bank * 32 + bit) as u32;
                    let prio = self.priority[id as usize] as u32;
                    if prio < best_prio {
                        best_prio = prio;
                        best_id = id;
                    }
                }
            }
        }

        best_id
    }

    // -----------------------------------------------------------------------
    // MMIO Access Handlers (Distributor & CPU Interface)
    // -----------------------------------------------------------------------

    /// Read from Distributor MMIO window.
    pub fn dist_read(&self, offset: u32, size: usize) -> u32 {
        let aligned_off = offset & !3;
        let shift = (offset & 3) * 8;
        let val32 = self.dist_read32(aligned_off);

        match size {
            1 => (val32 >> shift) & 0xFF,
            2 => (val32 >> shift) & 0xFFFF,
            4 => val32,
            _ => 0,
        }
    }

    /// Write to Distributor MMIO window.
    pub fn dist_write(&mut self, offset: u32, val: u32, size: usize) {
        match size {
            1 => {
                let aligned_off = offset & !3;
                let shift = (offset & 3) * 8;
                let mut cur = self.dist_read32(aligned_off);
                cur &= !(0xFF << shift);
                cur |= (val & 0xFF) << shift;
                self.dist_write32(aligned_off, cur);
            }
            2 => {
                let aligned_off = offset & !3;
                let shift = (offset & 3) * 8;
                let mut cur = self.dist_read32(aligned_off);
                cur &= !(0xFFFF << shift);
                cur |= (val & 0xFFFF) << shift;
                self.dist_write32(aligned_off, cur);
            }
            4 => {
                self.dist_write32(offset, val);
            }
            _ => {}
        }
    }

    fn dist_read32(&self, offset: u32) -> u32 {
        match offset {
            GICD_CTLR => self.d_ctlr,
            GICD_TYPER => {
                let it_lines = ((MAX_IRQS / 32) - 1) as u32;
                let cpu_num = 0u32; // 1 CPU
                (it_lines & 0x1F) | (cpu_num << 5)
            }
            GICD_IIDR => 0x0200_043B, // ARM GICv2 Implementer ID
            off if (GICD_IGROUPR_START..=GICD_IGROUPR_END).contains(&off) => {
                let idx = ((off - GICD_IGROUPR_START) / 4) as usize;
                if idx < self.group.len() { self.group[idx] } else { 0 }
            }
            off if (GICD_ISENABLER_START..=GICD_ISENABLER_END).contains(&off) => {
                let idx = ((off - GICD_ISENABLER_START) / 4) as usize;
                if idx < self.enabled.len() { self.enabled[idx] } else { 0 }
            }
            off if (GICD_ICENABLER_START..=GICD_ICENABLER_END).contains(&off) => {
                let idx = ((off - GICD_ICENABLER_START) / 4) as usize;
                if idx < self.enabled.len() { self.enabled[idx] } else { 0 }
            }
            off if (GICD_ISPENDR_START..=GICD_ISPENDR_END).contains(&off) => {
                let idx = ((off - GICD_ISPENDR_START) / 4) as usize;
                if idx < self.pending.len() { self.pending[idx] } else { 0 }
            }
            off if (GICD_ICPENDR_START..=GICD_ICPENDR_END).contains(&off) => {
                let idx = ((off - GICD_ICPENDR_START) / 4) as usize;
                if idx < self.pending.len() { self.pending[idx] } else { 0 }
            }
            off if (GICD_ISACTIVER_START..=GICD_ISACTIVER_END).contains(&off) => {
                let idx = ((off - GICD_ISACTIVER_START) / 4) as usize;
                if idx < self.active.len() { self.active[idx] } else { 0 }
            }
            off if (GICD_ICACTIVER_START..=GICD_ICACTIVER_END).contains(&off) => {
                let idx = ((off - GICD_ICACTIVER_START) / 4) as usize;
                if idx < self.active.len() { self.active[idx] } else { 0 }
            }
            off if (GICD_IPRIORITYR_START..=GICD_IPRIORITYR_END).contains(&off) => {
                let irq_start = (off - GICD_IPRIORITYR_START) as usize;
                let mut word = 0u32;
                for i in 0..4 {
                    if irq_start + i < MAX_IRQS {
                        word |= (self.priority[irq_start + i] as u32) << (i * 8);
                    }
                }
                word
            }
            off if (GICD_ITARGETSR_START..=GICD_ITARGETSR_END).contains(&off) => {
                let irq_start = (off - GICD_ITARGETSR_START) as usize;
                let mut word = 0u32;
                for i in 0..4 {
                    if irq_start + i < MAX_IRQS {
                        word |= (self.target[irq_start + i] as u32) << (i * 8);
                    }
                }
                word
            }
            off if (GICD_ICFGR_START..=GICD_ICFGR_END).contains(&off) => {
                let irq_start = ((off - GICD_ICFGR_START) * 4) as usize; // 16 irqs per word, 2 bits each
                let mut word = 0u32;
                for i in 0..16 {
                    let id = irq_start + i;
                    if id < MAX_IRQS {
                        let bank = id / 32;
                        let bit = 1u32 << (id % 32);
                        if (self.edge_triggered[bank] & bit) != 0 {
                            word |= 2u32 << (i * 2); // bit 1 indicates edge
                        }
                    }
                }
                word
            }
            _ => 0,
        }
    }

    fn dist_write32(&mut self, offset: u32, val: u32) {
        match offset {
            GICD_CTLR => {
                self.d_ctlr = val & 0x03;
            }
            off if (GICD_IGROUPR_START..=GICD_IGROUPR_END).contains(&off) => {
                let idx = ((off - GICD_IGROUPR_START) / 4) as usize;
                if idx < self.group.len() {
                    self.group[idx] = val;
                }
            }
            off if (GICD_ISENABLER_START..=GICD_ISENABLER_END).contains(&off) => {
                let idx = ((off - GICD_ISENABLER_START) / 4) as usize;
                if idx < self.enabled.len() {
                    self.enabled[idx] |= val;
                }
            }
            off if (GICD_ICENABLER_START..=GICD_ICENABLER_END).contains(&off) => {
                let idx = ((off - GICD_ICENABLER_START) / 4) as usize;
                if idx < self.enabled.len() {
                    self.enabled[idx] &= !val;
                }
            }
            off if (GICD_ISPENDR_START..=GICD_ISPENDR_END).contains(&off) => {
                let idx = ((off - GICD_ISPENDR_START) / 4) as usize;
                if idx < self.pending.len() {
                    self.pending[idx] |= val;
                }
            }
            off if (GICD_ICPENDR_START..=GICD_ICPENDR_END).contains(&off) => {
                let idx = ((off - GICD_ICPENDR_START) / 4) as usize;
                if idx < self.pending.len() {
                    self.pending[idx] &= !val;
                }
            }
            off if (GICD_ISACTIVER_START..=GICD_ISACTIVER_END).contains(&off) => {
                let idx = ((off - GICD_ISACTIVER_START) / 4) as usize;
                if idx < self.active.len() {
                    self.active[idx] |= val;
                }
            }
            off if (GICD_ICACTIVER_START..=GICD_ICACTIVER_END).contains(&off) => {
                let idx = ((off - GICD_ICACTIVER_START) / 4) as usize;
                if idx < self.active.len() {
                    self.active[idx] &= !val;
                }
            }
            off if (GICD_IPRIORITYR_START..=GICD_IPRIORITYR_END).contains(&off) => {
                let irq_start = (off - GICD_IPRIORITYR_START) as usize;
                for i in 0..4 {
                    if irq_start + i < MAX_IRQS {
                        self.priority[irq_start + i] = ((val >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            off if (GICD_ITARGETSR_START..=GICD_ITARGETSR_END).contains(&off) => {
                let irq_start = (off - GICD_ITARGETSR_START) as usize;
                for i in 0..4 {
                    let id = irq_start + i;
                    if id < MAX_IRQS && id >= 32 {
                        // Targets for 0..31 are read-only (CPU 0)
                        self.target[id] = ((val >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            off if (GICD_ICFGR_START..=GICD_ICFGR_END).contains(&off) => {
                let irq_start = ((off - GICD_ICFGR_START) * 4) as usize;
                for i in 0..16 {
                    let id = irq_start + i;
                    if id < MAX_IRQS {
                        let bit_val = (val >> (i * 2)) & 0x03;
                        let bank = id / 32;
                        let mask = 1u32 << (id % 32);
                        if (bit_val & 0x02) != 0 {
                            self.edge_triggered[bank] |= mask;
                        } else {
                            self.edge_triggered[bank] &= !mask;
                        }
                    }
                }
            }
            GICD_SGIR => {
                let sgi_id = val & 0x0F;
                self.trigger_sgi(sgi_id);
            }
            _ => {}
        }
    }

    /// Read from CPU Interface MMIO window.
    pub fn cpu_read(&mut self, offset: u32, size: usize) -> u32 {
        let val32 = match offset {
            GICC_CTLR => self.c_ctlr,
            GICC_PMR => self.pmr,
            GICC_BPR => self.bpr,
            GICC_IAR => self.acknowledge_irq(),
            GICC_RPR => self.running_priority.last().copied().unwrap_or(0xFF) as u32,
            GICC_HPPIR => self.highest_pending_irq(),
            _ => 0,
        };

        match size {
            1 => val32 & 0xFF,
            2 => val32 & 0xFFFF,
            4 => val32,
            _ => 0,
        }
    }

    /// Write to CPU Interface MMIO window.
    pub fn cpu_write(&mut self, offset: u32, val: u32, _size: usize) {
        match offset {
            GICC_CTLR => {
                self.c_ctlr = val & 0x21F;
            }
            GICC_PMR => {
                self.pmr = val & 0xFF;
            }
            GICC_BPR => {
                self.bpr = val & 0x07;
            }
            GICC_EOIR => {
                let intid = val & 0x3FF;
                self.end_of_interrupt(intid);
            }
            GICC_DIR => {
                let intid = val & 0x3FF;
                self.end_of_interrupt(intid);
            }
            _ => {}
        }
    }

    /// Acknowledges the highest-priority pending interrupt, marking it active.
    pub fn acknowledge_irq(&mut self) -> u32 {
        let intid = self.highest_pending_irq();
        if intid == SPURIOUS_IRQ {
            return SPURIOUS_IRQ;
        }

        let prio = self.priority[intid as usize];
        if (prio as u32) >= (self.pmr & 0xFF) {
            return SPURIOUS_IRQ;
        }

        let bank = (intid / 32) as usize;
        let bit = 1u32 << (intid % 32);

        // Transition from Pending to Active. Pending bit is cleared upon acknowledgement
        self.pending[bank] &= !bit;
        self.active[bank] |= bit;
        self.running_priority.push(prio);

        intid
    }

    /// Ends processing of the interrupt (EOI).
    pub fn end_of_interrupt(&mut self, intid: u32) {
        if intid >= MAX_IRQS as u32 {
            return;
        }
        let bank = (intid / 32) as usize;
        let bit = 1u32 << (intid % 32);

        self.active[bank] &= !bit;

        // If level-triggered, pending status follows the line level after EOI
        let line_high = (self.line_level[bank] & bit) != 0;
        let is_edge = (self.edge_triggered[bank] & bit) != 0;
        if !is_edge {
            if line_high {
                self.pending[bank] |= bit;
            } else {
                self.pending[bank] &= !bit;
            }
        }

        self.running_priority.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gic_init_defaults() {
        let gic = Gic::new();
        assert_eq!(gic.d_ctlr, 0);
        assert_eq!(gic.c_ctlr, 0);
        assert_eq!(gic.pmr, 0);
        assert!(!gic.cpu_irq_asserted());
        assert_eq!(gic.highest_pending_irq(), SPURIOUS_IRQ);
    }

    #[test]
    fn gic_dist_enable_and_status() {
        let mut gic = Gic::new();
        // Enable distributor
        gic.dist_write(GICD_CTLR, 1, 4);
        assert_eq!(gic.dist_read(GICD_CTLR, 4), 1);

        // Check typer
        let typer = gic.dist_read(GICD_TYPER, 4);
        assert_eq!(typer & 0x1F, 3); // 128 IRQs -> 4 banks -> ITLinesNumber = 3

        // Check IIDR
        assert_eq!(gic.dist_read(GICD_IIDR, 4), 0x0200_043B);
    }

    #[test]
    fn gic_enable_disable_irq() {
        let mut gic = Gic::new();
        // Enable IRQ 33 (SPI 1)
        let bank_off = GICD_ISENABLER_START + 4; // bank 1: IRQ 32..63
        gic.dist_write(bank_off, 1 << (33 - 32), 4);
        assert_ne!(gic.dist_read(bank_off, 4) & (1 << (33 - 32)), 0);

        // Disable IRQ 33
        let clear_off = GICD_ICENABLER_START + 4;
        gic.dist_write(clear_off, 1 << (33 - 32), 4);
        assert_eq!(gic.dist_read(bank_off, 4) & (1 << (33 - 32)), 0);
    }

    #[test]
    fn gic_software_generated_interrupt_flow() {
        let mut gic = Gic::new();
        // Enable distributor & CPU interface
        gic.dist_write(GICD_CTLR, 1, 4);
        gic.cpu_write(GICC_CTLR, 1, 4);
        gic.cpu_write(GICC_PMR, 0xFF, 4); // allow all priorities

        // Enable SGI 5
        gic.dist_write(GICD_ISENABLER_START, 1 << 5, 4);
        gic.dist_write(GICD_IPRIORITYR_START + 4, 0x20 << 8, 4); // Priority 0x20 for SGI 5

        // Trigger SGI 5 via SGIR
        gic.dist_write(GICD_SGIR, 5, 4);
        assert!(gic.cpu_irq_asserted(), "CPU IRQ must be asserted after SGI");

        // Acknowledge IRQ
        let ack = gic.cpu_read(GICC_IAR, 4);
        assert_eq!(ack, 5, "IAR must return SGI 5");
        assert!(!gic.cpu_irq_asserted(), "IRQ line deasserted after ACK");

        // End of interrupt
        gic.cpu_write(GICC_EOIR, 5, 4);
        assert_eq!(gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ, "no more IRQs pending");
    }

    #[test]
    fn gic_priority_masking() {
        let mut gic = Gic::new();
        gic.dist_write(GICD_CTLR, 1, 4);
        gic.cpu_write(GICC_CTLR, 1, 4);

        // Set PMR to 0x80
        gic.cpu_write(GICC_PMR, 0x80, 4);

        // Enable SPI 40 with low priority 0x90 (numerical > 0x80 -> masked)
        gic.dist_write(GICD_ISENABLER_START + 4, 1 << (40 - 32), 4);
        gic.dist_write(GICD_IPRIORITYR_START + 40, 0x90, 1);

        // Assert line
        gic.set_irq_level(40, true);
        assert!(!gic.cpu_irq_asserted(), "IRQ 40 should be masked by PMR 0x80");
        assert_eq!(gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);

        // Unmask by raising PMR to 0xA0
        gic.cpu_write(GICC_PMR, 0xA0, 4);
        assert!(gic.cpu_irq_asserted(), "IRQ 40 unmasked when PMR raised");
        assert_eq!(gic.cpu_read(GICC_IAR, 4), 40);
        gic.cpu_write(GICC_EOIR, 40, 4);
    }

    #[test]
    fn gic_level_sensitive_deassertion() {
        let mut gic = Gic::new();
        gic.dist_write(GICD_CTLR, 1, 4);
        gic.cpu_write(GICC_CTLR, 1, 4);
        gic.cpu_write(GICC_PMR, 0xFF, 4);

        // Enable IRQ 27 (Timer PPI)
        gic.dist_write(GICD_ISENABLER_START, 1 << 27, 4);
        gic.dist_write(GICD_IPRIORITYR_START + 27, 0x10, 1);

        // Assert level line
        gic.set_irq_level(27, true);
        assert!(gic.cpu_irq_asserted());

        // Deassert level line before ACK -> pending is cleared
        gic.set_irq_level(27, false);
        assert!(!gic.cpu_irq_asserted());
        assert_eq!(gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);
    }

    #[test]
    fn gic_preemption_higher_priority() {
        let mut gic = Gic::new();
        gic.dist_write(GICD_CTLR, 1, 4);
        gic.cpu_write(GICC_CTLR, 1, 4);
        gic.cpu_write(GICC_PMR, 0xFF, 4);

        // IRQ 35: low priority 0x80
        // IRQ 36: high priority 0x20
        gic.dist_write(GICD_ISENABLER_START + 4, (1 << (35 - 32)) | (1 << (36 - 32)), 4);
        gic.dist_write(GICD_IPRIORITYR_START + 35, 0x80, 1);
        gic.dist_write(GICD_IPRIORITYR_START + 36, 0x20, 1);

        // Assert both lines simultaneously
        gic.set_irq_level(35, true);
        gic.set_irq_level(36, true);

        // Higher priority (36) must be acknowledged first
        let ack1 = gic.cpu_read(GICC_IAR, 4);
        assert_eq!(ack1, 36);
        // Device 36 ISR services the device and deasserts its line:
        gic.set_irq_level(36, false);

        // End IRQ 36
        gic.cpu_write(GICC_EOIR, 36, 4);

        // Now lower priority (35) is delivered
        assert!(gic.cpu_irq_asserted());
        let ack2 = gic.cpu_read(GICC_IAR, 4);
        assert_eq!(ack2, 35);
        gic.cpu_write(GICC_EOIR, 35, 4);
    }
}
