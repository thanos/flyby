//! Timed actions applied by [`SimScheduler`][crate::scheduler::SimScheduler]
//! during a run (FlyScenario DSL `[[timeline]]` and Rhai `[script]`).

use crate::fault::FaultSpec;
use crate::traffic::TrafficConfig;

/// A scheduled mutation applied when virtual time reaches `at_ns`.
#[derive(Debug, Clone)]
pub enum TimelineAction {
    /// Replace traffic on a named NIC / source.
    SetTraffic {
        /// Virtual time (nanoseconds) when the action fires.
        at_ns: u64,
        /// Target source name.
        nic: String,
        /// New traffic configuration.
        traffic: TrafficConfig,
    },
    /// Replace fault policy on a named NIC / source.
    SetFault {
        /// Virtual time (nanoseconds) when the action fires.
        at_ns: u64,
        /// Target source name.
        nic: String,
        /// New fault specification.
        fault: FaultSpec,
    },
    /// Change a consumer's per-drain budget.
    SlowConsumer {
        /// Virtual time (nanoseconds) when the action fires.
        at_ns: u64,
        /// Target consumer name.
        consumer: String,
        /// New max slots per drain (`usize::MAX` = unlimited).
        max_per_drain: usize,
    },
}

impl TimelineAction {
    /// Virtual time at which this action should fire.
    pub fn at_ns(&self) -> u64 {
        match self {
            Self::SetTraffic { at_ns, .. }
            | Self::SetFault { at_ns, .. }
            | Self::SlowConsumer { at_ns, .. } => *at_ns,
        }
    }
}
