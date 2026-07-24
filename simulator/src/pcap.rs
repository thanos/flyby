//! Classic pcap ingest for the simulator.
//!
//! Reads libpcap *classic* capture files (microsecond or nanosecond resolution,
//! little- or big-endian) into memory and exposes them as a
//! [`NetworkSource`] driven by [`SimReplay`].
//!
//! PCAP-NG is not supported in v0.1 — convert with `editcap -F pcap` first.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use flyby_core::{Error, ErrorKind, Lifecycle, MetricsCollector, NullCollector, Result};
use flyby_net::{NetworkSource, PacketMeta, PushResult, RawBatch};
use flyby_storage::ReplayMode;

use crate::events::{EventSink, SimEvent, SimEventKind};
use crate::fault::{FaultInjector, FaultSpec};
use crate::metrics::SimMetricKey;
use crate::replay::SimReplay;

/// One packet loaded from a pcap file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcapPacket {
    /// Capture timestamp in nanoseconds since the capture epoch.
    pub timestamp_ns: u64,
    /// Raw link-layer frame bytes (as stored in the pcap).
    pub data: Vec<u8>,
}

/// Configuration for a [`PcapSource`].
#[derive(Debug, Clone)]
pub struct PcapConfig {
    /// Source name used in events / metrics.
    pub name: &'static str,
    /// Replay timing mode (virtual-clock aware via [`SimReplay`]).
    pub replay: ReplayMode,
    /// Optional fault injection applied after a packet is selected.
    pub fault: FaultSpec,
    /// Fault injector seed.
    pub fault_seed: u64,
    /// When `true`, wrap to the first packet after exhausting the capture.
    pub loop_capture: bool,
}

impl Default for PcapConfig {
    fn default() -> Self {
        Self {
            name: "pcap0",
            replay: ReplayMode::OriginalTiming,
            fault: FaultSpec::default(),
            fault_seed: 0,
            loop_capture: false,
        }
    }
}

/// Parse a classic pcap byte buffer into packets.
///
/// # Errors
///
/// Returns [`ErrorKind::Decode`] when the
/// header is not a recognised classic pcap magic, or the file is truncated.
pub fn parse_pcap(bytes: &[u8]) -> Result<Vec<PcapPacket>> {
    if bytes.len() < 24 {
        return Err(Error::new(
            ErrorKind::Decode,
            "pcap: file shorter than 24-byte global header",
        ));
    }

    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let (swap, nanos) = match magic {
        0xa1b2_c3d4 => (false, false), // us, little
        0xd4c3_b2a1 => (true, false),  // us, big (byte-swapped)
        0xa1b2_3c4d => (false, true),  // ns, little
        0x4d3c_b2a1 => (true, true),   // ns, big
        other => {
            return Err(Error::new(
                ErrorKind::Decode,
                format!("pcap: unsupported magic 0x{other:08x} (pcap-ng not supported)"),
            ));
        }
    };

    let read_u16 = |off: usize| -> u16 {
        let v = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap());
        if swap { v.swap_bytes() } else { v }
    };
    let read_u32 = |off: usize| -> u32 {
        let v = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        if swap { v.swap_bytes() } else { v }
    };

    let _version_major = read_u16(4);
    let _version_minor = read_u16(6);
    let _thiszone = read_u32(8);
    let _sigfigs = read_u32(12);
    let _snaplen = read_u32(16);
    let _network = read_u32(20);

    let mut packets = Vec::new();
    let mut off = 24usize;
    while off + 16 <= bytes.len() {
        let ts_sec = read_u32(off) as u64;
        let ts_frac = read_u32(off + 4) as u64;
        let incl_len = read_u32(off + 8) as usize;
        let _orig_len = read_u32(off + 12) as usize;
        off += 16;

        if off + incl_len > bytes.len() {
            return Err(Error::new(
                ErrorKind::Decode,
                format!(
                    "pcap: truncated packet at offset {off}: need {incl_len} bytes, have {}",
                    bytes.len().saturating_sub(off)
                ),
            ));
        }

        let timestamp_ns = if nanos {
            ts_sec.saturating_mul(1_000_000_000).saturating_add(ts_frac)
        } else {
            ts_sec
                .saturating_mul(1_000_000_000)
                .saturating_add(ts_frac.saturating_mul(1_000))
        };

        packets.push(PcapPacket {
            timestamp_ns,
            data: bytes[off..off + incl_len].to_vec(),
        });
        off += incl_len;
    }

    Ok(packets)
}

