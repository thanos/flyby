//! Traffic patterns for virtual NICs.
//!
//! A [`TrafficPattern`] controls the *rate* at which packets are released
//! by a [`VirtualNic`][crate::nic::VirtualNic].  The pattern is evaluated
//! per scheduler tick via [`TrafficPacer`]; the tick duration and the
//! pattern together determine how many packets fire each tick.
//!
//! ## Patterns
//!
//! | Pattern | Description |
//! |---|---|
//! | [`TrafficPattern::FixedRate`] | Constant packets/second |
//! | [`TrafficPattern::Burst`] | N packets then a gap |
//! | [`TrafficPattern::FullSpeed`] | As fast as possible |

use std::time::Duration;

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
    /// Default: 8 bytes (a u64 sequence number).
    pub payload_size: usize,
    /// Maximum batch size.  The NIC never delivers more than this many packets
    /// in one [`poll_batch`][flyby_net::NetworkSource::poll_batch] call.
    pub batch_size: usize,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            pattern: TrafficPattern::default(),
            payload_size: 8,
            batch_size: 64,
        }
    }
}

impl TrafficConfig {
    /// Stateless estimate of packets for a tick (ignores burst phase).
    ///
    /// Prefer [`TrafficPacer::packets_for_tick`] for live simulation — it
    /// tracks burst gaps correctly across ticks.
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
        }
    }

    /// 1 Mpps fixed-rate, 64-byte packets (typical small-packet benchmark).
    pub fn market_data() -> Self {
        Self {
            pattern: TrafficPattern::FixedRate { pps: 1_000_000 },
            payload_size: 22, // 64 bytes total with 42-byte net header
            batch_size: 64,
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
        }
    }

    /// Full-speed saturation test, 1500-byte frames.
    pub fn saturation() -> Self {
        Self {
            pattern: TrafficPattern::FullSpeed,
            payload_size: 1458, // 1500-byte Ethernet frame
            batch_size: 256,
        }
    }
}

/// Stateful traffic pacer that tracks burst gaps across ticks.
#[derive(Debug, Clone)]
pub struct TrafficPacer {
    config: TrafficConfig,
    /// Packets still owed in the current burst (Burst pattern only).
    burst_remaining: usize,
    /// Nanoseconds remaining in the inter-burst gap.
    gap_remaining_ns: u64,
    /// Fractional nanoseconds carried for FixedRate accuracy.
    fixed_carry_ns: u64,
}

impl TrafficPacer {
    /// Create a pacer from a traffic configuration.
    pub fn new(config: TrafficConfig) -> Self {
        let burst_remaining = match &config.pattern {
            TrafficPattern::Burst { burst_size, .. } => *burst_size,
            _ => 0,
        };
        Self {
            config,
            burst_remaining,
            gap_remaining_ns: 0,
            fixed_carry_ns: 0,
        }
    }

    /// Borrow the underlying config.
    pub fn config(&self) -> &TrafficConfig {
        &self.config
    }

    /// How many packets to emit for a tick of `tick_ns` nanoseconds.
    pub fn packets_for_tick(&mut self, tick_ns: u64) -> usize {
        let batch_size = self.config.batch_size;
        match &self.config.pattern {
            TrafficPattern::FullSpeed => batch_size,
            TrafficPattern::FixedRate { pps } => {
                if *pps == 0 {
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
                let burst_size = *burst_size;
                let gap_ns = gap.as_nanos() as u64;

                if self.gap_remaining_ns > 0 {
                    self.gap_remaining_ns = self.gap_remaining_ns.saturating_sub(tick_ns);
                    return 0;
                }

                if self.burst_remaining == 0 {
                    // Start a fresh burst.
                    self.burst_remaining = burst_size;
                }

                let n = self.burst_remaining.min(batch_size);
                self.burst_remaining -= n;
                if self.burst_remaining == 0 {
                    self.gap_remaining_ns = gap_ns;
                }
                n
            }
        }
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
        };
        let mut pacer = TrafficPacer::new(cfg);
        // 10 packets → 3 ticks of 4, 4, 2 then gap
        assert_eq!(pacer.packets_for_tick(1_000_000), 4);
        assert_eq!(pacer.packets_for_tick(1_000_000), 4);
        assert_eq!(pacer.packets_for_tick(1_000_000), 2);
        // gap = 2 ms; first 1 ms tick still in gap
        assert_eq!(pacer.packets_for_tick(1_000_000), 0);
        assert_eq!(pacer.packets_for_tick(1_000_000), 0);
        // gap done → next burst
        assert_eq!(pacer.packets_for_tick(1_000_000), 4);
    }

    #[test]
    fn pacer_fixed_rate_accumulates_fractional_ticks() {
        // 500 pps → 2 ms per packet. 1 ms ticks should alternate 0, 1, 0, 1…
        let cfg = TrafficConfig {
            pattern: TrafficPattern::FixedRate { pps: 500 },
            batch_size: 64,
            payload_size: 8,
        };
        let mut pacer = TrafficPacer::new(cfg);
        let mut total = 0usize;
        for _ in 0..10 {
            total += pacer.packets_for_tick(1_000_000);
        }
        // 10 ms × 500 pps = 5 packets
        assert_eq!(total, 5);
    }
}
