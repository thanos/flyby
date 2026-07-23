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
//! | [`Scenario::gaussian_rate`] | Gaussian arrivals around 50 kpps |
//! | [`Scenario::protocol_quotes`] | Binary market-quote payloads |
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
/// without a builder.  Names are owned [`String`]s so scenarios can be
/// loaded from the FlyScenario DSL as well as built-in presets.
#[derive(Debug, Clone)]
pub struct Scenario {
    /// Short identifier (snake_case).
    pub name: String,
    /// Human-readable description.
    pub description: String,
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
            name: "default".into(),
            description: "Default scenario: 1 kpps, no faults, 1 second.".into(),
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
    pub fn constant_rate() -> Self {
        use crate::generator::PayloadSpec;
        use crate::traffic::TrafficPattern;
        Self {
            name: "constant_rate".into(),
            description: "Steady 100 kpps, no faults, 1 second virtual time.".into(),
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 100_000 },
                payload_size: 8,
                batch_size: 128,
                payload: PayloadSpec::FixedSeq,
            },
            fault: FaultSpec::default(),
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// 10 000-packet burst then 1 ms gap, 5 seconds virtual duration.
    pub fn market_open_burst() -> Self {
        Self {
            name: "market_open_burst".into(),
            description: "10k-packet bursts with 1 ms gap, 5 seconds.".into(),
            traffic: TrafficConfig::market_open_burst(),
            fault: FaultSpec::default(),
            duration: Duration::from_secs(5),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// High packet rate + small ring = measurable queue overflow.
    pub fn queue_overflow() -> Self {
        use crate::generator::PayloadSpec;
        use crate::traffic::TrafficPattern;
        Self {
            name: "queue_overflow".into(),
            description: "Saturating rate with a tiny ring to trigger overflow.".into(),
            traffic: TrafficConfig {
                pattern: TrafficPattern::FullSpeed,
                payload_size: 8,
                batch_size: 256,
                payload: PayloadSpec::FixedSeq,
            },
            fault: FaultSpec::default(),
            duration: Duration::from_millis(100),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 100_000,
        }
    }

    /// 5% random packet drop, 10 seconds, 10 kpps.
    pub fn packet_loss() -> Self {
        use crate::generator::PayloadSpec;
        use crate::traffic::TrafficPattern;
        Self {
            name: "packet_loss".into(),
            description: "10 kpps with 5% random drop rate.".into(),
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 10_000 },
                payload_size: 8,
                batch_size: 64,
                payload: PayloadSpec::FixedSeq,
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
    pub fn slow_consumer() -> Self {
        use crate::generator::PayloadSpec;
        use crate::traffic::TrafficPattern;
        Self {
            name: "slow_consumer".into(),
            description: "1 kpps with 10% 500 µs latency spikes.".into(),
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 1_000 },
                payload_size: 8,
                batch_size: 16,
                payload: PayloadSpec::FixedSeq,
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
        use crate::generator::PayloadSpec;
        use crate::traffic::TrafficPattern;
        Self {
            name: "corrupt_packets".into(),
            description: "1 kpps with 1% packet corruption rate.".into(),
            traffic: TrafficConfig {
                pattern: TrafficPattern::FixedRate { pps: 1_000 },
                payload_size: 32,
                batch_size: 16,
                payload: PayloadSpec::FixedSeq,
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

    /// Gaussian arrival process around 50 kpps for 1 virtual second.
    pub fn gaussian_rate() -> Self {
        Self {
            name: "gaussian_rate".into(),
            description: "Gaussian arrivals ~ N(50k, 10k) pps, 1 second.".into(),
            traffic: TrafficConfig::gaussian_rate(),
            fault: FaultSpec::default(),
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// Protocol-aware binary market quotes at 10 kpps.
    pub fn protocol_quotes() -> Self {
        Self {
            name: "protocol_quotes".into(),
            description: "10 kpps binary market-quote payloads (AAPL).".into(),
            traffic: TrafficConfig::protocol_quotes(),
            fault: FaultSpec::default(),
            duration: Duration::from_secs(1),
            clock_mode: ClockMode::Virtual { start_ns: 0 },
            tick_ns: 1_000_000,
        }
    }

    /// Resolve a built-in scenario by snake_case name.
    ///
    /// Returns `None` when the name is unknown.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "constant_rate" => Some(Self::constant_rate()),
            "market_open_burst" => Some(Self::market_open_burst()),
            "queue_overflow" => Some(Self::queue_overflow()),
            "packet_loss" => Some(Self::packet_loss()),
            "slow_consumer" => Some(Self::slow_consumer()),
            "corrupt_packets" => Some(Self::corrupt_packets()),
            "gaussian_rate" => Some(Self::gaussian_rate()),
            "protocol_quotes" => Some(Self::protocol_quotes()),
            "default" => Some(Self::default()),
            _ => None,
        }
    }

    /// Names of all built-in named scenarios (excludes `"default"`).
    pub fn builtin_names() -> &'static [&'static str] {
        &[
            "constant_rate",
            "market_open_burst",
            "queue_overflow",
            "packet_loss",
            "slow_consumer",
            "corrupt_packets",
            "gaussian_rate",
            "protocol_quotes",
        ]
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
            Scenario::gaussian_rate(),
            Scenario::protocol_quotes(),
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
    fn by_name_resolves_builtins() {
        for name in Scenario::builtin_names() {
            let s = Scenario::by_name(name).expect(name);
            assert_eq!(s.name, *name); // String == &str
        }
        assert!(Scenario::by_name("nope").is_none());
    }
}
