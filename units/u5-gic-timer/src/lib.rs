//! U5 `gic-timer` — interrupt controller + ARM generic timer tick.
//!
//! Purity: EXPLICIT-STATE. All state travels in the signature; the function is
//! deterministic and performs no I/O, time reads, or threading.
//!
//! # Model
//! - The 64-bit free-running counter advances by `elapsed_cycles` every tick,
//!   even when the timer is disabled ("state advanced honestly").
//! - The timer line is **level-triggered**: it is asserted exactly while
//!   `timer enabled AND counter >= timer_compare`.
//! - [`tick`] returns at most one [`Irq`] per call, on the **rising edge** of the
//!   line (entry state had the line deasserted). While the line stays asserted
//!   across ticks no further IRQs are emitted — the guest re-arms by writing a
//!   new `timer_compare` (a future MMIO path, outside this unit), which
//!   deasserts the line; `tick` clears the timer pending bit accordingly.
//! - Enabling a past-due timer, or re-arming with a compare in the past, fires
//!   exactly once on the next tick (the line rises).
//!
//! # Owned constants
//! The frozen contracts (`pathn_contracts::cpu`) fix `Irq { num: u32 }` but no
//! number for the generic timer, so this unit owns the assignment:
//! - [`TIMER_IRQ_NUM`] = 27 — ARM virtual-timer PPI (SBSA INTID 27), the
//!   canonical vCPU-facing generic timer in virtualized guests.
//! - [`TIMER_ENABLE_BIT`] = 0 — bit of `IrqState.enabled` gating the timer,
//!   mirroring `CNTV_CTL_EL0.ENABLE`.
//! - [`TIMER_PENDING_BIT`] — `IrqState.pending` bit indexed by the INTID.
//!
//! # Known limitation
//! `u64` wrap of the counter takes ~585 years at 1 GHz. If `counter +
//! elapsed` wraps *through* `timer_compare`, no IRQ fires for that tick
//! (comparison is on the wrapped value). This is documented, not silent: the
//! counter itself still wraps correctly via `wrapping_add` (never panics).

use pathn_contracts::cpu::{Irq, IrqState};

/// Generic-timer interrupt number: ARM virtual timer PPI → INTID 27.
///
/// Owned by this unit; the frozen contract only fixes the `Irq` shape.
pub const TIMER_IRQ_NUM: u32 = 27;

/// Bit of [`IrqState::enabled`] that gates the generic timer (bit 0).
pub const TIMER_ENABLE_BIT: u32 = 0;

/// [`IrqState::pending`] bit tracking the timer line, indexed by INTID.
pub const TIMER_PENDING_BIT: u64 = 1u64 << TIMER_IRQ_NUM;

