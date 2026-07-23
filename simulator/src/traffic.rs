//! Traffic patterns for virtual NICs.
//!
//! A [`TrafficPattern`] controls the *rate* at which packets are released
//! by a [`VirtualNic`][crate::nic::VirtualNic].  The pattern is evaluated
//! per scheduler tick via [`TrafficPacer`]; the tick duration and the
//! pattern together determine how many packets fire each tick.
//!
//! Payload *content* is configured separately via [`PayloadSpec`].
//!
//! ## Patterns
//!
//! | Pattern | Description |
//! |---|---|
//! | [`TrafficPattern::FixedRate`] | Constant packets/second |
//! | [`TrafficPattern::Burst`] | N packets then a gap |
//! | [`TrafficPattern::Gaussian`] | Packets/tick ~ Normal(mean, std) |
//! | [`TrafficPattern::FullSpeed`] | As fast as possible |

use std::time::Duration;

use crate::generator::PayloadSpec;

/// How the virtual NIC paces packet delivery.
#[derive(Debug, Clone, PartialEq)]
pub enum TrafficPattern {
    /// Emit packets at a constant rate.
    ///
    /// The scheduler converts `pps` to a per-tick count based on the tick
    /// duration.
    FixedRate {
        /// Target packets per second.
        pps: u64,
    },

    /// Emit `burst_size` packets then pause for `gap`.
    ///
    /// Simulates bursty traffic such as a market-open auction or a batch
    /// feed arriving in chunks.  Large bursts span multiple ticks when
    /// `burst_size` exceeds the configured batch size.
    Burst {
        /// Packets per burst.
        burst_size: usize,
        /// Pause between bursts.
        gap: Duration,
    },

    /// Sample packets-per-tick from a Normal(`mean_pps * tick_s`, `std_pps * tick_s`)
    /// distribution (Box–Muller, LCG-seeded).
    ///
    /// Samples are clamped to `[0, batch_size]`.
    Gaussian {
        /// Mean packets per second.
        mean_pps: f64,
        /// Standard deviation of packets per second.
        std_pps: f64,
        /// LCG seed for reproducible sampling.
        seed: u64,
    },

    /// Emit as many packets as the batch allows every tick.
    ///
    /// Use for throughput benchmarks where you want to saturate the pipeline.
    FullSpeed,
}

impl Default for TrafficPattern {
    fn default() -> Self {
        Self::FixedRate { pps: 1_000 }
    }
}

/// Configuration for the packet generator inside a [`VirtualNic`][crate::nic::VirtualNic].
#[derive(Debug, Clone)]
pub struct TrafficConfig {
    /// Timing pattern.
    pub pattern: TrafficPattern,
    /// Payload bytes per packet (excluding the 42-byte Ethernet/IP/UDP header).
    ///
    /// Default: 8 bytes (a u64 sequence number).  For
    /// [`PayloadSpec::GaussianSize`] the generator may use a larger `max`.
    pub payload_size: usize,
    /// Maximum batch size.  The NIC never delivers more than this many packets
    /// in one [`poll_batch`][flyby_net::NetworkSource::poll_batch] call.
    pub batch_size: usize,
    /// How payload bytes are filled.
    pub payload: PayloadSpec,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            pattern: TrafficPattern::default(),
            payload_size: 8,
            batch_size: 64,
            payload: PayloadSpec::FixedSeq,
        }
    }
}

impl PartialEq for TrafficConfig {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
            && self.payload_size == other.payload_size
            && self.batch_size == other.batch_size
            && self.payload == other.payload
    }
}

impl TrafficConfig {
    /// Stateless estimate of packets for a tick (ignores burst/gaussian phase).
    ///
    /// Prefer [`TrafficPacer::packets_for_tick`] for live simulation.
    pub fn packets_for_tick(&self, tick_ns: u64) -> usize {
        match &self.pattern {
            TrafficPattern::FullSpeed => self.batch_size,
            TrafficPattern::FixedRate { pps } => {
                if *pps == 0 {
                    return 0;
                }
                let ns_per_packet = 1_000_000_000u64 / pps;
                if ns_per_packet == 0 {
                    return self.batch_size;
                }
                let count = tick_ns / ns_per_packet;
                (count as usize).min(self.batch_size)
            }
            TrafficPattern::Burst { burst_size, .. } => (*burst_size).min(self.batch_size),
            TrafficPattern::Gaussian { mean_pps, .. } => {
                let expected = mean_pps * (tick_ns as f64) / 1_000_000_000.0;
                (expected.round().max(0.0) as usize).min(self.batch_size)
            }
        }
    }

