//! [`SimulatedNetSource`]: an in-process simulated network source.
//!
//! Generates synthetic Ethernet/IP/UDP packets without requiring real
//! hardware. This is the primary source for developing parsers,
//! placement strategies, and sinks before AF_XDP or DPDK are available.
//!
//! ## Packet format
//!
//! Each generated packet has the structure:
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
//!
//! ## Usage
//!
//! ```rust
//! use flyby_net::{SimulatedNetSource, SimNetConfig, NetworkSource, RawBatch};
//! use flyby_core::Lifecycle;
//!
//! let config = SimNetConfig { batch_size: 8, ..SimNetConfig::default() };
//! let mut src = SimulatedNetSource::new(config);
//! src.init().unwrap();
//!
//! let mut batch = RawBatch::new(32, 2048);
//! let n = src.poll_batch(&mut batch).unwrap();
//! assert_eq!(n, 8);
//! ```

use flyby_core::{Error, Lifecycle, Result, Source};

use crate::batch::{PacketMeta, RawBatch};
use crate::config::SimNetConfig;
use crate::source::NetworkSource;

// Fixed Ethernet/IP/UDP header sizes.
const ETH_HEADER: usize = 14; // dst(6) + src(6) + ethertype(2)
const IP_HEADER: usize = 20; // minimal IPv4, no options
const UDP_HEADER: usize = 8; // src_port(2)+dst_port(2)+len(2)+checksum(2)
const NET_HEADER: usize = ETH_HEADER + IP_HEADER + UDP_HEADER;

/// In-process simulated network source.
///
/// Generates synthetic packets at the configured rate. Drop rate and idle
/// rate are configurable for testing back-pressure and drop-counting logic.
pub struct SimulatedNetSource {
    config: SimNetConfig,
    sequence: u64,
    /// Pre-built packet template; only the payload sequence number changes.
    template: Vec<u8>,
    initialized: bool,
}

impl SimulatedNetSource {
    /// Construct a new simulator with the given configuration.
    pub fn new(config: SimNetConfig) -> Self {
        let frame_size = NET_HEADER + config.payload_size;
        let mut template = vec![0u8; frame_size];
        Self::fill_static_headers(&mut template, config.udp_dst_port, frame_size);
        Self {
            config,
            sequence: 0,
            template,
            initialized: false,
        }
    }

