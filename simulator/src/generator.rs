//! Packet payload generators for virtual NICs.
//!
//! Part VI requires generators for fixed-size packets, random payloads,
//! Gaussian distributions, protocol-aware messages, and custom callbacks.
//! Rate pacing lives in [`crate::traffic`]; this module owns **what** goes
//! in each packet.

use std::fmt;
use std::sync::Arc;

/// Ethernet (14) + IPv4 (20) + UDP (8) header size used by synthetic frames.
pub const NET_HEADER_LEN: usize = 42;

/// User callback that fills a payload buffer for packet `seq`.
pub type CustomPayloadFn = dyn Fn(u64, &mut [u8]) + Send + Sync;

/// How the virtual NIC fills packet payloads.
#[derive(Clone, Default)]
pub enum PayloadSpec {
    /// Fixed-size payload: big-endian sequence number in the first 8 bytes,
    /// remaining bytes zero. Matches [`flyby_net::SimulatedNetSource`].
    #[default]
    FixedSeq,

    /// Deterministic pseudo-random payload bytes (LCG-seeded).
    Random {
        /// LCG seed.
        seed: u64,
    },

    /// Payload length sampled from a Normal(`mean`, `std_dev`) distribution,
    /// clamped to `[1, max]`. Content is a sequence number plus random fill.
    GaussianSize {
        /// Mean payload length in bytes.
        mean: f64,
        /// Standard deviation of payload length.
        std_dev: f64,
        /// LCG seed for sampling.
        seed: u64,
        /// Maximum payload length (also sizes the NIC batch slots).
        max: usize,
    },

    /// Structured, protocol-aware message body.
    Protocol(ProtocolMessage),

    /// User-supplied callback: `(sequence, payload_buf)`.
    ///
    /// Wrapped in [`Arc`] so [`PayloadSpec`] / [`TrafficConfig`](crate::traffic::TrafficConfig)
    /// remain `Clone`.
    Custom(Arc<CustomPayloadFn>),
}

impl fmt::Debug for PayloadSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FixedSeq => write!(f, "FixedSeq"),
            Self::Random { seed } => f.debug_struct("Random").field("seed", seed).finish(),
            Self::GaussianSize {
                mean,
                std_dev,
                seed,
                max,
            } => f
                .debug_struct("GaussianSize")
                .field("mean", mean)
                .field("std_dev", std_dev)
                .field("seed", seed)
                .field("max", max)
                .finish(),
            Self::Protocol(p) => f.debug_tuple("Protocol").field(p).finish(),
            Self::Custom(_) => write!(f, "Custom(..)"),
        }
    }
}

impl PartialEq for PayloadSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::FixedSeq, Self::FixedSeq) => true,
            (Self::Random { seed: a }, Self::Random { seed: b }) => a == b,
            (
                Self::GaussianSize {
                    mean: m1,
                    std_dev: s1,
                    seed: e1,
                    max: x1,
                },
                Self::GaussianSize {
                    mean: m2,
                    std_dev: s2,
                    seed: e2,
                    max: x2,
                },
            ) => m1 == m2 && s1 == s2 && e1 == e2 && x1 == x2,
            (Self::Protocol(a), Self::Protocol(b)) => a == b,
            (Self::Custom(_), Self::Custom(_)) => false,
            _ => false,
        }
    }
}

/// Built-in protocol-aware payload layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolMessage {
    /// Binary market quote (34 bytes):
    /// `msg_type(u8)='Q' | flags(u8) | symbol([u8;8]) | bid(u64 BE) | ask(u64 BE) | seq(u64 BE)`.
    MarketQuote {
        /// ASCII symbol, padded/truncated to 8 bytes.
        symbol: [u8; 8],
    },

    /// Length-prefixed FIX-like ASCII quote.
    ///
    /// Body: `35=Q|55={symbol}|44={price}|34={seq}|` preceded by a `u16` BE length.
    FixQuote {
        /// Symbol string written into tag 55.
        symbol: String,
    },
}

impl ProtocolMessage {
    /// Create a [`MarketQuote`][Self::MarketQuote] from an ASCII symbol.
    pub fn market_quote(symbol: &str) -> Self {
        let mut buf = [b' '; 8];
        let bytes = symbol.as_bytes();
        let n = bytes.len().min(8);
        buf[..n].copy_from_slice(&bytes[..n]);
        Self::MarketQuote { symbol: buf }
    }

    /// Create a [`FixQuote`][Self::FixQuote].
    pub fn fix_quote(symbol: impl Into<String>) -> Self {
        Self::FixQuote {
            symbol: symbol.into(),
        }
    }

    /// Nominal payload size for slot allocation.
    pub fn nominal_size(&self) -> usize {
        match self {
            Self::MarketQuote { .. } => 34,
            Self::FixQuote { symbol } => 2 + 32 + symbol.len(), // generous upper bound
        }
    }
}