    /// 1 Mpps fixed-rate, 64-byte packets (typical small-packet benchmark).
    pub fn market_data() -> Self {
        Self {
            pattern: TrafficPattern::FixedRate { pps: 1_000_000 },
            payload_size: 22, // 64 bytes total with 42-byte net header
            batch_size: 64,
            payload: PayloadSpec::FixedSeq,
        }
    }

    /// A single burst of 10 000 packets followed by a 1 ms gap.
    pub fn market_open_burst() -> Self {
        Self {
            pattern: TrafficPattern::Burst {
                burst_size: 10_000,
                gap: Duration::from_millis(1),
            },
            payload_size: 32,
            batch_size: 256,
            payload: PayloadSpec::FixedSeq,
        }
    }

    /// Full-speed saturation test, 1500-byte frames.
    pub fn saturation() -> Self {
        Self {
            pattern: TrafficPattern::FullSpeed,
            payload_size: 1458, // 1500-byte Ethernet frame
            batch_size: 256,
            payload: PayloadSpec::FixedSeq,
        }
    }

    /// Gaussian arrival rate around 50 kpps.
    pub fn gaussian_rate() -> Self {
        Self {
            pattern: TrafficPattern::Gaussian {
                mean_pps: 50_000.0,
                std_pps: 10_000.0,
                seed: 42,
            },
            payload_size: 32,
            batch_size: 128,
            payload: PayloadSpec::FixedSeq,
        }
    }

    /// Protocol-aware binary market quotes at 10 kpps.
    pub fn protocol_quotes() -> Self {
        use crate::generator::ProtocolMessage;
        Self {
            pattern: TrafficPattern::FixedRate { pps: 10_000 },
            payload_size: 34,
            batch_size: 64,
            payload: PayloadSpec::Protocol(ProtocolMessage::market_quote("AAPL")),
        }
    }
}

/// Stateful traffic pacer that tracks burst gaps and Gaussian sampling.
#[derive(Debug, Clone)]
pub struct TrafficPacer {
    config: TrafficConfig,
    /// Packets still owed in the current burst (Burst pattern only).
    burst_remaining: usize,
    /// Nanoseconds remaining in the inter-burst gap.
    gap_remaining_ns: u64,
    /// Fractional nanoseconds carried for FixedRate accuracy.
    fixed_carry_ns: u64,
    /// LCG state for Gaussian sampling.
    gaussian_rng: u64,
    /// Spare Box–Muller sample.
    gaussian_spare: Option<f64>,
}

impl TrafficPacer {
    /// Create a pacer from a traffic configuration.
    pub fn new(config: TrafficConfig) -> Self {
        let burst_remaining = match &config.pattern {
            TrafficPattern::Burst { burst_size, .. } => *burst_size,
            _ => 0,
        };
        let gaussian_rng = match &config.pattern {
            TrafficPattern::Gaussian { seed, .. } => *seed,
            _ => 0,
        };
        Self {
            config,
            burst_remaining,
            gap_remaining_ns: 0,
            fixed_carry_ns: 0,
            gaussian_rng,
            gaussian_spare: None,
        }
    }

    /// Borrow the underlying config.
    pub fn config(&self) -> &TrafficConfig {
        &self.config
    }

    /// How many packets to emit for a tick of `tick_ns` nanoseconds.
    pub fn packets_for_tick(&mut self, tick_ns: u64) -> usize {
        let batch_size = self.config.batch_size;
        match self.config.pattern.clone() {
            TrafficPattern::FullSpeed => batch_size,
            TrafficPattern::FixedRate { pps } => {
                if pps == 0 {
                    return 0;
                }
                let budget = self.fixed_carry_ns.saturating_add(tick_ns);
                let ns_per_packet = 1_000_000_000u64 / pps;
                if ns_per_packet == 0 {
                    self.fixed_carry_ns = 0;
                    return batch_size;
                }
                let count = budget / ns_per_packet;
                self.fixed_carry_ns = budget % ns_per_packet;
                (count as usize).min(batch_size)
            }
            TrafficPattern::Burst { burst_size, gap } => {
                let gap_ns = gap.as_nanos() as u64;

                if self.gap_remaining_ns > 0 {
                    self.gap_remaining_ns = self.gap_remaining_ns.saturating_sub(tick_ns);
                    return 0;
                }

                if self.burst_remaining == 0 {
                    self.burst_remaining = burst_size;
                }

                let n = self.burst_remaining.min(batch_size);
                self.burst_remaining -= n;
                if self.burst_remaining == 0 {
                    self.gap_remaining_ns = gap_ns;
                }
                n
            }
            TrafficPattern::Gaussian {
                mean_pps, std_pps, ..
            } => {
                let tick_s = tick_ns as f64 / 1_000_000_000.0;
                let mean = mean_pps * tick_s;
                let std = std_pps * tick_s;
                let sample = self.sample_gaussian(mean, std);
                (sample.round().max(0.0) as usize).min(batch_size)
            }
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.gaussian_rng = self
            .gaussian_rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.gaussian_rng
    }

    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    fn sample_gaussian(&mut self, mean: f64, std_dev: f64) -> f64 {
        if let Some(spare) = self.gaussian_spare.take() {
            return mean + std_dev * spare;
        }
        let u1 = self.next_unit().clamp(f64::EPSILON, 1.0);
        let u2 = self.next_unit().clamp(f64::EPSILON, 1.0);
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        self.gaussian_spare = Some(r * theta.sin());
        mean + std_dev * (r * theta.cos())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_rate_1kpps_1ms_tick_yields_1_packet() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::FixedRate { pps: 1_000 },
            batch_size: 64,
            ..TrafficConfig::default()
        };
        assert_eq!(cfg.packets_for_tick(1_000_000), 1);
    }

