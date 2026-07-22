//! Fault injection: controlled introduction of failures into the simulation.
//!
//! Every injected fault is observable — the simulator emits a [`SimEvent`]
//! for each fault so dashboards and tests can detect and measure them.
//!
//! ## Fault types
//!
//! | Fault | Effect |
//! |---|---|
//! | Drop | Packet or record is silently discarded |
//! | Corrupt | A byte in the payload is flipped |
//! | Latency spike | Delivery is delayed by `spike_ns` nanoseconds (virtual time) |
//!
//! ## Determinism
//!
//! `FaultInjector` uses a simple LCG (same technique as `SimulatedNetSource`)
//! seeded from a sequence number.  Given the same seed and `FaultSpec`, the
//! same sequence of faults is produced every run — essential for reproducible
//! test failures.
//!
//! [`SimEvent`]: crate::events::SimEvent

/// Configuration for fault injection.
///
/// All rates are in `[0.0, 1.0]`.  Setting all to zero disables injection
/// with no performance overhead in [`FaultInjector::should_drop`].
#[derive(Debug, Clone, PartialEq)]
pub struct FaultSpec {
    /// Probability that a packet or record is dropped (0.0 = never, 1.0 = always).
    pub drop_rate: f64,
    /// Probability that a packet payload byte is flipped.
    pub corrupt_rate: f64,
    /// Probability that a latency spike is injected.
    pub latency_spike_rate: f64,
    /// Duration of a latency spike in nanoseconds.
    pub latency_spike_ns: u64,
}

impl Default for FaultSpec {
    fn default() -> Self {
        Self { drop_rate: 0.0, corrupt_rate: 0.0, latency_spike_rate: 0.0, latency_spike_ns: 0 }
    }
}

impl FaultSpec {
    /// `true` if this spec injects no faults at all.
    pub fn is_clean(&self) -> bool {
        self.drop_rate == 0.0
            && self.corrupt_rate == 0.0
            && self.latency_spike_rate == 0.0
    }
}

/// Applies fault injection decisions to a packet/record sequence.
///
/// Constructed from a [`FaultSpec`] and a seed; call [`should_drop`],
/// [`should_corrupt`], and [`should_spike`] in order for each item.
///
/// [`should_drop`]: FaultInjector::should_drop
/// [`should_corrupt`]: FaultInjector::should_corrupt
/// [`should_spike`]: FaultInjector::should_spike
#[derive(Debug, Clone)]
pub struct FaultInjector {
    spec: FaultSpec,
    /// LCG state, advanced per-item for determinism.
    state: u64,
}

impl FaultInjector {
    /// Create a new injector with the given spec and seed.
    ///
    /// Using the same `seed` and [`FaultSpec`] always produces the same
    /// sequence of decisions.
    pub fn new(spec: FaultSpec, seed: u64) -> Self {
        Self { spec, state: seed }
    }

    /// `true` if the current item should be dropped.
    ///
    /// Advances the internal LCG state.
    pub fn should_drop(&mut self) -> bool {
        if self.spec.drop_rate == 0.0 {
            return false;
        }
        self.random_unit() < self.spec.drop_rate
    }

    /// `true` if the current item's payload should be corrupted.
    ///
    /// Call after [`should_drop`][Self::should_drop]; no-op on dropped items.
    pub fn should_corrupt(&mut self) -> bool {
        if self.spec.corrupt_rate == 0.0 {
            return false;
        }
        self.random_unit() < self.spec.corrupt_rate
    }

    /// `true` if a latency spike should be applied.
    ///
    /// Returns the spike duration in nanoseconds if a spike fires, else 0.
    pub fn should_spike(&mut self) -> u64 {
        if self.spec.latency_spike_rate == 0.0 {
            return 0;
        }
        if self.random_unit() < self.spec.latency_spike_rate {
            self.spec.latency_spike_ns
        } else {
            0
        }
    }