/// Load and parse a classic pcap file from disk.
pub fn load_pcap(path: impl AsRef<Path>) -> Result<Vec<PcapPacket>> {
    let bytes = fs::read(path.as_ref()).map_err(|e| {
        Error::new(
            ErrorKind::Io,
            format!("pcap: failed to read {}: {e}", path.as_ref().display()),
        )
    })?;
    parse_pcap(&bytes)
}

/// Write a classic pcap (little-endian) with microsecond or nanosecond timestamps.
pub fn write_pcap_bytes_ex(packets: &[(u64, &[u8])], nanosecond: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let magic: u32 = if nanosecond { 0xa1b2_3c4d } else { 0xa1b2_c3d4 };
    out.extend_from_slice(&magic.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // thiszone
    out.extend_from_slice(&0u32.to_le_bytes()); // sigfigs
    out.extend_from_slice(&65535u32.to_le_bytes()); // snaplen
    out.extend_from_slice(&1u32.to_le_bytes()); // LINKTYPE_ETHERNET

    for (ts_ns, data) in packets {
        let sec = (*ts_ns / 1_000_000_000) as u32;
        let frac = if nanosecond {
            (*ts_ns % 1_000_000_000) as u32
        } else {
            ((*ts_ns % 1_000_000_000) / 1_000) as u32
        };
        out.extend_from_slice(&sec.to_le_bytes());
        out.extend_from_slice(&frac.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Write a minimal classic (microsecond, little-endian) pcap for tests/tools.
pub fn write_pcap_bytes(packets: &[(u64, &[u8])]) -> Vec<u8> {
    write_pcap_bytes_ex(packets, false)
}

/// A [`NetworkSource`] that replays packets from a classic pcap capture.
pub struct PcapSource<E: EventSink> {
    config: PcapConfig,
    packets: Vec<PcapPacket>,
    cursor: usize,
    replay: SimReplay,
    fault: FaultInjector,
    events: E,
    metrics: Arc<dyn MetricsCollector>,
    /// Packets examined (including drops).
    pub packets_generated: u64,
    /// Packets dropped by fault injection.
    pub packets_dropped: u64,
    /// Packets corrupted by fault injection.
    pub packets_corrupted: u64,
    /// Latency spike nanoseconds accumulated since last drain.
    pub pending_spike_ns: u64,
    clock_ns: u64,
    tick_ns: u64,
    initialized: bool,
    exhausted: bool,
}

impl<E: EventSink> PcapSource<E> {
    /// Create a source from already-parsed packets.
    pub fn new(packets: Vec<PcapPacket>, config: PcapConfig, events: E) -> Result<Self> {
        Self::with_metrics(packets, config, events, Arc::new(NullCollector))
    }

    /// Create a source with a custom metrics collector.
    pub fn with_metrics(
        packets: Vec<PcapPacket>,
        config: PcapConfig,
        events: E,
        metrics: Arc<dyn MetricsCollector>,
    ) -> Result<Self> {
        let replay = SimReplay::new(config.replay.clone())?;
        let fault = FaultInjector::new(config.fault.clone(), config.fault_seed);
        Ok(Self {
            config,
            packets,
            cursor: 0,
            replay,
            fault,
            events,
            metrics,
            packets_generated: 0,
            packets_dropped: 0,
            packets_corrupted: 0,
            pending_spike_ns: 0,
            clock_ns: 0,
            tick_ns: 1_000_000,
            initialized: false,
            exhausted: false,
        })
    }

    /// Load a pcap file from disk.
    pub fn from_path(path: impl AsRef<Path>, config: PcapConfig, events: E) -> Result<Self> {
        let packets = load_pcap(path)?;
        Self::new(packets, config, events)
    }

    /// Source name.
    pub fn name(&self) -> &'static str {
        self.config.name
    }

    /// Number of packets in the capture.
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    /// `true` if the capture contains no packets.
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    /// Provide tick / clock context (same contract as [`VirtualNic`][crate::nic::VirtualNic]).
    pub fn set_tick_context(&mut self, tick_ns: u64, clock_ns: u64) {
        self.tick_ns = tick_ns;
        self.clock_ns = clock_ns;
    }

    /// Take accumulated latency-spike nanoseconds.
    pub fn take_spike_ns(&mut self) -> u64 {
        let ns = self.pending_spike_ns;
        self.pending_spike_ns = 0;
        ns
    }

    /// Traffic hot-swap is a no-op for pcap replay sources.
    pub fn set_traffic(&mut self, _traffic: crate::traffic::TrafficConfig) {}

    /// Replace the fault injection policy (timeline / DSL hot-swap).
    pub fn set_fault(&mut self, fault: FaultSpec) {
        self.config.fault = fault.clone();
        self.fault = FaultInjector::new(fault, self.config.fault_seed);
    }

    /// Arm single-step replay.
    pub fn advance_replay(&mut self) {
        self.replay.advance();
    }

    /// Pause / resume replay emission.
    pub fn pause_replay(&mut self) {
        self.replay.pause();
    }

    /// Resume after [`pause_replay`][Self::pause_replay].
    pub fn resume_replay(&mut self) {
        self.replay.resume();
    }

    fn emit(&self, kind: SimEventKind) {
        self.events.emit(SimEvent {
            clock_ns: self.clock_ns,
            kind,
        });
    }
}

impl<E: EventSink> Lifecycle for PcapSource<E> {
    fn init(&mut self) -> Result<()> {
        self.cursor = 0;
        self.packets_generated = 0;
        self.packets_dropped = 0;
        self.packets_corrupted = 0;
        self.pending_spike_ns = 0;
        self.exhausted = false;
        self.replay = SimReplay::new(self.config.replay.clone())?;
        self.initialized = true;
        self.emit(SimEventKind::SimulatorStarted {
            scenario: self.config.name.to_string(),
        });
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.initialized = false;
        Ok(())
    }
}

impl<E: EventSink> NetworkSource for PcapSource<E> {
    fn poll_batch(&mut self, batch: &mut RawBatch) -> Result<usize> {
        if !self.initialized {
            return Err(Error::new(
                ErrorKind::Lifecycle,
                format!(
                    "PcapSource '{}': call init() before poll_batch()",
                    self.config.name
                ),
            ));
        }

        let max_frame = self
            .packets
            .iter()
            .map(|p| p.data.len())
            .max()
            .unwrap_or(64)
            .max(64);
        batch.reset(max_frame);

        if self.packets.is_empty() {
            self.exhausted = true;
            return Ok(0);
        }

        let name = self.config.name;
        let mut delivered = 0usize;

        while delivered < batch.capacity() {
            if self.cursor >= self.packets.len() {
                if self.config.loop_capture {
                    self.cursor = 0;
                    // Re-anchor timing on loop for OriginalTiming / TimeScaled.
                    self.replay = SimReplay::new(self.config.replay.clone())?;
                } else {
                    self.exhausted = true;
                    break;
                }
            }

            let pkt = &self.packets[self.cursor];
            if !self.replay.ready_at(pkt.timestamp_ns, self.clock_ns) {
                break;
            }

            let mut data = pkt.data.clone();
            let ts = pkt.timestamp_ns;
            self.cursor += 1;
            self.packets_generated += 1;
            let seq = self.packets_generated;

            if self.fault.should_drop() {
                self.packets_dropped += 1;
                batch.record_drop();
                self.emit(SimEventKind::PacketDropped { nic: name, seq });
                self.metrics
                    .record_counter(&SimMetricKey::PacketsDropped, 1);
                continue;
            }

            if self.fault.should_corrupt() {
                self.fault.corrupt_payload(&mut data);
                self.packets_corrupted += 1;
                self.emit(SimEventKind::PacketCorrupted { nic: name, seq });
                self.metrics
                    .record_counter(&SimMetricKey::PacketsCorrupted, 1);
            }

            let spike = self.fault.should_spike();
            if spike > 0 {
                self.pending_spike_ns = self.pending_spike_ns.saturating_add(spike);
            }

            let meta = PacketMeta {
                timestamp_ns: ts,
                queue_id: 0,
                original_len: data.len().min(u32::MAX as usize) as u32,
            };
            match batch.push(&data, meta) {
                PushResult::Ok | PushResult::Truncated => {
                    delivered += 1;
                    self.emit(SimEventKind::PacketGenerated {
                        nic: name,
                        len: data.len(),
                        seq,
                    });
                    self.metrics
                        .record_counter(&SimMetricKey::PacketsGenerated, 1);
                    self.metrics
                        .record_counter(&SimMetricKey::ThroughputBytes, data.len() as u64);
                }
                PushResult::Full => {
                    // Put the packet back so it is retried next poll.
                    self.cursor = self.cursor.saturating_sub(1);
                    self.packets_generated = self.packets_generated.saturating_sub(1);
                    break;
                }
            }
        }

        let _ = self.tick_ns; // reserved for future rate-limited pcap modes
        Ok(delivered)
    }

    fn backend_name(&self) -> &'static str {
        "pcap"
    }
}

/// Helper to silence unused Cursor import if we switch away — keep Read available
/// for callers that stream. (Cursor used in tests.)
#[allow(dead_code)]
fn _read_all(r: &mut dyn Read) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf)
        .map_err(|e| Error::new(ErrorKind::Io, format!("pcap read: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{NullEventSink, VecEventSink};
    use crate::fault::FaultSpec;
    use flyby_core::Lifecycle;
    use flyby_net::NetworkSource;
    use std::io::{Cursor, Write};
    use tempfile::NamedTempFile;

    fn sample_capture() -> Vec<u8> {
        write_pcap_bytes(&[
            (0, &[0u8; 64]),
            (1_000_000, &[1u8; 64]), // 1 ms later
            (5_000_000, &[2u8; 32]), // 5 ms
        ])
    }

    #[test]
    fn parse_roundtrip() {
        let bytes = sample_capture();
        let pkts = parse_pcap(&bytes).unwrap();
        assert_eq!(pkts.len(), 3);
        assert_eq!(pkts[0].timestamp_ns, 0);
        assert_eq!(pkts[1].timestamp_ns, 1_000_000);
        assert_eq!(pkts[2].data.len(), 32);
    }

    #[test]
    fn load_from_tempfile() {
        let bytes = sample_capture();
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&bytes).unwrap();
        f.flush().unwrap();
        let pkts = load_pcap(f.path()).unwrap();
        assert_eq!(pkts.len(), 3);
    }

    #[test]
    fn full_speed_delivers_all() {
        let bytes = sample_capture();
        let pkts = parse_pcap(&bytes).unwrap();
        let mut src = PcapSource::new(
            pkts,
            PcapConfig {
                replay: ReplayMode::FullSpeed,
                ..PcapConfig::default()
            },
            NullEventSink,
        )
        .unwrap();
        src.init().unwrap();
        src.set_tick_context(1_000_000, 0);
        let mut batch = RawBatch::new(8, 128);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 3);
        assert!(src.exhausted);
    }

    #[test]
    fn original_timing_waits_for_clock() {
        let pkts = parse_pcap(&sample_capture()).unwrap();
        let sink = VecEventSink::new();
        let mut src = PcapSource::new(
            pkts,
            PcapConfig {
                replay: ReplayMode::OriginalTiming,
                ..PcapConfig::default()
            },
            sink,
        )
        .unwrap();
        src.init().unwrap();

        let mut batch = RawBatch::new(8, 128);
        src.set_tick_context(1_000_000, 0);
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 1); // only ts=0

        batch.reset(128);
        src.set_tick_context(1_000_000, 500_000);
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 0); // 1ms pkt not ready

        batch.reset(128);
        src.set_tick_context(1_000_000, 1_000_000);
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 1);
    }

    #[test]
    fn rejects_bad_magic() {
        let err = parse_pcap(&[0u8; 24]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode);
    }

    #[test]
    fn cursor_helper_reads() {
        let mut c = Cursor::new(vec![1u8, 2, 3]);
        assert_eq!(_read_all(&mut c).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn rejects_short_and_truncated() {
        assert!(parse_pcap(&[0u8; 8]).is_err());
        // Valid header + truncated packet record.
        let mut bytes = write_pcap_bytes(&[(0, &[0u8; 64])]);
        bytes.truncate(24 + 16 + 10); // header + pkt hdr + partial body
        assert!(parse_pcap(&bytes).is_err());
    }

    #[test]
    fn nanosecond_and_byte_swapped_magic() {
        let ns = write_pcap_bytes_ex(&[(1_500_000_000, &[9u8; 16])], true);
        let pkts = parse_pcap(&ns).unwrap();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].timestamp_ns, 1_500_000_000);

        // Byte-swap a microsecond LE capture into BE magic + fields.
        let le = write_pcap_bytes(&[(2_000_000, &[7u8; 8])]);
        let mut be = le.clone();
        // Swap magic to big-endian us form.
        be[0..4].copy_from_slice(&0xd4c3_b2a1u32.to_le_bytes());
        // Swap multi-byte fields in global header and packet header.
        for off in [4usize, 6] {
            be[off..off + 2].reverse();
        }
        for off in [8usize, 12, 16, 20, 24, 28, 32, 36] {
            be[off..off + 4].reverse();
        }
        let pkts = parse_pcap(&be).unwrap();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].data, vec![7u8; 8]);
    }

    #[test]
    fn loop_capture_and_fault_injection() {
        let pkts = parse_pcap(&sample_capture()).unwrap();
        // Drop everything without looping (loop+100% drop would spin forever).
        let mut src = PcapSource::new(
            pkts.clone(),
            PcapConfig {
                replay: ReplayMode::FullSpeed,
                loop_capture: false,
                fault: FaultSpec {
                    drop_rate: 1.0,
                    ..FaultSpec::default()
                },
                fault_seed: 1,
                ..PcapConfig::default()
            },
            VecEventSink::new(),
        )
        .unwrap();
        src.init().unwrap();
        src.set_tick_context(1_000_000, 0);
        let mut batch = RawBatch::new(8, 128);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 0);
        assert!(src.packets_dropped >= 3);
        assert!(!src.is_empty());
        assert_eq!(src.len(), 3);
        assert_eq!(src.name(), "pcap0");
        assert_eq!(src.backend_name(), "pcap");

        // Looping capture re-arms after exhaustion.
        let mut looped = PcapSource::new(
            pkts,
            PcapConfig {
                replay: ReplayMode::FullSpeed,
                loop_capture: true,
                ..PcapConfig::default()
            },
            NullEventSink,
        )
        .unwrap();
        looped.init().unwrap();
        looped.set_tick_context(1_000_000, 0);
        // Capacity larger than capture length so one poll wraps under loop_capture.
        let mut batch = RawBatch::new(8, 128);
        let n = looped.poll_batch(&mut batch).unwrap();
        assert!(n >= 3, "expected looped delivery, got {n}");
        assert!(!looped.exhausted);
    }

    #[test]
    fn pause_resume_advance_and_hot_swap() {
        let pkts = parse_pcap(&sample_capture()).unwrap();
        let mut src = PcapSource::new(
            pkts,
            PcapConfig {
                replay: ReplayMode::SingleStep,
                ..PcapConfig::default()
            },
            NullEventSink,
        )
        .unwrap();
        src.init().unwrap();
        src.pause_replay();
        src.set_tick_context(1_000_000, 0);
        let mut batch = RawBatch::new(4, 128);
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 0);
        src.resume_replay();
        src.advance_replay();
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 1);

        src.set_fault(FaultSpec {
            corrupt_rate: 1.0,
            latency_spike_rate: 1.0,
            latency_spike_ns: 100,
            ..FaultSpec::default()
        });
        src.set_traffic(crate::traffic::TrafficConfig::default());
        src.advance_replay();
        batch.reset(128);
        let _ = src.poll_batch(&mut batch).unwrap();
        // Fault policy is applied on the next armed step; either spike or corrupt may fire.
        let spiked = src.take_spike_ns();
        assert!(spiked > 0 || src.packets_corrupted > 0 || src.packets_generated > 0);

        src.shutdown().unwrap();
        assert!(src.poll_batch(&mut batch).is_err());
    }

    #[test]
    fn empty_capture_and_from_path() {
        let mut src = PcapSource::new(
            Vec::new(),
            PcapConfig {
                replay: ReplayMode::FullSpeed,
                ..PcapConfig::default()
            },
            NullEventSink,
        )
        .unwrap();
        assert!(src.is_empty());
        src.init().unwrap();
        let mut batch = RawBatch::new(2, 64);
        assert_eq!(src.poll_batch(&mut batch).unwrap(), 0);
        assert!(src.exhausted);

        let bytes = sample_capture();
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&bytes).unwrap();
        f.flush().unwrap();
        let loaded = PcapSource::from_path(
            f.path(),
            PcapConfig {
                replay: ReplayMode::FullSpeed,
                ..PcapConfig::default()
            },
            NullEventSink,
        )
        .unwrap();
        assert_eq!(loaded.len(), 3);
        assert!(load_pcap("/no/such/pcap.file").is_err());
    }
}
