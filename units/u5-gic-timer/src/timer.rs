//! ARM Generic Timer Device Model + Interrupt Routing.
//!
//! Purity: EXPLICIT-STATE. All state lives inside [`GenericTimer`].
//! Performs no I/O, no threading, no time reads — fully deterministic.
//!
//! # Architecture Reference
//! - ARMv8-A Architecture Reference Manual (DDI 0487), Section D11 (The Generic Timer)
//! - Virtual Timer PPI: INTID 27 (`CNTV`)
//! - Physical Timer PPI: INTID 30 (`CNTP`)
//!
//! # Control Register Bits (`CNTP_CTL_EL0` / `CNTV_CTL_EL0`)
//! - bit 0: `ENABLE` (1 = timer enabled)
//! - bit 1: `IMASK` (1 = interrupt output masked, no IRQ asserted)
//! - bit 2: `ISTATUS` (1 = timer condition met: `counter >= cval`)

use crate::gic::Gic;

pub const VIRT_TIMER_IRQ_NUM: u32 = 27;
pub const PHYS_TIMER_IRQ_NUM: u32 = 30;

pub const TIMER_CTL_ENABLE: u32 = 1 << 0;
pub const TIMER_CTL_IMASK: u32 = 1 << 1;
pub const TIMER_CTL_ISTATUS: u32 = 1 << 2;

/// Explicit state of the ARM Generic Timer (both Physical and Virtual timers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericTimer {
    pub cntpct: u64,
    pub cntp_cval: u64,
    pub cntp_ctl: u32,

    pub cntvct: u64,
    pub cntv_cval: u64,
    pub cntv_ctl: u32,
    pub cntvoff: u64,
}

impl Default for GenericTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl GenericTimer {
    pub fn new() -> Self {
        Self {
            cntpct: 0,
            cntp_cval: 0,
            cntp_ctl: 0,
            cntvct: 0,
            cntv_cval: 0,
            cntv_ctl: 0,
            cntvoff: 0,
        }
    }

    /// Advance the free-running counters by `cycles`.
    pub fn tick(&mut self, cycles: u64) {
        self.cntpct = self.cntpct.wrapping_add(cycles);
        self.cntvct = self.cntpct.wrapping_sub(self.cntvoff);
        self.update_status();
    }

    /// Update ISTATUS bits for both timers based on current count and compare values.
    fn update_status(&mut self) {
        // Physical timer condition
        if (self.cntp_ctl & TIMER_CTL_ENABLE) != 0 && self.cntpct >= self.cntp_cval {
            self.cntp_ctl |= TIMER_CTL_ISTATUS;
        } else {
            self.cntp_ctl &= !TIMER_CTL_ISTATUS;
        }

        // Virtual timer condition
        if (self.cntv_ctl & TIMER_CTL_ENABLE) != 0 && self.cntvct >= self.cntv_cval {
            self.cntv_ctl |= TIMER_CTL_ISTATUS;
        } else {
            self.cntv_ctl &= !TIMER_CTL_ISTATUS;
        }
    }

    /// Whether the virtual timer interrupt output is currently asserted.
    pub fn virt_timer_asserted(&self) -> bool {
        let condition_met = (self.cntv_ctl & TIMER_CTL_ENABLE) != 0 && self.cntvct >= self.cntv_cval;
        let masked = (self.cntv_ctl & TIMER_CTL_IMASK) != 0;
        condition_met && !masked
    }

    /// Whether the physical timer interrupt output is currently asserted.
    pub fn phys_timer_asserted(&self) -> bool {
        let condition_met = (self.cntp_ctl & TIMER_CTL_ENABLE) != 0 && self.cntpct >= self.cntp_cval;
        let masked = (self.cntp_ctl & TIMER_CTL_IMASK) != 0;
        condition_met && !masked
    }

    // ---- TVAL Derived Registers ----

    pub fn read_cntp_tval(&self) -> i32 {
        self.cntp_cval.wrapping_sub(self.cntpct) as u32 as i32
    }

    pub fn write_cntp_tval(&mut self, tval: i32) {
        self.cntp_cval = self.cntpct.wrapping_add(tval as i64 as u64);
        self.update_status();
    }

    pub fn read_cntv_tval(&self) -> i32 {
        self.cntv_cval.wrapping_sub(self.cntvct) as u32 as i32
    }

    pub fn write_cntv_tval(&mut self, tval: i32) {
        self.cntv_cval = self.cntvct.wrapping_add(tval as i64 as u64);
        self.update_status();
    }

    // ---- Control Registers ----

    pub fn set_cntp_ctl(&mut self, val: u32) {
        // ISTATUS (bit 2) is read-only from guest
        self.cntp_ctl = (self.cntp_ctl & TIMER_CTL_ISTATUS) | (val & (TIMER_CTL_ENABLE | TIMER_CTL_IMASK));
        self.update_status();
    }

    pub fn set_cntv_ctl(&mut self, val: u32) {
        self.cntv_ctl = (self.cntv_ctl & TIMER_CTL_ISTATUS) | (val & (TIMER_CTL_ENABLE | TIMER_CTL_IMASK));
        self.update_status();
    }

    // ---- Step & Drive GIC Interface ----