/// Advance the generic timer and report newly asserted interrupts.
///
/// Returns the updated state plus at most one [`Irq`] — emitted only on the
/// rising edge of the timer line. See the module docs for the full model.
pub fn tick(state: &IrqState, elapsed_cycles: u64) -> (IrqState, Vec<Irq>) {
    let new_count = state.timer_count.wrapping_add(elapsed_cycles);
    let enabled = (state.enabled & (1u32 << TIMER_ENABLE_BIT)) != 0;
    let asserted = enabled && new_count >= state.timer_compare;
    let was_asserted = (state.pending & TIMER_PENDING_BIT) != 0;

    let mut pending = state.pending & !TIMER_PENDING_BIT;
    let mut irqs = Vec::new();
    if asserted {
        pending |= TIMER_PENDING_BIT;
        if !was_asserted {
            irqs.push(Irq { num: TIMER_IRQ_NUM });
        }
    }

    (
        IrqState {
            enabled: state.enabled,
            pending,
            timer_count: new_count,
            timer_compare: state.timer_compare,
        },
        irqs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer_state(count: u64, compare: u64, enabled: u32) -> IrqState {
        IrqState {
            enabled,
            pending: 0,
            timer_count: count,
            timer_compare: compare,
        }
    }

    const ENABLED: u32 = 1u32 << TIMER_ENABLE_BIT;

    #[test]
    fn timer_compare_match_emits_exact_irq_number() {
        // Edge: counter lands exactly on the compare value.
        let (next, irqs) = tick(&timer_state(990, 1000, ENABLED), 10);
        assert_eq!(next.timer_count, 1000);
        assert_eq!(irqs, vec![Irq { num: 27 }]);
        assert_eq!(irqs[0].num, TIMER_IRQ_NUM);
        assert_ne!(next.pending & TIMER_PENDING_BIT, 0);
    }

    #[test]
    fn timer_compare_passed_emits_exact_irq_number() {
        // Reaching *past* the compare value also fires.
        let (next, irqs) = tick(&timer_state(990, 1000, ENABLED), 25);
        assert_eq!(next.timer_count, 1015);
        assert_eq!(irqs, vec![Irq { num: TIMER_IRQ_NUM }]);
    }

    #[test]
    fn timer_just_before_compare_emits_nothing() {
        let (next, irqs) = tick(&timer_state(989, 1000, ENABLED), 10);
        assert_eq!(next.timer_count, 999);
        assert!(irqs.is_empty(), "spurious IRQ before compare: {irqs:?}");
        assert_eq!(next.pending & TIMER_PENDING_BIT, 0);
    }

    #[test]
    fn timer_disabled_emits_nothing_but_advances_counter() {
        // Disabled: no IRQ, but the counter still advances honestly.
        let (next, irqs) = tick(&timer_state(990, 1000, 0), 10);
        assert_eq!(next.timer_count, 1000);
        assert!(irqs.is_empty(), "disabled timer must not fire: {irqs:?}");
        assert_eq!(next.pending & TIMER_PENDING_BIT, 0);
    }

    #[test]
    fn timer_enable_bit_zero_only_bit_zero_counts() {
        // Other bits in `enabled` must not gate the timer.
        let (next, irqs) = tick(&timer_state(990, 1000, 0b10), 10);
        assert!(irqs.is_empty());
        assert_eq!(next.timer_count, 1000);
    }

    #[test]
    fn timer_counter_overflow_wraps_correctly() {
        // u64::MAX - 5 + 10 wraps to 4; must not panic in debug builds.
        let (next, irqs) = tick(&timer_state(u64::MAX - 5, u64::MAX, ENABLED), 10);
        assert_eq!(next.timer_count, 4);
        assert!(irqs.is_empty(), "wrapped count 4 < compare: {irqs:?}");
    }

    #[test]
    fn timer_stays_asserted_without_repeat_irq_until_rearm() {
        // First tick: rising edge -> one IRQ. Second tick, still asserted ->
        // none. Guest re-arms with a future compare -> line deasserts, pending
        // bit clears, no IRQ.
        let s0 = timer_state(990, 1000, ENABLED);
        let (s1, irqs1) = tick(&s0, 10);
        assert_eq!(irqs1.len(), 1);
        let (s2, irqs2) = tick(&s1, 10);
        assert!(irqs2.is_empty(), "repeat IRQ while asserted: {irqs2:?}");
        assert_ne!(s2.pending & TIMER_PENDING_BIT, 0);
        let rearmed = IrqState {
            timer_compare: s2.timer_count + 5000,
            ..s2
        };
        let (s3, irqs3) = tick(&rearmed, 10);
        assert!(irqs3.is_empty());
        assert_eq!(s3.pending & TIMER_PENDING_BIT, 0);
        assert_eq!(s3.timer_count, s2.timer_count + 10);
    }

    #[test]
    fn timer_rearm_in_past_fires_once() {
        // Guest programs a compare at/below the current count: fires exactly
        // once on the next tick (line rises), not on every tick after.
        let s0 = timer_state(5000, 9000, ENABLED);
        let (s1, _) = tick(&s0, 10);
        assert!(s1.pending & TIMER_PENDING_BIT == 0);
        let rearmed = IrqState {
            timer_compare: 100, // in the past
            ..s1
        };
        let (s2, irqs2) = tick(&rearmed, 10);
        assert_eq!(irqs2, vec![Irq { num: TIMER_IRQ_NUM }]);
        let (_, irqs3) = tick(&s2, 10);
        assert!(irqs3.is_empty());
    }

    #[test]
    fn timer_zero_elapsed_still_evaluates_condition() {
        // A zero-elapsed tick right after arming must fire when past due.
        let (next, irqs) = tick(&timer_state(1000, 1000, ENABLED), 0);
        assert_eq!(next.timer_count, 1000);
        assert_eq!(irqs, vec![Irq { num: TIMER_IRQ_NUM }]);
    }

    #[test]
    fn timer_soak_no_compare_hit_emits_zero_irqs() {
        // 10k ticks, compare never reached -> zero IRQs, counter exact.
        let mut state = timer_state(0, u64::MAX, ENABLED);
        let mut total = 0usize;
        for _ in 0..10_000 {
            let (next, irqs) = tick(&state, 7);
            total += irqs.len();
            state = next;
        }
        assert_eq!(total, 0, "spurious IRQs in soak test");
        assert_eq!(state.timer_count, 70_000);
        assert_eq!(state.pending & TIMER_PENDING_BIT, 0);
    }

    #[test]
    fn timer_is_deterministic_and_pure() {
        let s = timer_state(42, 100, ENABLED);
        let (a_state, a_irqs) = tick(&s, 58);
        let (b_state, b_irqs) = tick(&s, 58);
        assert_eq!(a_state, b_state);
        assert_eq!(a_irqs, b_irqs);
        assert_eq!(a_irqs, vec![Irq { num: TIMER_IRQ_NUM }]);
        // Input state untouched (borrowed, not mutated).
        assert_eq!(s.timer_count, 42);
    }
}