    /// Write the static header fields into the template buffer.
    ///
    /// Only fields that never change between packets are filled here.
    /// The payload is patched per-packet in [`poll_batch`][Self::poll_batch].
    fn fill_static_headers(buf: &mut [u8], udp_dst_port: u16, frame_size: usize) {
        // Ethernet header
        // dst MAC: broadcast ff:ff:ff:ff:ff:ff
        buf[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        // src MAC: 02:00:00:00:00:01 (locally administered)
        buf[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
        // EtherType: IPv4
        buf[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

        // IPv4 header (minimal, no options)
        buf[14] = 0x45; // version=4, IHL=5
        buf[15] = 0x00; // DSCP/ECN
        let total_len = (frame_size - ETH_HEADER) as u16;
        buf[16..18].copy_from_slice(&total_len.to_be_bytes());
        // id=0, flags=0, frag_offset=0
        buf[18..20].copy_from_slice(&[0x00, 0x00]);
        buf[20..22].copy_from_slice(&[0x00, 0x00]);
        buf[22] = 64; // TTL
        buf[23] = 17; // proto = UDP
        // checksum: 0 (intentionally invalid, simulator only)
        buf[24..26].copy_from_slice(&[0x00, 0x00]);
        // src IP: 192.168.1.1
        buf[26..30].copy_from_slice(&[192, 168, 1, 1]);
        // dst IP: 192.168.1.2
        buf[30..34].copy_from_slice(&[192, 168, 1, 2]);

        // UDP header
        // src port: 12345
        buf[34..36].copy_from_slice(&12345u16.to_be_bytes());
        // dst port: configured
        buf[36..38].copy_from_slice(&udp_dst_port.to_be_bytes());
        let udp_len = (UDP_HEADER + (frame_size - NET_HEADER)) as u16;
        buf[38..40].copy_from_slice(&udp_len.to_be_bytes());
        // checksum: 0 (intentionally invalid)
        buf[40..42].copy_from_slice(&[0x00, 0x00]);
        // payload: zeroed (patched per-packet)
    }

    /// Patch the sequence number into the payload area of `packet`.
    fn write_sequence(packet: &mut [u8], sequence: u64) {
        if packet.len() >= NET_HEADER + 8 {
            packet[NET_HEADER..NET_HEADER + 8].copy_from_slice(&sequence.to_be_bytes());
        }
    }

    /// Simulated frame size in bytes.
    pub fn frame_size(&self) -> usize {
        NET_HEADER + self.config.payload_size
    }
}

impl Lifecycle for SimulatedNetSource {
    fn init(&mut self) -> Result<()> {
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.initialized = false;
        Ok(())
    }
}

impl Source for SimulatedNetSource {
    /// Single-packet shim: returns the next packet from a one-packet
    /// batch. Exists so `SimulatedNetSource` satisfies `flyby_core::Source`
    /// for compatibility with the current pipeline skeleton.
    ///
    /// Prefer [`NetworkSource::poll_batch`] for production use.
    fn poll(&mut self) -> Result<Option<&[u8]>> {
        if !self.initialized {
            return Err(Error::source("SimulatedNetSource not initialised"));
        }
        Self::write_sequence(&mut self.template, self.sequence);
        self.sequence = self.sequence.wrapping_add(1);
        Ok(Some(&self.template))
    }
}

impl NetworkSource for SimulatedNetSource {
    fn poll_batch(&mut self, batch: &mut RawBatch) -> Result<usize> {
        if !self.initialized {
            return Err(Error::source("SimulatedNetSource not initialised"));
        }

        batch.reset(self.frame_size());

        // Simulate idle: return empty batch at the configured idle_rate.
        // We use the sequence number as a deterministic pseudo-random
        // input (fast, no RNG dependency).
        let idle_threshold = (self.config.idle_rate * u32::MAX as f32) as u32;
        let pseudo = ((self.sequence.wrapping_mul(6364136223846793005)) >> 32) as u32;
        if self.config.idle_rate > 0.0 && pseudo < idle_threshold {
            return Ok(0);
        }

        let drop_threshold = (self.config.drop_rate * u32::MAX as f32) as u32;
        let n = self.config.batch_size.min(batch.capacity());

        for _ in 0..n {
            // Simulate NIC drop: count but skip this packet.
            let pseudo = ((self.sequence.wrapping_mul(6364136223846793005)) >> 32) as u32;
            if self.config.drop_rate > 0.0 && pseudo < drop_threshold {
                batch.dropped += 1;
                self.sequence = self.sequence.wrapping_add(1);
                continue;
            }

            Self::write_sequence(&mut self.template, self.sequence);
            let meta = PacketMeta {
                timestamp_ns: self.sequence * 1_000, // synthetic nanoseconds
                queue_id: 0,
                original_len: self.frame_size() as u16,
            };
            batch.push(&self.template, meta);
            self.sequence = self.sequence.wrapping_add(1);
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
    use flyby_core::Lifecycle;

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
    fn uninitialized_source_returns_error() {
        let mut src = make_src();
        let mut batch = make_batch();
        assert!(src.poll_batch(&mut batch).is_err());
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
        // With 50% drop rate over 64 packets, we expect some drops.
        // The exact number varies but should be > 0 and < 64.
        assert!(batch.dropped > 0);
        assert!(batch.len() + batch.dropped as usize <= 64);
    }

    #[test]
    fn ethertype_is_ipv4() {
        let mut src = make_src();
        src.init().unwrap();
        let mut batch = make_batch();
        src.poll_batch(&mut batch).unwrap();
        let (data, _) = batch.packets().next().unwrap();
        let ethertype = u16::from_be_bytes([data[12], data[13]]);
        assert_eq!(ethertype, 0x0800, "EtherType must be IPv4 (0x0800)");
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
}
