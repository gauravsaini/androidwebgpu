//! U14 metrics-core — PURE frame-time aggregation (LLD §14).
//!
//! Responsibility: turn a stream of per-frame measurements into HUD-ready
//! summary statistics. This crate is PURE: [`aggregate`] is a deterministic
//! function of its input slice — no I/O, no clock, no threads, no hidden
//! state. Rendering stays in `crates/metrics_overlay` (QUARANTINED); this
//! crate never touches it.
//!
//! ## Defensive-input policy (never panics)
//!
//! Real frame measurements are noisy, so the policy is explicit:
//!
//! - **NaN / ±infinity `dt_ms`**: excluded from the sample, as if the frame
//!   were never measured. A non-finite sample carries no information about
//!   duration; including it would poison the mean to NaN/∞. If every sample
//!   is excluded, the result is the empty-slice result (all zeros).
//! - **Negative `dt_ms`**: clamped to `0.0`. A negative frame time is a timer
//!   artifact, not a duration; clamping keeps the sample count stable while
//!   refusing to let artifacts drag the mean below zero.
//! - **Overflow of `total_draw_calls`**: saturating addition on `u64`. The
//!   plain `Iterator::sum` would panic in debug builds on overflow; this
//!   crate must never panic.
//! - **Empty input**: `fps = 0.0`, `avg_ms = 0.0`, `p95_ms = 0.0`,
//!   `total_draw_calls = 0`. There is no meaningful rate for zero frames, so
//!   `fps` is defined as `0.0` rather than infinity.
//! - **`fps` when `avg_ms <= 0.0`**: defined as `0.0` (division by zero is
//!   avoided; a zero mean implies the clamped-artifact case above).
//!
//! ## Statistic definitions
//!
//! - `avg_ms`: arithmetic mean of the valid `dt_ms` samples, accumulated in
//!   `f64` to avoid precision drift over long sessions, then rounded back to
//!   `f32`.
//! - `fps`: `1000.0 / avg_ms` (`0.0` when `avg_ms <= 0.0`).
//! - `p95_ms`: 95th percentile by the nearest-rank method — sort the valid
//!   samples ascending with a total order, take rank `ceil(0.95 * n)`,
//!   1-indexed. Deterministic; no interpolation.
//! - `total_draw_calls`: saturating sum of `draw_calls` over **all** events,
//!   including events whose `dt_ms` was excluded or clamped (a dropped
//!   timing measurement does not erase a counted draw call).

/// One completed frame: its measured duration and draw-call count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameEvent {
    /// Frame duration in milliseconds. May be negative (clamped to 0.0) or
    /// non-finite (excluded); see the module-level policy.
    pub dt_ms: f32,
    /// Draw calls issued for this frame.
    pub draw_calls: u32,
}

/// Aggregated HUD statistics over a frame-event window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    /// Frames per second: `1000.0 / avg_ms`, or `0.0` when undefined.
    pub fps: f32,
    /// Mean frame time in milliseconds (`0.0` when no valid samples).
    pub avg_ms: f32,
    /// 95th-percentile frame time, nearest-rank (`0.0` when no valid samples).
    pub p95_ms: f32,
    /// Saturating total of `draw_calls` across all events.
    pub total_draw_calls: u64,
}

impl Stats {
    /// The all-zeros result for an empty (or fully excluded) sample.
    pub const ZERO: Stats = Stats {
        fps: 0.0,
        avg_ms: 0.0,
        p95_ms: 0.0,
        total_draw_calls: 0,
    };
}

