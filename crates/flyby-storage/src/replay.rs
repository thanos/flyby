//! Replay engine.
//!
//! The replay engine is a timing helper that sits **outside** a
//! [`StorageSource`][crate::source::StorageSource]: the caller polls the
//! source, then asks the engine whether the next record's timestamp is
//! ready to emit.
//!
//! ## Modes
//!
//! | Mode | Description |
//! |---|---|
//! | [`ReplayMode::FullSpeed`] | No throttling; emit as fast as the source reads |
//! | [`ReplayMode::OriginalTiming`] | Honour the per-record timestamp; reconstruct original inter-arrival gaps |
//! | [`ReplayMode::TimeScaled`] | Stretch or compress the original timing by a factor |
//! | [`ReplayMode::Burst`] | Emit a fixed burst of records then idle for a gap |
//! | [`ReplayMode::SingleStep`] | Emit at most one ready record per successful poll; call [`ReplayEngine::advance`] to arm the next |
//!
//! ## Timing implementation
//!
//! The engine uses the monotonic clock ([`std::time::Instant`]) to track wall
//! time; it does **not** call `sleep`.  Instead, [`poll`][ReplayEngine::poll]
//! returns `false` if the next record's scheduled time has not elapsed.
//! Sleeping is a caller concern.
//!
//! Non-monotonic timestamps (backward jumps) are treated as zero-gap
//! (emit immediately) unless validated externally.

use std::time::{Duration, Instant};

use flyby_core::{Error, Result};

// ---------------------------------------------------------------------------
// ReplayMode
// ---------------------------------------------------------------------------

/// How the replay engine controls record emission timing.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplayMode {
    /// Emit records as fast as the source can read them.
    FullSpeed,

    /// Honour the `timestamp_ns` field in each [`RecordMeta`][crate::batch::RecordMeta].
    OriginalTiming,

    /// Stretch or compress the original timing by `factor`.
    ///
    /// - `factor > 1.0` — slow down (e.g. 2.0 = half speed)
    /// - `factor < 1.0` — speed up (e.g. 0.5 = double speed)
    /// - `factor = 1.0` — identical to [`OriginalTiming`][Self::OriginalTiming]
    ///
    /// `factor` must be finite and `> 0`.
    TimeScaled {
        /// Timing multiplier.  Must be finite and positive.
        factor: f64,
    },

    /// Emit records in bursts of `count` then pause for `gap`.
    Burst {
        /// Records per burst (must be ≥ 1).
        count: usize,
        /// Wall-clock gap between bursts.
        gap: Duration,
    },

    /// Emit exactly one record per armed poll.
    ///
    /// After a successful `poll` returns `true`, subsequent polls return
    /// `false` until [`ReplayEngine::advance`] is called.
    SingleStep,
}