    /// Advance the timer by `cycles` and route the output interrupt lines into the [`Gic`].
    pub fn step(&mut self, cycles: u64, gic: &mut Gic) {
        self.tick(cycles);
        gic.set_irq_level(VIRT_TIMER_IRQ_NUM, self.virt_timer_asserted());
        gic.set_irq_level(PHYS_TIMER_IRQ_NUM, self.phys_timer_asserted());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gic::{
        GICC_CTLR, GICC_EOIR, GICC_IAR, GICC_PMR, GICD_CTLR, GICD_IPRIORITYR_START,
        GICD_ISENABLER_START, SPURIOUS_IRQ,
    };

    #[test]
    fn timer_initial_defaults() {
        let timer = GenericTimer::new();
        assert_eq!(timer.cntpct, 0);
        assert_eq!(timer.cntvct, 0);
        assert!(!timer.virt_timer_asserted());
        assert!(!timer.phys_timer_asserted());
    }

    #[test]
    fn timer_counter_advancement() {
        let mut timer = GenericTimer::new();
        timer.tick(1000);
        assert_eq!(timer.cntpct, 1000);
        assert_eq!(timer.cntvct, 1000);

        timer.tick(500);
        assert_eq!(timer.cntpct, 1500);
        assert_eq!(timer.cntvct, 1500);
    }

    #[test]
    fn timer_tval_roundtrip() {
        let mut timer = GenericTimer::new();
        timer.tick(5000);

        // Set timer for 1000 cycles in future
        timer.write_cntv_tval(1000);
        assert_eq!(timer.cntv_cval, 6000);
        assert_eq!(timer.read_cntv_tval(), 1000);

        // Advance 400 cycles
        timer.tick(400);
        assert_eq!(timer.read_cntv_tval(), 600);

        // Advance 600 cycles (lands on compare)
        timer.tick(600);
        assert_eq!(timer.read_cntv_tval(), 0);

        // Advance 200 cycles past compare (negative TVAL)
        timer.tick(200);
        assert_eq!(timer.read_cntv_tval(), -200);
    }

    #[test]
    fn timer_assertion_and_masking() {
        let mut timer = GenericTimer::new();
        timer.cntv_cval = 1000;
        timer.set_cntv_ctl(TIMER_CTL_ENABLE);

        // Not yet due
        timer.tick(999);
        assert_eq!(timer.cntv_ctl & TIMER_CTL_ISTATUS, 0);
        assert!(!timer.virt_timer_asserted());

        // Hits compare
        timer.tick(1);
        assert_ne!(timer.cntv_ctl & TIMER_CTL_ISTATUS, 0);
        assert!(timer.virt_timer_asserted(), "Virtual timer line should assert");

        // Mask the timer via IMASK
        timer.set_cntv_ctl(TIMER_CTL_ENABLE | TIMER_CTL_IMASK);
        assert_ne!(timer.cntv_ctl & TIMER_CTL_ISTATUS, 0, "ISTATUS still 1 when masked");
        assert!(!timer.virt_timer_asserted(), "Line deasserted when masked");

        // Unmask
        timer.set_cntv_ctl(TIMER_CTL_ENABLE);
        assert!(timer.virt_timer_asserted());

        // Re-arm in future
        timer.cntv_cval = 5000;
        timer.tick(0);
        assert_eq!(timer.cntv_ctl & TIMER_CTL_ISTATUS, 0);
        assert!(!timer.virt_timer_asserted());
    }

    #[test]
    fn timer_gic_integration_baremetal_flow() {
        let mut timer = GenericTimer::new();
        let mut gic = Gic::new();

        // 1. Initialize GIC
        gic.dist_write(GICD_CTLR, 1, 4);
        gic.cpu_write(GICC_CTLR, 1, 4);
        gic.cpu_write(GICC_PMR, 0xFF, 4); // allow all priorities

        // Enable Virtual Timer PPI (27)
        gic.dist_write(GICD_ISENABLER_START, 1 << VIRT_TIMER_IRQ_NUM, 4);
        gic.dist_write(GICD_IPRIORITYR_START + VIRT_TIMER_IRQ_NUM, 0x10, 1);

        // 2. Program Virtual Timer for 1000 cycles
        timer.write_cntv_tval(1000);
        timer.set_cntv_ctl(TIMER_CTL_ENABLE);

        // 3. Step 500 cycles -> timer not reached
        timer.step(500, &mut gic);
        assert!(!gic.cpu_irq_asserted());
        assert_eq!(gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);

        // 4. Step 500 more cycles -> hits compare (count = 1000)
        timer.step(500, &mut gic);
        assert!(gic.cpu_irq_asserted(), "GIC CPU IRQ must assert on timer expiration");

        // 5. Bare-metal ISR: acknowledge IRQ from GIC
        let intid = gic.cpu_read(GICC_IAR, 4);
        assert_eq!(intid, VIRT_TIMER_IRQ_NUM, "Acknowledged IRQ must be PPI 27");
        assert!(!gic.cpu_irq_asserted(), "Line deasserted while servicing active IRQ");

        // 6. Bare-metal ISR re-arms the timer for next tick
        timer.write_cntv_tval(1000);
        timer.step(0, &mut gic); // sync timer line to GIC: timer condition cleared, line low

        // 7. End of Interrupt (EOI)
        gic.cpu_write(GICC_EOIR, VIRT_TIMER_IRQ_NUM, 4);
        assert_eq!(gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ, "No more IRQs pending");
    }
}