/// Aggregate a slice of frame events into HUD statistics.
///
/// Pure: the output depends only on `events`. Never panics; see the
/// module-level defensive-input policy.
///
/// ```
/// use u14_metrics::{aggregate, FrameEvent};
///
/// let stats = aggregate(&[
///     FrameEvent { dt_ms: 16.0, draw_calls: 2 },
///     FrameEvent { dt_ms: 20.0, draw_calls: 3 },
/// ]);
/// assert!((stats.avg_ms - 18.0).abs() < 1e-6);
/// assert!((stats.fps - 55.555_6).abs() < 1e-3);
/// assert_eq!(stats.total_draw_calls, 5);
/// ```
pub fn aggregate(events: &[FrameEvent]) -> Stats {
    // Collect valid (finite) samples, clamping negatives to 0.0, in f64 for
    // accumulation precision. Draw calls are summed separately over ALL
    // events so a bad timer reading never erases counted work.
    let mut total_draw_calls: u64 = 0;
    let mut samples: Vec<f32> = Vec::with_capacity(events.len());
    for e in events {
        total_draw_calls = total_draw_calls.saturating_add(u64::from(e.draw_calls));
        if e.dt_ms.is_finite() {
            samples.push(e.dt_ms.max(0.0));
        }
    }

    if samples.is_empty() {
        // Timing stats are undefined, but draw calls were already counted.
        return Stats {
            total_draw_calls,
            ..Stats::ZERO
        };
    }

    let n = samples.len();
    let sum_f64: f64 = samples.iter().map(|&d| f64::from(d)).sum();
    let avg_f64 = sum_f64 / n as f64;
    let avg_ms = avg_f64 as f32;

    let fps = if avg_f64 > 0.0 {
        (1000.0 / avg_f64) as f32
    } else {
        0.0
    };

    // Nearest-rank percentile: total order sort (no NaN can remain), rank =
    // ceil(0.95 * n), 1-indexed.
    samples.sort_by(|a, b| a.total_cmp(b));
    let rank = (0.95 * n as f64).ceil() as usize; // >= 1 for n >= 1
    let p95_ms = samples[rank - 1];

    Stats {
        fps,
        avg_ms,
        p95_ms,
        total_draw_calls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn assert_stats(s: &Stats, fps: f32, avg: f32, p95: f32, draws: u64) {
        assert!(
            approx_eq(s.fps, fps, 1e-3),
            "fps: got {}, want {}",
            s.fps,
            fps
        );
        assert!(
            approx_eq(s.avg_ms, avg, 1e-3),
            "avg_ms: got {}, want {}",
            s.avg_ms,
            avg
        );
        assert!(
            approx_eq(s.p95_ms, p95, 1e-3),
            "p95_ms: got {}, want {}",
            s.p95_ms,
            p95
        );
        assert_eq!(s.total_draw_calls, draws);
    }

    #[test]
    fn aggregation_synthetic_stream_matches_hand_computed() {
        // 20 frames: ten at 16.0 ms, ten at 20.0 ms; 5 draw calls each.
        // mean = 18.0 -> fps = 55.555...
        // p95 nearest-rank: rank = ceil(0.95*20) = 19 -> sorted[18] = 20.0
        // total draws = 100
        let events: Vec<FrameEvent> = (0..10)
            .map(|_| FrameEvent {
                dt_ms: 16.0,
                draw_calls: 5,
            })
            .chain((0..10).map(|_| FrameEvent {
                dt_ms: 20.0,
                draw_calls: 5,
            }))
            .collect();
        let s = aggregate(&events);
        assert_stats(&s, 55.555_6, 18.0, 20.0, 100);
    }

    #[test]
    fn aggregation_ascending_stream_percentile_rank() {
        // dts 1..=20 ms, one draw call each.
        // mean = 10.5 -> fps = 95.238...
        // p95: rank = ceil(19.0) = 19 -> sorted[18] = 19.0
        let events: Vec<FrameEvent> = (1..=20)
            .map(|d| FrameEvent {
                dt_ms: d as f32,
                draw_calls: 1,
            })
            .collect();
        let s = aggregate(&events);
        assert_stats(&s, 95.238_1, 10.5, 19.0, 20);
    }

    #[test]
    fn aggregation_empty_slice_returns_zeros() {
        let s = aggregate(&[]);
        assert_eq!(s, Stats::ZERO);
        assert_eq!(s.fps, 0.0);
        assert_eq!(s.avg_ms, 0.0);
        assert_eq!(s.p95_ms, 0.0);
        assert_eq!(s.total_draw_calls, 0);
    }

    #[test]
    fn aggregation_single_frame() {
        // n=1: mean = dt, fps = 1000/dt, p95 rank = ceil(0.95) = 1 -> dt.
        let s = aggregate(&[FrameEvent {
            dt_ms: 16.666,
            draw_calls: 7,
        }]);
        assert_stats(&s, 60.002_4, 16.666, 16.666, 7);
    }

    #[test]
    fn aggregation_defensive_negative_and_nan_dt() {
        // -5.0 -> clamped to 0.0 (kept, draws counted)
        // NaN  -> excluded entirely (draws still counted)
        // valid samples: [0.0, 20.0]; mean = 10.0 -> fps = 100.0
        // p95: rank = ceil(1.9) = 2 -> sorted[1] = 20.0
        // total draws = 3 + 4 + 5 = 12
        let events = [
            FrameEvent {
                dt_ms: -5.0,
                draw_calls: 3,
            },
            FrameEvent {
                dt_ms: f32::NAN,
                draw_calls: 4,
            },
            FrameEvent {
                dt_ms: 20.0,
                draw_calls: 5,
            },
        ];
        let s = aggregate(&events);
        assert_stats(&s, 100.0, 10.0, 20.0, 12);
    }

    #[test]
    fn aggregation_all_nonfinite_dt_behaves_like_empty() {
        let events = [
            FrameEvent {
                dt_ms: f32::NAN,
                draw_calls: 2,
            },
            FrameEvent {
                dt_ms: f32::INFINITY,
                draw_calls: 3,
            },
            FrameEvent {
                dt_ms: f32::NEG_INFINITY,
                draw_calls: 4,
            },
        ];
        let s = aggregate(&events);
        // Timing stats are empty-like, but draw calls were still counted.
        assert_eq!(s.fps, 0.0);
        assert_eq!(s.avg_ms, 0.0);
        assert_eq!(s.p95_ms, 0.0);
        assert_eq!(s.total_draw_calls, 9);
    }

    #[test]
    fn aggregation_all_zero_dt_gives_zero_fps_not_inf() {
        let events = [
            FrameEvent {
                dt_ms: 0.0,
                draw_calls: 1,
            },
            FrameEvent {
                dt_ms: -1.0,
                draw_calls: 1,
            },
        ];
        let s = aggregate(&events);
        assert_eq!(s.avg_ms, 0.0);
        assert_eq!(s.fps, 0.0);
        assert!(s.fps.is_finite());
        assert_eq!(s.p95_ms, 0.0);
        assert_eq!(s.total_draw_calls, 2);
    }

    #[test]
    fn aggregation_deterministic_across_calls() {
        let events: Vec<FrameEvent> = (0..50)
            .map(|i| FrameEvent {
                dt_ms: 12.0 + (i % 7) as f32,
                draw_calls: i,
            })
            .collect();
        assert_eq!(aggregate(&events), aggregate(&events));
    }
}