    #[test]
    fn fixed_rate_1mpps_1ms_tick_yields_1000_capped_at_batch() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::FixedRate { pps: 1_000_000 },
            batch_size: 64,
            ..TrafficConfig::default()
        };
        assert_eq!(cfg.packets_for_tick(1_000_000), 64);
    }

    #[test]
    fn full_speed_always_returns_batch_size() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::FullSpeed,
            batch_size: 32,
            ..TrafficConfig::default()
        };
        assert_eq!(cfg.packets_for_tick(1), 32);
        assert_eq!(cfg.packets_for_tick(u64::MAX), 32);
    }

    #[test]
    fn burst_returns_burst_size_capped_at_batch() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::Burst {
                burst_size: 10,
                gap: Duration::from_millis(10),
            },
            batch_size: 8,
            ..TrafficConfig::default()
        };
        assert_eq!(cfg.packets_for_tick(1_000_000), 8);
    }

    #[test]
    fn zero_pps_returns_zero() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::FixedRate { pps: 0 },
            batch_size: 64,
            ..TrafficConfig::default()
        };
        assert_eq!(cfg.packets_for_tick(1_000_000_000), 0);
    }

    #[test]
    fn market_data_preset_has_expected_pps() {
        let cfg = TrafficConfig::market_data();
        assert!(matches!(
            cfg.pattern,
            TrafficPattern::FixedRate { pps: 1_000_000 }
        ));
    }

    #[test]
    fn saturation_preset_is_full_speed() {
        let cfg = TrafficConfig::saturation();
        assert!(matches!(cfg.pattern, TrafficPattern::FullSpeed));
    }

    #[test]
    fn pacer_burst_then_gap_then_burst() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::Burst {
                burst_size: 10,
                gap: Duration::from_millis(2),
            },
            batch_size: 4,
            payload_size: 8,
            payload: PayloadSpec::FixedSeq,
        };
        let mut pacer = TrafficPacer::new(cfg);
        assert_eq!(pacer.packets_for_tick(1_000_000), 4);
        assert_eq!(pacer.packets_for_tick(1_000_000), 4);
        assert_eq!(pacer.packets_for_tick(1_000_000), 2);
        assert_eq!(pacer.packets_for_tick(1_000_000), 0);
        assert_eq!(pacer.packets_for_tick(1_000_000), 0);
        assert_eq!(pacer.packets_for_tick(1_000_000), 4);
    }

    #[test]
    fn pacer_fixed_rate_accumulates_fractional_ticks() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::FixedRate { pps: 500 },
            batch_size: 64,
            payload_size: 8,
            payload: PayloadSpec::FixedSeq,
        };
        let mut pacer = TrafficPacer::new(cfg);
        let mut total = 0usize;
        for _ in 0..10 {
            total += pacer.packets_for_tick(1_000_000);
        }
        assert_eq!(total, 5);
    }

    #[test]
    fn gaussian_pacer_is_deterministic_and_bounded() {
        let cfg = TrafficConfig {
            pattern: TrafficPattern::Gaussian {
                mean_pps: 10_000.0,
                std_pps: 2_000.0,
                seed: 99,
            },
            batch_size: 32,
            payload_size: 8,
            payload: PayloadSpec::FixedSeq,
        };
        let mut a = TrafficPacer::new(cfg.clone());
        let mut b = TrafficPacer::new(cfg);
        let mut total = 0usize;
        for _ in 0..100 {
            let na = a.packets_for_tick(1_000_000);
            let nb = b.packets_for_tick(1_000_000);
            assert_eq!(na, nb);
            assert!(na <= 32);
            total += na;
        }
        // Roughly 10 packets/tick × 100 ≈ 1000; allow wide band.
        assert!(total > 500 && total < 1500, "total={total}");
    }
}
