//! Replay engine.
//!
//! The replay engine sits between a [`StorageSource`][crate::source::StorageSource]
//! and the pipeline and controls the timing with which records are released.
//!
//! ## Modes
//!
//! | Mode | Description |
//! |---|---|
//! | [`ReplayMode::FullSpeed`] | No throttling; emit as fast as the source reads |
//! | [`ReplayMode::OriginalTiming`] | Honour the per-record timestamp; reconstruct original inter-arrival gaps |
//! | [`ReplayMode::TimeScaled`] | Stretch or compress the original timing by a factor |
//! | [`ReplayMode::Burst`] | Emit a fixed burst of records then idle for a gap |
//! | [`ReplayMode::SingleStep`] | Emit exactly one record per [`poll`][ReplayEngine::poll] call |
//!
//! ## Why replay is a first-class feature
//!
//! - **Deterministic tests**: a file replay always produces the same sequence
//!   of records, making CI tests independent of network availability.
//! - **Parser development**: replay a captured trace against new parser code
//!   without touching production infrastructure.
//! - **Benchmarks**: drive the pipeline at controlled rates to isolate
//!   per-component latency.
//!
//! ## Timing implementation
//!
//! The engine uses the monotonic clock ([`std::time::Instant`]) to track wall
//! time; it does **not** call `sleep`.  Instead, [`poll`][ReplayEngine::poll]
//! returns `None` (nothing ready yet) if the next record's scheduled time has
//! not elapsed.  The caller is expected to spin or yield between polls.
//! Sleeping is a caller concern; the engine is not responsible for it.

use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// ReplayMode
// ---------------------------------------------------------------------------

/// How the replay engine controls record emission timing.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplayMode {
    /// Emit records as fast as the source can read them.
    ///
    /// Use for benchmarks and for sources that do not embed timestamps.
    FullSpeed,

    /// Honour the `timestamp_ns` field in each [`RecordMeta`][crate::batch::RecordMeta].
    ///
    /// Reconstructs the original inter-arrival timing.  The first record is
    /// emitted immediately; subsequent records are held until the elapsed wall
    /// time since start matches the original elapsed time between the first
    /// and current record's timestamps.
    OriginalTiming,

    /// Stretch or compress the original timing by `factor`.
    ///
    /// - `factor > 1.0` — slow down (e.g. 2.0 = half speed)
    /// - `factor < 1.0` — speed up (e.g. 0.5 = double speed)
    /// - `factor = 1.0` — identical to [`OriginalTiming`][Self::OriginalTiming]
    ///
    /// Requires `timestamp_ns` to be non-zero in each record.
    TimeScaled {
        /// Timing multiplier.  Must be finite and positive.
        factor: f64,
    },

    /// Emit records in bursts of `count` then pause for `gap`.
    ///
    /// Useful for simulating bursty traffic patterns in benchmarks.
    Burst {
        /// Records per burst.
        count: usize,
        /// Wall-clock gap between bursts.
        gap: Duration,
    },

    /// Emit exactly one record per [`poll`][ReplayEngine::poll] call.
    ///
    /// Useful for step-through debugging or very-low-rate integration tests.
    SingleStep,
}

// ---------------------------------------------------------------------------
// ReplayEngine
// ---------------------------------------------------------------------------

/// Controls timing for replaying records from a storage source.
///
/// Created with a [`ReplayMode`] and driven by repeated calls to
/// [`poll`][Self::poll].  The engine tracks wall time and the origin
/// timestamps embedded in records to compute when each record should be
/// released.
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
}

impl ReplayEngine {
    /// Create a replay engine with the given mode.
    pub fn new(mode: ReplayMode) -> Self {
        Self { mode, start: None, first_ts_ns: None, burst_count: 0, burst_resume: None }
    }

    /// Query whether the next record (with the given `timestamp_ns`) should be
    /// emitted now.
    ///
    /// Returns `true` when the record is ready to be passed downstream.
    /// Returns `false` when the caller should wait and retry.
    ///
    /// The engine initialises internal state on the first call; subsequent
    /// calls must pass monotonically non-decreasing `timestamp_ns` values when
    /// using timestamp-sensitive modes.
    pub fn poll(&mut self, timestamp_ns: u64) -> bool {
        let now = Instant::now();

        match &self.mode {
            ReplayMode::FullSpeed => true,

            ReplayMode::SingleStep => true,

            ReplayMode::OriginalTiming => {
                self.original_timing_ready(now, timestamp_ns, 1.0)
            }

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
            // First record (or duplicate timestamp) — emit immediately.
            return true;
        }

        let original_delta_ns = ts_ns.saturating_sub(first_ts);
        let scaled_ns = (original_delta_ns as f64 * factor) as u64;
        let deadline = start + Duration::from_nanos(scaled_ns);
        now >= deadline
    }

    fn burst_ready(&mut self, now: Instant, burst_count: usize, gap: Duration) -> bool {
        // Check if we're in a pause.
        if let Some(resume) = self.burst_resume {
            if now < resume {
                return false;
            }
            // Pause is over.
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
        let mut engine = ReplayEngine::new(ReplayMode::FullSpeed);
        for ts in [0, 1000, 2000, 999_999_999] {
            assert!(engine.poll(ts), "full-speed should always return true");
        }
    }

    #[test]
    fn single_step_always_ready() {
        let mut engine = ReplayEngine::new(ReplayMode::SingleStep);
        assert!(engine.poll(0));
        assert!(engine.poll(0));
    }

    #[test]
    fn original_timing_first_record_immediate() {
        let mut engine = ReplayEngine::new(ReplayMode::OriginalTiming);
        assert!(engine.poll(1_000_000_000));
    }

    #[test]
    fn time_scaled_first_record_immediate() {
        let mut engine = ReplayEngine::new(ReplayMode::TimeScaled { factor: 2.0 });
        assert!(engine.poll(500_000_000));
    }

    #[test]
    fn burst_emits_then_pauses() {
        let mut engine =
            ReplayEngine::new(ReplayMode::Burst { count: 2, gap: Duration::from_secs(3600) });
        assert!(engine.poll(0)); // record 1 in burst
        assert!(engine.poll(0)); // record 2 in burst — triggers pause
        assert!(!engine.poll(0)); // still paused
    }

    #[test]
    fn burst_resumes_after_gap() {
        let gap = Duration::from_millis(0); // zero gap — resumes immediately
        let mut engine = ReplayEngine::new(ReplayMode::Burst { count: 1, gap });
        assert!(engine.poll(0)); // triggers pause with 0 gap
        // With a zero gap the resume instant is in the past; next poll should emit.
        assert!(engine.poll(0));
    }
}