impl ReplayMode {
    /// Validate mode parameters.
    pub fn validate(&self) -> Result<()> {
        match self {
            ReplayMode::TimeScaled { factor } => {
                if !factor.is_finite() || *factor <= 0.0 {
                    return Err(Error::config("TimeScaled factor must be finite and > 0"));
                }
            }
            ReplayMode::Burst { count, .. } => {
                if *count == 0 {
                    return Err(Error::config("Burst count must be ≥ 1"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ReplayEngine
// ---------------------------------------------------------------------------

/// Controls timing for replaying records from a storage source.
pub struct ReplayEngine {
    mode: ReplayMode,
    /// Monotonic start time (set on first poll).
    start: Option<Instant>,
    /// Timestamp of the first record (nanoseconds); used to compute deltas.
    first_ts_ns: Option<u64>,
    /// Number of records emitted in the current burst window.
    burst_count: usize,
    /// Wall-clock time when the current burst pause ends.
    burst_resume: Option<Instant>,
    /// SingleStep: armed for the next emission.
    single_step_armed: bool,
}

impl ReplayEngine {
    /// Create a replay engine with the given mode.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::Config`][flyby_core::ErrorKind::Config] when
    /// mode parameters are invalid.
    pub fn new(mode: ReplayMode) -> Result<Self> {
        mode.validate()?;
        Ok(Self {
            mode,
            start: None,
            first_ts_ns: None,
            burst_count: 0,
            burst_resume: None,
            single_step_armed: true,
        })
    }

    /// Create without validation (for tests that intentionally use edge values).
    ///
    /// Prefer [`new`][Self::new] in production code.
    pub fn new_unchecked(mode: ReplayMode) -> Self {
        Self {
            mode,
            start: None,
            first_ts_ns: None,
            burst_count: 0,
            burst_resume: None,
            single_step_armed: true,
        }
    }

    /// Arm the next SingleStep emission. No-op for other modes.
    pub fn advance(&mut self) {
        if matches!(self.mode, ReplayMode::SingleStep) {
            self.single_step_armed = true;
        }
    }

    /// Query whether the next record (with the given `timestamp_ns`) should be
    /// emitted now.
    ///
    /// Returns `true` when the record is ready to be passed downstream.
    /// Returns `false` when the caller should wait and retry.
    pub fn poll(&mut self, timestamp_ns: u64) -> bool {
        let now = Instant::now();

        match &self.mode {
            ReplayMode::FullSpeed => true,

            ReplayMode::SingleStep => {
                if !self.single_step_armed {
                    return false;
                }
                self.single_step_armed = false;
                true
            }

            ReplayMode::OriginalTiming => self.original_timing_ready(now, timestamp_ns, 1.0),

            ReplayMode::TimeScaled { factor } => {
                let f = *factor;
                self.original_timing_ready(now, timestamp_ns, f)
            }

            ReplayMode::Burst { count, gap } => {
                let count = *count;
                let gap = *gap;
                self.burst_ready(now, count, gap)
            }
        }
    }

    fn original_timing_ready(&mut self, now: Instant, ts_ns: u64, factor: f64) -> bool {
        let start = *self.start.get_or_insert(now);
        let first_ts = *self.first_ts_ns.get_or_insert(ts_ns);

        if ts_ns <= first_ts {
            // First record, duplicate, or backward jump — emit immediately.
            return true;
        }

        let original_delta_ns = ts_ns.saturating_sub(first_ts);
        // Prefer integer math when factor is 1.0; otherwise scale carefully.
        let scaled_ns = if (factor - 1.0).abs() < f64::EPSILON {
            original_delta_ns
        } else {
            // Cap to avoid undefined Duration for huge products.
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
        let deadline = start + Duration::from_nanos(scaled_ns);
        now >= deadline
    }

    fn burst_ready(&mut self, now: Instant, burst_count: usize, gap: Duration) -> bool {
        if let Some(resume) = self.burst_resume {
            if now < resume {
                return false;
            }
            self.burst_resume = None;
            self.burst_count = 0;
        }

        self.burst_count += 1;
        if self.burst_count >= burst_count {
            self.burst_resume = Some(now + gap);
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
        let mut engine = ReplayEngine::new(ReplayMode::FullSpeed).unwrap();
        for ts in [0, 1000, 2000, 999_999_999] {
            assert!(engine.poll(ts), "full-speed should always return true");
        }
    }

    #[test]
    fn single_step_requires_advance() {
        let mut engine = ReplayEngine::new(ReplayMode::SingleStep).unwrap();
        assert!(engine.poll(0));
        assert!(!engine.poll(0), "second poll blocks until advance");
        engine.advance();
        assert!(engine.poll(0));
    }

    #[test]
    fn original_timing_first_record_immediate() {
        let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming).unwrap();
        assert!(engine.poll(1_000_000_000));
    }

    #[test]
    fn time_scaled_rejects_bad_factor() {
        assert!(ReplayEngine::new(ReplayMode::TimeScaled { factor: 0.0 }).is_err());
        assert!(ReplayEngine::new(ReplayMode::TimeScaled { factor: f64::NAN }).is_err());
    }

    #[test]
    fn time_scaled_first_record_immediate() {
        let mut engine = ReplayEngine::new(ReplayMode::TimeScaled { factor: 2.0 }).unwrap();
        assert!(engine.poll(500_000_000));
    }

    #[test]
    fn burst_emits_then_pauses() {
        let mut engine = ReplayEngine::new(ReplayMode::Burst {
            count: 2,
            gap: Duration::from_secs(3600),
        })
        .unwrap();
        assert!(engine.poll(0));
        assert!(engine.poll(0));
        assert!(!engine.poll(0));
    }

    #[test]
    fn burst_resumes_after_gap() {
        let gap = Duration::from_millis(0);
        let mut engine = ReplayEngine::new(ReplayMode::Burst { count: 1, gap }).unwrap();
        assert!(engine.poll(0));
        assert!(engine.poll(0));
    }
}
