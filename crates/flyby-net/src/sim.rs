//! [`SimulatedNetSource`]: an in-process simulated network source.
//!
//! Generates synthetic Ethernet/IP/UDP packets without requiring real
//! hardware. This is the primary source for developing parsers,
//! placement strategies, and sinks before AF_XDP or DPDK are available.
//!
//! ## Packet format
//!
//! ```text
//! [Ethernet dst  6B][Ethernet src  6B][EtherType 2B = 0x0800]
//! [IPv4 header  20B]
//! [UDP header    8B]
//! [Payload       NB]  ← configurable; default is an 8-byte sequence number
//! ```
//!
//! Checksums are intentionally wrong (all zeros) — the simulator is for
//! pipeline testing, not wire-compatible replay.

use std::sync::Arc;

use flyby_core::{Error, Lifecycle, MetricsCollector, NullCollector, Result, Source};

use crate::batch::{PacketMeta, PushResult, RawBatch};
use crate::config::SimNetConfig;
use crate::metrics::NetMetricKey;
use crate::source::NetworkSource;

// Fixed Ethernet/IP/UDP header sizes.
const ETH_HEADER: usize = 14;
const IP_HEADER: usize = 20;
const UDP_HEADER: usize = 8;
const NET_HEADER: usize = ETH_HEADER + IP_HEADER + UDP_HEADER;

/// SplitMix64-style mix for deterministic pseudo-random decisions.
fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// In-process simulated network source.
pub struct SimulatedNetSource {
    config: SimNetConfig,
    sequence: u64,
    /// Separate counter for idle/drop PRNG so decisions are not correlated
    /// solely with the packet sequence on path 0.
    rng_state: u64,
    /// Pre-built packet template; only the payload sequence number changes.
    template: Vec<u8>,
    /// Single-packet buffer for the `Source::poll` shim.
    poll_scratch: Vec<u8>,
    initialized: bool,
    metrics: Arc<dyn MetricsCollector>,
}

impl SimulatedNetSource {
    /// Construct a new simulator with the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if `config.validate()` fails. Prefer validating first in
    /// library code that must return `Result`.
    pub fn new(config: SimNetConfig) -> Self {
        config
            .validate()
            .expect("SimNetConfig::validate failed; fix config before constructing");
        Self::new_unchecked(config)
    }

