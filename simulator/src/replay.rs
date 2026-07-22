//! Simulator integration with the storage [`ReplayEngine`][flyby_storage::ReplayEngine].
//!
//! Part VI requires replay options (original timestamps, accelerated, slowed,
//! paused, single-step).  The storage crate already owns [`ReplayMode`] and
//! [`ReplayEngine`][flyby_storage::ReplayEngine]; this module adapts them to
//! the simulator's virtual clock so deterministic scenarios do not depend on
//! wall-clock `sleep`.
//!
//! ## Virtual-clock modes
//!
//! | Mode | Behaviour under [`SimReplay::ready_at`] |
//! |---|---|
//! | FullSpeed | Always ready |
//! | OriginalTiming | Ready when `clock_ns` covers the record delta |
//! | TimeScaled | Same as original, with `factor` applied |
//! | Burst | Ready for `count` records, then gap in virtual ns |
//! | SingleStep | Ready once per [`SimReplay::advance`] |

use std::time::Duration;

use flyby_storage::ReplayMode;

/// Simulator-facing replay controller driven by virtual (or real) clock ns.
#[derive(Debug, Clone)]
pub struct SimReplay {
    mode: ReplayMode,
    first_ts_ns: Option<u64>,
    burst_count: usize,
    /// Virtual-clock nanoseconds when the current burst gap ends.
    burst_resume_ns: Option<u64>,
    single_step_armed: bool,
    /// When paused, nothing is ready until [`resume`][Self::resume].
    paused: bool,
}

impl SimReplay {
    /// Create a replay controller for the given storage replay mode.
    pub fn new(mode: ReplayMode) -> flyby_core::Result<Self> {
        mode.validate()?;
        Ok(Self {
            mode,
            first_ts_ns: None,
            burst_count: 0,
            burst_resume_ns: None,
            single_step_armed: true,
            paused: false,
        })
    }

    /// Borrow the configured mode.
    pub fn mode(&self) -> &ReplayMode {
        &self.mode
    }

    /// Pause emission (educational / interactive).
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume after [`pause`][Self::pause].
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// `true` when emission is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Arm the next SingleStep emission. No-op for other modes.
    pub fn advance(&mut self) {
        if matches!(self.mode, ReplayMode::SingleStep) {
            self.single_step_armed = true;
        }
    }

    /// Query whether a record with `timestamp_ns` is ready at simulator
    /// clock value `clock_ns`.
    pub fn ready_at(&mut self, timestamp_ns: u64, clock_ns: u64) -> bool {
        if self.paused {
            return false;
        }

        match &self.mode {
            ReplayMode::FullSpeed => true,

            ReplayMode::SingleStep => {
                if !self.single_step_armed {
                    return false;
                }
                self.single_step_armed = false;
                true
            }

            ReplayMode::OriginalTiming => self.timing_ready(timestamp_ns, clock_ns, 1.0),

            ReplayMode::TimeScaled { factor } => {
                let f = *factor;
                self.timing_ready(timestamp_ns, clock_ns, f)
            }

            ReplayMode::Burst { count, gap } => {
                let count = *count;
                let gap = *gap;
                self.burst_ready(clock_ns, count, gap)
            }
        }
    }

    fn timing_ready(&mut self, ts_ns: u64, clock_ns: u64, factor: f64) -> bool {
        let first_ts = *self.first_ts_ns.get_or_insert(ts_ns);
        if ts_ns <= first_ts {
            return true;
        }
        let original_delta_ns = ts_ns.saturating_sub(first_ts);
        let scaled_ns = if (factor - 1.0).abs() < f64::EPSILON {
            original_delta_ns
        } else {
            let scaled = (original_delta_ns as f64) * factor;
            if !scaled.is_finite() || scaled < 0.0 {
                return true;
            }
            if scaled >= u64::MAX as f64 {
                u64::MAX
            } else {
                scaled as u64
            }
        };
        clock_ns >= scaled_ns
    }

    fn burst_ready(&mut self, clock_ns: u64, burst_count: usize, gap: Duration) -> bool {
        if let Some(resume) = self.burst_resume_ns {
            if clock_ns < resume {
                return false;
            }
            self.burst_resume_ns = None;
            self.burst_count = 0;
        }

        self.burst_count += 1;
        if self.burst_count >= burst_count {
            self.burst_resume_ns = Some(clock_ns.saturating_add(gap.as_nanos() as u64));
            self.burst_count = 0;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_speed_always_ready() {
        let mut r = SimReplay::new(ReplayMode::FullSpeed).unwrap();
        assert!(r.ready_at(0, 0));
        assert!(r.ready_at(1_000_000, 0));
    }

    #[test]
    fn original_timing_waits_for_clock() {
        let mut r = SimReplay::new(ReplayMode::OriginalTiming).unwrap();
        assert!(r.ready_at(1_000, 0)); // first record anchors
        assert!(!r.ready_at(1_000 + 5_000_000, 1_000_000)); // need 5 ms
        assert!(r.ready_at(1_000 + 5_000_000, 5_000_000));
    }

    #[test]
    fn time_scaled_slows_down() {
        let mut r = SimReplay::new(ReplayMode::TimeScaled { factor: 2.0 }).unwrap();
        assert!(r.ready_at(0, 0));
        // 1 ms original → 2 ms scaled
        assert!(!r.ready_at(1_000_000, 1_000_000));
        assert!(r.ready_at(1_000_000, 2_000_000));
    }

    #[test]
    fn single_step_requires_advance() {
        let mut r = SimReplay::new(ReplayMode::SingleStep).unwrap();
        assert!(r.ready_at(0, 0));
        assert!(!r.ready_at(1, 0));
        r.advance();
        assert!(r.ready_at(1, 0));
    }

    #[test]
    fn pause_blocks_emission() {
        let mut r = SimReplay::new(ReplayMode::FullSpeed).unwrap();
        r.pause();
        assert!(!r.ready_at(0, 0));
        r.resume();
        assert!(r.ready_at(0, 0));
    }

    #[test]
    fn burst_then_gap_in_virtual_time() {
        let mut r = SimReplay::new(ReplayMode::Burst {
            count: 2,
            gap: Duration::from_millis(1),
        })
        .unwrap();
        assert!(r.ready_at(0, 0));
        assert!(r.ready_at(0, 0)); // second ends burst, schedules gap
        assert!(!r.ready_at(0, 500_000));
        assert!(r.ready_at(0, 1_000_000));
    }
}