    /// Flip one byte in `payload` at a position derived from the current LCG state.
    ///
    /// Does nothing if `payload` is empty.
    pub fn corrupt_payload(&mut self, payload: &mut [u8]) {
        if payload.is_empty() {
            return;
        }
        let pos = (self.next_state() as usize) % payload.len();
        payload[pos] ^= 0xFF;
    }

    // Advance LCG and return a uniform f64 in [0.0, 1.0).
    fn random_unit(&mut self) -> f64 {
        let n = self.next_state();
        (n >> 32) as f64 / u32::MAX as f64
    }

    fn next_state(&mut self) -> u64 {
        // Knuth multiplicative LCG (same constants as SimulatedNetSource)
        self.state = self.state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spec_is_clean() {
        assert!(FaultSpec::default().is_clean());
    }

    #[test]
    fn zero_drop_rate_never_drops() {
        let mut inj = FaultInjector::new(FaultSpec::default(), 42);
        for _ in 0..1_000 {
            assert!(!inj.should_drop());
        }
    }

    #[test]
    fn full_drop_rate_always_drops() {
        let spec = FaultSpec { drop_rate: 1.0, ..FaultSpec::default() };
        let mut inj = FaultInjector::new(spec, 0);
        for _ in 0..100 {
            assert!(inj.should_drop());
        }
    }

    #[test]
    fn partial_drop_rate_produces_some_drops() {
        let spec = FaultSpec { drop_rate: 0.5, ..FaultSpec::default() };
        let mut inj = FaultInjector::new(spec, 1);
        let drops: usize = (0..1000).filter(|_| inj.should_drop()).count();
        assert!(drops > 200 && drops < 800, "expected ~500 drops, got {drops}");
    }

    #[test]
    fn corrupt_payload_flips_a_byte() {
        let mut inj = FaultInjector::new(FaultSpec::default(), 7);
        let original = vec![0xAAu8; 16];
        let mut payload = original.clone();
        inj.corrupt_payload(&mut payload);
        assert_ne!(payload, original, "corruption should change the payload");
        let diffs = payload.iter().zip(original.iter()).filter(|(a, b)| a != b).count();
        assert_eq!(diffs, 1, "exactly one byte should be flipped");
    }

    #[test]
    fn determinism_same_seed_same_decisions() {
        let spec = FaultSpec { drop_rate: 0.3, corrupt_rate: 0.1, ..FaultSpec::default() };
        let decisions_a: Vec<bool> = {
            let mut inj = FaultInjector::new(spec.clone(), 99);
            (0..100).map(|_| inj.should_drop()).collect()
        };
        let decisions_b: Vec<bool> = {
            let mut inj = FaultInjector::new(spec, 99);
            (0..100).map(|_| inj.should_drop()).collect()
        };
        assert_eq!(decisions_a, decisions_b, "same seed must produce same decisions");
    }

    #[test]
    fn different_seeds_produce_different_decisions() {
        let spec = FaultSpec { drop_rate: 0.5, ..FaultSpec::default() };
        let a: Vec<bool> = {
            let mut inj = FaultInjector::new(spec.clone(), 1);
            (0..50).map(|_| inj.should_drop()).collect()
        };
        let b: Vec<bool> = {
            let mut inj = FaultInjector::new(spec, 2);
            (0..50).map(|_| inj.should_drop()).collect()
        };
        assert_ne!(a, b, "different seeds should produce different sequences");
    }

    #[test]
    fn latency_spike_returns_configured_ns() {
        let spec = FaultSpec {
            latency_spike_rate: 1.0,
            latency_spike_ns: 50_000,
            ..FaultSpec::default()
        };
        let mut inj = FaultInjector::new(spec, 0);
        assert_eq!(inj.should_spike(), 50_000);
    }

    #[test]
    fn zero_spike_rate_returns_zero() {
        let mut inj = FaultInjector::new(FaultSpec::default(), 0);
        for _ in 0..100 {
            assert_eq!(inj.should_spike(), 0);
        }
    }
}