    /// Construct without validating (caller must have validated).
    pub fn try_new(config: SimNetConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self::new_unchecked(config))
    }

    fn new_unchecked(config: SimNetConfig) -> Self {
        let frame_size = NET_HEADER + config.payload_size;
        let mut template = vec![0u8; frame_size];
        Self::fill_static_headers(&mut template, config.udp_dst_port, frame_size);
        Self {
            config,
            sequence: 0,
            rng_state: 0xC0FFEE_u64,
            template,
            poll_scratch: Vec::new(),
            initialized: false,
            metrics: Arc::new(NullCollector),
        }
    }

    /// Attach a metrics collector (shared across polls).
    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsCollector>) -> Self {
        self.metrics = metrics;
        self
    }

    fn fill_static_headers(buf: &mut [u8], udp_dst_port: u16, frame_size: usize) {
        buf[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        buf[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
        buf[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

        buf[14] = 0x45;
        buf[15] = 0x00;
        let total_len = (frame_size - ETH_HEADER) as u16;
        buf[16..18].copy_from_slice(&total_len.to_be_bytes());
        buf[18..20].copy_from_slice(&[0x00, 0x00]);
        buf[20..22].copy_from_slice(&[0x00, 0x00]);
        buf[22] = 64;
        buf[23] = 17;
        buf[24..26].copy_from_slice(&[0x00, 0x00]);
        buf[26..30].copy_from_slice(&[192, 168, 1, 1]);
        buf[30..34].copy_from_slice(&[192, 168, 1, 2]);

        buf[34..36].copy_from_slice(&12345u16.to_be_bytes());
        buf[36..38].copy_from_slice(&udp_dst_port.to_be_bytes());
        let udp_len = (UDP_HEADER + (frame_size - NET_HEADER)) as u16;
        buf[38..40].copy_from_slice(&udp_len.to_be_bytes());
        buf[40..42].copy_from_slice(&[0x00, 0x00]);
    }

    fn write_sequence(packet: &mut [u8], sequence: u64) {
        if packet.len() >= NET_HEADER + 8 {
            packet[NET_HEADER..NET_HEADER + 8].copy_from_slice(&sequence.to_be_bytes());
        }
    }

    /// Simulated frame size in bytes.
    pub fn frame_size(&self) -> usize {
        NET_HEADER + self.config.payload_size
    }

    fn next_u32(&mut self) -> u32 {
        self.rng_state = mix64(self.rng_state);
        (self.rng_state >> 32) as u32
    }

    fn rate_hit(&mut self, rate: f32) -> bool {
        if rate <= 0.0 {
            return false;
        }
        // Compare in u64 domain to avoid f32 precision issues with u32::MAX.
        let threshold = (rate as f64 * (u32::MAX as f64 + 1.0)) as u64;
        (self.next_u32() as u64) < threshold
    }

    fn ensure_init(&self) -> Result<()> {
        if !self.initialized {
            return Err(Error::lifecycle("SimulatedNetSource not initialised"));
        }
        Ok(())
    }
}

impl Lifecycle for SimulatedNetSource {
    fn init(&mut self) -> Result<()> {
        self.config.validate()?;
        self.initialized = true;
        self.sequence = 0;
        self.rng_state = 0xC0FFEE_u64;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.initialized = false;
        Ok(())
    }
}

impl Source for SimulatedNetSource {
    /// Single-packet shim routed through the same idle/drop logic as
    /// [`NetworkSource::poll_batch`] (one-slot batch).
    fn poll(&mut self) -> Result<Option<&[u8]>> {
        self.ensure_init()?;
        let mut batch = RawBatch::new(1, self.frame_size().max(1));
        let n = self.poll_batch(&mut batch)?;
        if n == 0 {
            return Ok(None);
        }
        let (data, _) = batch.packets().next().expect("n > 0");
        self.poll_scratch.clear();
        self.poll_scratch.extend_from_slice(data);
        Ok(Some(&self.poll_scratch))
    }
}

impl NetworkSource for SimulatedNetSource {
    fn poll_batch(&mut self, batch: &mut RawBatch) -> Result<usize> {
        self.ensure_init()?;

        batch.reset(self.frame_size());
        self.metrics.record_counter(&NetMetricKey::BatchesPolled, 1);

        // Idle: empty batch at configured idle_rate (independent PRNG stream).
        if self.rate_hit(self.config.idle_rate) {
            return Ok(0);
        }

        let intended = self.config.batch_size;
        let capacity = batch.capacity();
        let mut produced = 0usize;
        let mut dropped_this = 0u64;
        let mut bytes = 0u64;

        for _ in 0..intended {
            if self.rate_hit(self.config.drop_rate) {
                batch.record_drop();
                dropped_this += 1;
                self.sequence = self.sequence.wrapping_add(1);
                continue;
            }

            if produced >= capacity {
                batch.record_drop();
                dropped_this += 1;
                self.sequence = self.sequence.wrapping_add(1);
                continue;
            }

            Self::write_sequence(&mut self.template, self.sequence);
            let frame_len = self.frame_size();
            let meta = PacketMeta {
                timestamp_ns: self.sequence.saturating_mul(1_000),
                queue_id: 0,
                original_len: frame_len.min(u32::MAX as usize) as u32,
            };
            match batch.push(&self.template, meta) {
                PushResult::Ok | PushResult::Truncated => {
                    produced += 1;
                    bytes += frame_len as u64;
                }
                PushResult::Full => {
                    batch.record_drop();
                    dropped_this += 1;
                }
            }
            self.sequence = self.sequence.wrapping_add(1);
        }

        if produced > 0 {
            self.metrics
                .record_counter(&NetMetricKey::PacketsReceived, produced as u64);
            self.metrics
                .record_counter(&NetMetricKey::BytesReceived, bytes);
            self.metrics
                .record_histogram(&NetMetricKey::BatchSize, produced as f64);
        }
        if dropped_this > 0 {
            self.metrics
                .record_counter(&NetMetricKey::PacketsDropped, dropped_this);
        }

        Ok(batch.len())
    }

    fn backend_name(&self) -> &'static str {
        "simulator"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flyby_core::{ErrorKind, Lifecycle};

    fn make_src() -> SimulatedNetSource {
        SimulatedNetSource::new(SimNetConfig::default())
    }

    fn make_batch() -> RawBatch {
        RawBatch::new(64, 2048)
    }

    #[test]
    fn poll_batch_returns_configured_count() {
        let mut src = make_src();
        src.init().unwrap();
        let mut batch = make_batch();
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, SimNetConfig::default().batch_size);
        assert_eq!(batch.len(), n);
    }

    #[test]
    fn packets_have_correct_frame_size() {
        let config = SimNetConfig {
            payload_size: 16,
            ..SimNetConfig::default()
        };
        let mut src = SimulatedNetSource::new(config);
        src.init().unwrap();
        let mut batch = make_batch();
        src.poll_batch(&mut batch).unwrap();
        for (data, _meta) in batch.packets() {
            assert_eq!(data.len(), NET_HEADER + 16);
        }
    }

    #[test]
    fn sequence_numbers_are_monotonic() {
        let mut src = make_src();
        src.init().unwrap();
        let mut batch = make_batch();
        src.poll_batch(&mut batch).unwrap();

        let mut prev_seq = None::<u64>;
        for (data, _meta) in batch.packets() {
            let seq = u64::from_be_bytes(data[NET_HEADER..NET_HEADER + 8].try_into().unwrap());
            if let Some(prev) = prev_seq {
                assert!(seq > prev, "sequence must increase monotonically");
            }
            prev_seq = Some(seq);
        }
    }

    #[test]
    fn uninitialized_source_returns_lifecycle_error() {
        let mut src = make_src();
        let mut batch = make_batch();
        let err = src.poll_batch(&mut batch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Lifecycle);
    }

    #[test]
    fn shutdown_prevents_further_polls() {
        let mut src = make_src();
        src.init().unwrap();
        src.shutdown().unwrap();
        let mut batch = make_batch();
        assert!(src.poll_batch(&mut batch).is_err());
    }

    #[test]
    fn drop_rate_produces_dropped_count() {
        let config = SimNetConfig {
            drop_rate: 0.5,
            batch_size: 64,
            ..SimNetConfig::default()
        };
        let mut src = SimulatedNetSource::new(config);
        src.init().unwrap();
        let mut batch = RawBatch::new(64, 2048);
        src.poll_batch(&mut batch).unwrap();
        assert!(batch.dropped() > 0);
        assert!(batch.len() + batch.dropped() as usize <= 64);
    }

    #[test]
    fn capacity_overflow_counts_drops() {
        let config = SimNetConfig {
            batch_size: 8,
            drop_rate: 0.0,
            idle_rate: 0.0,
            ..SimNetConfig::default()
        };
        let mut src = SimulatedNetSource::new(config);
        src.init().unwrap();
        let mut batch = RawBatch::new(2, 2048);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 2);
        assert_eq!(batch.dropped(), 6);
    }

    #[test]
    fn source_poll_respects_idle() {
        let config = SimNetConfig {
            idle_rate: 0.999,
            batch_size: 1,
            ..SimNetConfig::default()
        };
        let mut src = SimulatedNetSource::new(config);
        src.init().unwrap();
        let mut idle = 0;
        for _ in 0..50 {
            if src.poll().unwrap().is_none() {
                idle += 1;
            }
        }
        assert!(idle > 0, "expected some idle polls, got {idle}");
    }

    #[test]
    fn ethertype_is_ipv4() {
        let mut src = make_src();
        src.init().unwrap();
        let mut batch = make_batch();
        src.poll_batch(&mut batch).unwrap();
        let (data, _) = batch.packets().next().unwrap();
        let ethertype = u16::from_be_bytes([data[12], data[13]]);
        assert_eq!(ethertype, 0x0800);
    }

    #[test]
    fn udp_dst_port_matches_config() {
        let config = SimNetConfig {
            udp_dst_port: 9001,
            ..SimNetConfig::default()
        };
        let mut src = SimulatedNetSource::new(config);
        src.init().unwrap();
        let mut batch = make_batch();
        src.poll_batch(&mut batch).unwrap();
        let (data, _) = batch.packets().next().unwrap();
        let dst_port = u16::from_be_bytes([data[36], data[37]]);
        assert_eq!(dst_port, 9001);
    }

    #[test]
    fn backend_name() {
        let src = make_src();
        assert_eq!(src.backend_name(), "simulator");
    }

    #[test]
    fn invalid_config_rejected() {
        let cfg = SimNetConfig {
            idle_rate: 1.5,
            ..SimNetConfig::default()
        };
        assert!(SimulatedNetSource::try_new(cfg).is_err());
    }
}