/// Stateful helper that fills payloads according to a [`PayloadSpec`].
#[derive(Debug, Clone)]
pub struct PayloadGenerator {
    spec: PayloadSpec,
    /// LCG state for Random / GaussianSize.
    rng: u64,
    /// Second Box–Muller sample waiting to be consumed.
    gaussian_spare: Option<f64>,
}

impl PayloadGenerator {
    /// Create a generator from a payload specification.
    pub fn new(spec: PayloadSpec) -> Self {
        let rng = match &spec {
            PayloadSpec::Random { seed } => *seed,
            PayloadSpec::GaussianSize { seed, .. } => *seed,
            _ => 0,
        };
        Self {
            spec,
            rng,
            gaussian_spare: None,
        }
    }

    /// Borrow the underlying spec.
    pub fn spec(&self) -> &PayloadSpec {
        &self.spec
    }

    /// Maximum payload bytes this generator may produce.
    pub fn max_payload_len(&self, configured: usize) -> usize {
        match &self.spec {
            PayloadSpec::GaussianSize { max, .. } => (*max).max(1),
            PayloadSpec::Protocol(p) => p.nominal_size().max(configured).max(1),
            _ => configured.max(1),
        }
    }

    /// Fill `buf` for packet `seq`, returning the number of payload bytes used.
    ///
    /// `buf` must be large enough for the chosen spec; unused trailing bytes
    /// are left untouched. The return value is the live payload length.
    pub fn fill(&mut self, seq: u64, buf: &mut [u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        match &self.spec {
            PayloadSpec::FixedSeq => {
                fill_seq(buf, seq);
                buf.len()
            }
            PayloadSpec::Random { .. } => {
                for byte in buf.iter_mut() {
                    *byte = (self.next_u64() & 0xFF) as u8;
                }
                buf.len()
            }
            PayloadSpec::GaussianSize {
                mean, std_dev, max, ..
            } => {
                let mean = *mean;
                let std_dev = *std_dev;
                let max = (*max).min(buf.len()).max(1);
                let sample = self.sample_gaussian(mean, std_dev);
                let len = sample.round().clamp(1.0, max as f64) as usize;
                fill_seq(&mut buf[..len], seq);
                if len > 8 {
                    for byte in &mut buf[8..len] {
                        *byte = (self.next_u64() & 0xFF) as u8;
                    }
                }
                len
            }
            PayloadSpec::Protocol(ProtocolMessage::MarketQuote { symbol }) => {
                if buf.len() < 34 {
                    let n = buf.len();
                    buf.fill(0);
                    if n > 0 {
                        buf[0] = b'Q';
                    }
                    return n;
                }
                buf[0] = b'Q';
                buf[1] = 0;
                buf[2..10].copy_from_slice(symbol);
                let bid = 100_000u64.wrapping_add(seq % 1_000);
                let ask = bid.wrapping_add(1);
                buf[10..18].copy_from_slice(&bid.to_be_bytes());
                buf[18..26].copy_from_slice(&ask.to_be_bytes());
                buf[26..34].copy_from_slice(&seq.to_be_bytes());
                34
            }
            PayloadSpec::Protocol(ProtocolMessage::FixQuote { symbol }) => {
                let body = format!("35=Q|55={symbol}|44={}|34={seq}|", 100 + (seq % 50));
                let body_bytes = body.as_bytes();
                let total = 2 + body_bytes.len();
                if buf.len() < total {
                    let n = buf.len();
                    buf.fill(0);
                    return n;
                }
                let len = body_bytes.len() as u16;
                buf[0..2].copy_from_slice(&len.to_be_bytes());
                buf[2..2 + body_bytes.len()].copy_from_slice(body_bytes);
                total
            }
            PayloadSpec::Custom(cb) => {
                cb(seq, buf);
                buf.len()
            }
        }
    }

    fn next_u64(&mut self) -> u64 {
        // Knuth multiplicative LCG (same family as FaultInjector).
        self.rng = self
            .rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.rng
    }

    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Box–Muller sample from Normal(mean, std_dev).
    fn sample_gaussian(&mut self, mean: f64, std_dev: f64) -> f64 {
        if let Some(spare) = self.gaussian_spare.take() {
            return mean + std_dev * spare;
        }
        // Avoid log(0).
        let u1 = self.next_unit().clamp(f64::EPSILON, 1.0);
        let u2 = self.next_unit().clamp(f64::EPSILON, 1.0);
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        let z0 = r * theta.cos();
        let z1 = r * theta.sin();
        self.gaussian_spare = Some(z1);
        mean + std_dev * z0
    }
}

fn fill_seq(buf: &mut [u8], seq: u64) {
    if buf.len() >= 8 {
        buf[..8].copy_from_slice(&seq.to_be_bytes());
        for b in &mut buf[8..] {
            *b = 0;
        }
    } else {
        let bytes = seq.to_be_bytes();
        let n = buf.len();
        buf.copy_from_slice(&bytes[8 - n..]);
    }
}

/// Build a synthetic Ethernet/IPv4/UDP frame with the given payload.
///
/// Checksums are left as zero (same convention as [`flyby_net::SimulatedNetSource`]).
pub fn build_udp_frame(payload: &[u8], udp_dst_port: u16) -> Vec<u8> {
    let frame_size = NET_HEADER_LEN + payload.len();
    let mut buf = vec![0u8; frame_size];

    // Ethernet
    buf[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    buf[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
    buf[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

    // IPv4
    buf[14] = 0x45;
    buf[15] = 0x00;
    let total_len = (frame_size - 14) as u16;
    buf[16..18].copy_from_slice(&total_len.to_be_bytes());
    buf[22] = 64;
    buf[23] = 17; // UDP
    buf[26..30].copy_from_slice(&[192, 168, 1, 1]);
    buf[30..34].copy_from_slice(&[192, 168, 1, 2]);

    // UDP
    buf[34..36].copy_from_slice(&12345u16.to_be_bytes());
    buf[36..38].copy_from_slice(&udp_dst_port.to_be_bytes());
    let udp_len = (8 + payload.len()) as u16;
    buf[38..40].copy_from_slice(&udp_len.to_be_bytes());

    buf[NET_HEADER_LEN..].copy_from_slice(payload);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_seq_writes_be_sequence() {
        let mut g = PayloadGenerator::new(PayloadSpec::FixedSeq);
        let mut buf = [0u8; 16];
        assert_eq!(g.fill(0x1122_3344_5566_7788, &mut buf), 16);
        assert_eq!(&buf[..8], &0x1122_3344_5566_7788u64.to_be_bytes());
        assert_eq!(&buf[8..], &[0u8; 8]);
    }

    #[test]
    fn random_is_deterministic() {
        let mut a = PayloadGenerator::new(PayloadSpec::Random { seed: 7 });
        let mut b = PayloadGenerator::new(PayloadSpec::Random { seed: 7 });
        let mut ba = [0u8; 32];
        let mut bb = [0u8; 32];
        a.fill(0, &mut ba);
        b.fill(0, &mut bb);
        assert_eq!(ba, bb);
    }

    #[test]
    fn gaussian_size_stays_in_range() {
        let mut g = PayloadGenerator::new(PayloadSpec::GaussianSize {
            mean: 64.0,
            std_dev: 16.0,
            seed: 1,
            max: 128,
        });
        let mut buf = [0u8; 128];
        for seq in 0..200 {
            let n = g.fill(seq, &mut buf);
            assert!((1..=128).contains(&n), "len {n} out of range");
        }
    }

    #[test]
    fn market_quote_layout() {
        let mut g =
            PayloadGenerator::new(PayloadSpec::Protocol(ProtocolMessage::market_quote("AAPL")));
        let mut buf = [0u8; 34];
        assert_eq!(g.fill(42, &mut buf), 34);
        assert_eq!(buf[0], b'Q');
        assert_eq!(&buf[2..6], b"AAPL");
        let seq = u64::from_be_bytes(buf[26..34].try_into().unwrap());
        assert_eq!(seq, 42);
    }

    #[test]
    fn fix_quote_roundtrip_prefix() {
        let mut g =
            PayloadGenerator::new(PayloadSpec::Protocol(ProtocolMessage::fix_quote("MSFT")));
        let mut buf = [0u8; 128];
        let n = g.fill(7, &mut buf);
        let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
        assert_eq!(n, 2 + len);
        let body = std::str::from_utf8(&buf[2..2 + len]).unwrap();
        assert!(body.contains("55=MSFT"));
        assert!(body.contains("34=7"));
    }

    #[test]
    fn custom_callback() {
        let spec = PayloadSpec::Custom(Arc::new(|seq, buf| {
            buf.fill(0xAB);
            if !buf.is_empty() {
                buf[0] = (seq & 0xFF) as u8;
            }
        }));
        let mut g = PayloadGenerator::new(spec);
        let mut buf = [0u8; 4];
        g.fill(9, &mut buf);
        assert_eq!(buf[0], 9);
        assert_eq!(buf[1], 0xAB);
    }

    #[test]
    fn build_udp_frame_has_ethertype_ipv4() {
        let frame = build_udp_frame(&[1, 2, 3, 4], 9000);
        assert_eq!(frame.len(), NET_HEADER_LEN + 4);
        assert_eq!(u16::from_be_bytes([frame[12], frame[13]]), 0x0800);
        assert_eq!(u16::from_be_bytes([frame[36], frame[37]]), 9000);
        assert_eq!(&frame[NET_HEADER_LEN..], &[1, 2, 3, 4]);
    }
}
