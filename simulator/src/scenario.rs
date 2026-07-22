//! Scenario engine: declarative descriptions of simulator behaviour.
//!
//! A [`Scenario`] describes a complete simulation run — the virtual hardware,
//! traffic pattern, fault injection policy, and run duration.  Scenarios are
//! version-controlled alongside the codebase so benchmark results are always
//! tied to a specific configuration.
//!
//! ## Built-in scenarios
//!
//! | Scenario | Description |
//! |---|---|
//! | [`Scenario::constant_rate`] | Steady 100 kpps, no faults |
//! | [`Scenario::market_open_burst`] | 10 000-packet burst then 1 ms gap |
//! | [`Scenario::queue_overflow`] | High rate + tight ring → observable drops |
//! | [`Scenario::packet_loss`] | 5% random drop rate |
//! | [`Scenario::slow_consumer`] | Low rate, high latency spikes |
//!
//! ## Usage
//!
//! ```rust
//! use flyby_simulator::scenario::Scenario;
//!
//! let s = Scenario::constant_rate();
//! println!("Running: {} — {}", s.name, s.description);
//! ```

use crate::clock::ClockMode;
use crate::fault::FaultSpec;
use crate::traffic::TrafficConfig;
use std::time::Duration;

/// A complete simulation scenario.
///
/// All fields are `pub` so scenarios can be created and modified inline
/// without a builder.
#[derive(Debug, Clone)]
pub struct Scenario {
    /// Short identifier (snake_case).
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Traffic generation configuration.
    pub traffic: TrafficConfig,
    /// Fault injection policy.
    pub fault: FaultSpec,
    /// How long to run the simulation.
    pub duration: Duration,
    /// Clock mode for the run.
    pub clock_mode: ClockMode,
    /// Scheduler tick interval.  Smaller = more resolution, more overhead.
    pub tick_ns: u64,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            name: "default",
            description: "Default scenario: 1 kpps, no faults, 1 second.",
            traffic: TrafficConfig::default(),
            fault: FaultSpec::default(),
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000, // 1 ms ticks
        }
    }
}

impl Scenario {
    /// Steady 100 kpps traffic, no faults, 1 second virtual duration.
    ///
    /// Baseline scenario for throughput benchmarks.
    pub fn constant_rate() -> Self {
        use crate::traffic::TrafficPattern;
        Self {
            name: "constant_rate",
            description: "Steady 100 kpps, no faults, 1 second virtual time.",
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 100_000 },
                payload_size: 8,
                batch_size: 128,
            },
            fault: FaultSpec::default(),
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// 10 000-packet burst then 1 ms gap, 5 seconds virtual duration.
    ///
    /// Models a market-open auction: a large burst of order messages followed
    /// by normal market-data flow.
    pub fn market_open_burst() -> Self {
        Self {
            name: "market_open_burst",
            description: "10k-packet bursts with 1 ms gap, 5 seconds.",
            traffic: TrafficConfig::market_open_burst(),
            fault: FaultSpec::default(),
            duration: Duration::from_secs(5),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// High packet rate + small ring = measurable queue overflow.
    ///
    /// Use this to verify that drop counters and `QueueOverflow` events are
    /// wired correctly.
    pub fn queue_overflow() -> Self {
        use crate::traffic::TrafficPattern;
        Self {
            name: "queue_overflow",
            description: "Saturating rate with a tiny ring to trigger overflow.",
            traffic: TrafficConfig {
                pattern: TrafficPattern::FullSpeed,
                payload_size: 8,
                batch_size: 256,
            },
            fault: FaultSpec::default(),
            duration: Duration::from_millis(100),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 100_000,
        }
    }

    /// 5% random packet drop, 10 seconds, 10 kpps.
    pub fn packet_loss() -> Self {
        use crate::traffic::TrafficPattern;
        Self {
            name: "packet_loss",
            description: "10 kpps with 5% random drop rate.",
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 10_000 },
                payload_size: 8,
                batch_size: 64,
            },
            fault: FaultSpec {
                drop_rate: 0.05,
                ..FaultSpec::default()
            },
            duration: Duration::from_secs(10),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// Low rate with 10% probability of a 500 µs latency spike.
    ///
    /// Models a slow or intermittently stalled consumer, e.g. one that
    /// performs disk I/O on the critical path.
    pub fn slow_consumer() -> Self {
        use crate::traffic::TrafficPattern;
        Self {
            name: "slow_consumer",
            description: "1 kpps with 10% 500 µs latency spikes.",
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 1_000 },
                payload_size: 8,
                batch_size: 16,
            },
            fault: FaultSpec {
                latency_spike_rate: 0.10,
                latency_spike_ns: 500_000,
                ..FaultSpec::default()
            },
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// Corrupt 1% of packets.  Useful to test parser error handling.
    pub fn corrupt_packets() -> Self {
        use crate::traffic::TrafficPattern;
        Self {
            name: "corrupt_packets",
            description: "1 kpps with 1% packet corruption rate.",
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 1_000 },
                payload_size: 32,
                batch_size: 16,
            },
            fault: FaultSpec {
                corrupt_rate: 0.01,
                ..FaultSpec::default()
            },
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_rate_has_no_faults() {
        let s = Scenario::constant_rate();
        assert!(s.fault.is_clean());
    }

    #[test]
    fn packet_loss_has_nonzero_drop_rate() {
        let s = Scenario::packet_loss();
        assert!(s.fault.drop_rate > 0.0);
    }

    #[test]
    fn slow_consumer_has_latency_spike() {
        let s = Scenario::slow_consumer();
        assert!(s.fault.latency_spike_ns > 0);
        assert!(s.fault.latency_spike_rate > 0.0);
    }

    #[test]
    fn all_scenarios_have_nonempty_names() {
        let scenarios = [
            Scenario::default(),
            Scenario::constant_rate(),
            Scenario::market_open_burst(),
            Scenario::queue_overflow(),
            Scenario::packet_loss(),
            Scenario::slow_consumer(),
            Scenario::corrupt_packets(),
        ];
        for s in &scenarios {
            assert!(!s.name.is_empty(), "scenario name must not be empty");
            assert!(
                !s.description.is_empty(),
                "scenario description must not be empty"
            );
        }
    }

    #[test]
    fn all_scenarios_use_virtual_time() {
        let scenarios = [
            Scenario::default(),
            Scenario::constant_rate(),
            Scenario::market_open_burst(),
        ];
        for s in scenarios {
            assert!(
                matches!(s.clock_mode, ClockMode::Virtual { .. }),
                "scenario '{}' should use virtual time",
                s.name
            );
        }
    }
}
