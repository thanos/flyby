//! Simulator clock: real time or deterministic virtual time.
//!
//! ## Two modes
//!
//! | Mode | Description |
//! |---|---|
//! | [`ClockMode::RealTime`] | Wraps [`std::time::Instant`]; wall-clock behaviour |
//! | [`ClockMode::Virtual`] | Tick-driven; time only advances when [`SimClock::advance`] is called |
//!
//! Virtual time is the key to deterministic simulation: all components share
//! one clock, no system-call timing jitter enters the loop, and tests always
//! produce the same sequence of events regardless of machine load.
//!
//! ## Usage
//!
//! ```rust
//! use flyby_simulator::clock::{ClockMode, SimClock};
//! use std::time::Duration;
//!
//! let mut clock = SimClock::new(ClockMode::Virtual { start_ns: 0 });
//! assert_eq!(clock.now_ns(), 0);
//! clock.advance(Duration::from_millis(1));
//! assert_eq!(clock.now_ns(), 1_000_000);
//! ```

use std::time::{Duration, Instant};

/// How the simulator clock tracks time.
#[derive(Debug, Clone)]
pub enum ClockMode {
    /// Use the system monotonic clock. Time advances naturally.
    RealTime,
    /// Fully controlled virtual time.  Time starts at `start_ns` nanoseconds
    /// since an arbitrary epoch and only advances through explicit calls to
    /// [`SimClock::advance`].
    Virtual {
        /// Initial clock value in nanoseconds.
        start_ns: u64,
    },
}

/// The simulator's unified clock.
///
/// All virtual components — NICs, storage, schedulers — read time from a
/// shared `SimClock`.  In virtual mode the scheduler owns the clock and
/// advances it each tick; components call [`now_ns`][Self::now_ns] to
/// observe the current time.
#[derive(Debug)]
pub struct SimClock {
    mode: ClockMode,
    /// Virtual time in nanoseconds (only used in `Virtual` mode).
    virtual_ns: u64,
    /// Wall-clock anchor for real-time mode (set at construction).
    real_start: Instant,
}

impl SimClock {
    /// Create a clock in the given mode.
    pub fn new(mode: ClockMode) -> Self {
        let virtual_ns = match &mode {
            ClockMode::Virtual { start_ns } => *start_ns,
            ClockMode::RealTime => 0,
        };
        Self { mode, virtual_ns, real_start: Instant::now() }
    }

    /// Current time in nanoseconds.
    ///
    /// In real-time mode this is nanoseconds elapsed since the clock was created.
    /// In virtual mode this is the accumulated virtual time.
    pub fn now_ns(&self) -> u64 {
        match self.mode {
            ClockMode::RealTime => {
                self.real_start.elapsed().as_nanos() as u64
            }
            ClockMode::Virtual { .. } => self.virtual_ns,
        }
    }

    /// Advance virtual time by `delta`.
    ///
    /// No-op in real-time mode (time advances on its own).
    pub fn advance(&mut self, delta: Duration) {
        if matches!(self.mode, ClockMode::Virtual { .. }) {
            self.virtual_ns = self.virtual_ns.saturating_add(delta.as_nanos() as u64);
        }
    }

    /// Advance virtual time by `ns` nanoseconds.
    ///
    /// No-op in real-time mode.
    pub fn advance_ns(&mut self, ns: u64) {
        if matches!(self.mode, ClockMode::Virtual { .. }) {
            self.virtual_ns = self.virtual_ns.saturating_add(ns);
        }
    }

    /// `true` if the clock is running in virtual (deterministic) mode.
    pub fn is_virtual(&self) -> bool {
        matches!(self.mode, ClockMode::Virtual { .. })
    }
}

impl Clone for SimClock {
    fn clone(&self) -> Self {
        Self {
            mode: self.mode.clone(),
            virtual_ns: self.virtual_ns,
            real_start: self.real_start,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_clock_starts_at_given_time() {
        let c = SimClock::new(ClockMode::Virtual { start_ns: 1_000_000 });
        assert_eq!(c.now_ns(), 1_000_000);
    }

    #[test]
    fn virtual_clock_advances_by_duration() {
        let mut c = SimClock::new(ClockMode::Virtual { start_ns: 0 });
        c.advance(Duration::from_millis(5));
        assert_eq!(c.now_ns(), 5_000_000);
    }

    #[test]
    fn virtual_clock_advance_ns() {
        let mut c = SimClock::new(ClockMode::Virtual { start_ns: 100 });
        c.advance_ns(900);
        assert_eq!(c.now_ns(), 1_000);
    }

    #[test]
    fn virtual_clock_is_virtual() {
        let c = SimClock::new(ClockMode::Virtual { start_ns: 0 });
        assert!(c.is_virtual());
    }

    #[test]
    fn real_time_clock_is_not_virtual() {
        let c = SimClock::new(ClockMode::RealTime);
        assert!(!c.is_virtual());
    }

    #[test]
    fn real_time_clock_advance_is_noop() {
        let mut c = SimClock::new(ClockMode::RealTime);
        let before = c.now_ns();
        c.advance(Duration::from_secs(100)); // no effect
        let after = c.now_ns();
        // Real-time should not jump by 100 seconds; virtual_ns unchanged
        assert!(after - before < 1_000_000_000, "real-time advance should be a no-op");
    }

    #[test]
    fn multiple_advances_accumulate() {
        let mut c = SimClock::new(ClockMode::Virtual { start_ns: 0 });
        for _ in 0..10 {
            c.advance_ns(100);
        }
        assert_eq!(c.now_ns(), 1_000);
    }
}
