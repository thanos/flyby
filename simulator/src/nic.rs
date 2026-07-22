//! Virtual NIC: a [`NetworkSource`] implementation backed by the simulator.
//!
//! [`VirtualNic`] wraps [`SimulatedNetSource`] and adds:
//!
//! - **Traffic pacing**: [`TrafficPacer`] limits packets per scheduler tick.
//! - **Fault injection**: drop and corrupt packets via [`FaultInjector`];
//!   dropped packets are removed from the delivered batch.
//! - **Event tracing**: emits a [`SimEvent`] for every packet generated,
//!   dropped, or corrupted.
//! - **Metrics**: records [`SimMetricKey`] samples when a collector is attached.
//!
//! ## Production parity
//!
//! `VirtualNic` implements exactly the same traits as the AF_XDP and DPDK
//! backends:
//!
//! ```text
//! Lifecycle + NetworkSource
//! ```
//!
//! Pipeline code that accepts `impl NetworkSource` works identically with a
//! `VirtualNic` and with a real backend.

use std::sync::Arc;

use flyby_core::{Error, ErrorKind, Lifecycle, MetricsCollector, NullCollector, Result};
use flyby_net::{NetworkSource, PushResult, RawBatch, SimNetConfig, SimulatedNetSource};

use crate::events::{EventSink, SimEvent, SimEventKind};
use crate::fault::{FaultInjector, FaultSpec};
use crate::metrics::SimMetricKey;
use crate::traffic::{TrafficConfig, TrafficPacer};

/// Configuration for a virtual NIC.
#[derive(Debug, Clone)]
pub struct VirtualNicConfig {
    /// Name used in log messages and metric labels.
    pub name: &'static str,
    /// Traffic generation parameters.
    pub traffic: TrafficConfig,
    /// Fault injection policy.
    pub fault: FaultSpec,
    /// Seed for the fault injector LCG (controls reproducibility).
    pub fault_seed: u64,
}

impl Default for VirtualNicConfig {
    fn default() -> Self {
        Self {
            name: "nic0",
            traffic: TrafficConfig::default(),
            fault: FaultSpec::default(),
            fault_seed: 0,
        }
    }
}

/// A simulated network interface card.
///
/// Implements [`NetworkSource`] so it is interchangeable with real AF_XDP
/// or DPDK backends.  Create one per simulated NIC; pass to the pipeline's
/// `poll_batch` loop.
pub struct VirtualNic<E: EventSink> {
    config: VirtualNicConfig,
    inner: SimulatedNetSource,
    /// Scratch batch filled by the inner source before fault filtering.
    scratch: RawBatch,
    pacer: TrafficPacer,
    fault: FaultInjector,
    events: E,
    metrics: Arc<dyn MetricsCollector>,
    /// Total packets emitted (pre-fault-injection).
    pub packets_generated: u64,
    /// Packets dropped by fault injection.
    pub packets_dropped: u64,
    /// Packets corrupted by fault injection.
    pub packets_corrupted: u64,
    /// Latency spike nanoseconds accumulated since last drain (virtual time).
    pub pending_spike_ns: u64,
    /// Last tick context (set by the scheduler before each poll).
    tick_ns: u64,
    /// Simulator clock at the start of the current tick.
    clock_ns: u64,
    initialized: bool,
}

impl<E: EventSink> VirtualNic<E> {
    /// Create a virtual NIC with the given config and event sink.
    pub fn new(config: VirtualNicConfig, events: E) -> Self {
        Self::with_metrics(config, events, Arc::new(NullCollector))
    }

    /// Create a virtual NIC with a custom metrics collector.
    pub fn with_metrics(
        config: VirtualNicConfig,
        events: E,
        metrics: Arc<dyn MetricsCollector>,
    ) -> Self {
        let frame_size = 42 + config.traffic.payload_size.max(1);
        let batch_cap = config.traffic.batch_size.max(1);
        let sim_config = SimNetConfig {
            batch_size: batch_cap,
            payload_size: config.traffic.payload_size,
            drop_rate: 0.0, // fault injection handled here, not in inner source
            idle_rate: 0.0,
            udp_dst_port: 9000,
        };
        let fault = FaultInjector::new(config.fault.clone(), config.fault_seed);
        let pacer = TrafficPacer::new(config.traffic.clone());
        Self {
            inner: SimulatedNetSource::new(sim_config),
            scratch: RawBatch::new(batch_cap, frame_size),
            pacer,
            config,
            fault,
            events,
            metrics,
            packets_generated: 0,
            packets_dropped: 0,
            packets_corrupted: 0,
            pending_spike_ns: 0,
            tick_ns: 1_000_000,
            clock_ns: 0,
            initialized: false,
        }
    }

    /// Name of this NIC, as set in [`VirtualNicConfig::name`].
    pub fn name(&self) -> &'static str {
        self.config.name
    }

    /// Provide tick context used by the next [`poll_batch`][NetworkSource::poll_batch].
    ///
    /// The scheduler calls this before each poll so the NIC can pace traffic
    /// and stamp events with the simulator clock.
    pub fn set_tick_context(&mut self, tick_ns: u64, clock_ns: u64) {
        self.tick_ns = tick_ns;
        self.clock_ns = clock_ns;
    }

    /// Take and clear accumulated latency-spike nanoseconds.
    pub fn take_spike_ns(&mut self) -> u64 {
        let ns = self.pending_spike_ns;
        self.pending_spike_ns = 0;
        ns
    }

    fn emit(&self, kind: SimEventKind) {
        self.events.emit(SimEvent {
            clock_ns: self.clock_ns,
            kind,
        });
    }
}

impl<E: EventSink> Lifecycle for VirtualNic<E> {
    fn init(&mut self) -> Result<()> {
        self.inner.init()?;
        self.packets_generated = 0;
        self.packets_dropped = 0;
        self.packets_corrupted = 0;
        self.pending_spike_ns = 0;
        self.pacer = TrafficPacer::new(self.config.traffic.clone());
        self.initialized = true;
        self.emit(SimEventKind::SimulatorStarted {
            scenario: self.config.name.to_string(),
        });
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.inner.shutdown()?;
        self.initialized = false;
        Ok(())
    }
}

impl<E: EventSink> NetworkSource for VirtualNic<E> {
    fn poll_batch(&mut self, batch: &mut RawBatch) -> Result<usize> {
        if !self.initialized {
            return Err(Error::new(
                ErrorKind::Lifecycle,
                format!(
                    "VirtualNic '{}': call init() before poll_batch()",
                    self.config.name
                ),
            ));
        }

        let intended = self.pacer.packets_for_tick(self.tick_ns);
        if intended == 0 {
            batch.reset(42 + self.config.traffic.payload_size.max(1));
            return Ok(0);
        }

        self.inner.set_batch_size(intended);
        let raw_count = self.inner.poll_batch(&mut self.scratch)?;

        let name = self.config.name;
        let frame_size = 42 + self.config.traffic.payload_size.max(1);
        batch.reset(frame_size);

        // Snapshot scratch slots so we can mutate payloads independently.
        let packets: Vec<(Vec<u8>, flyby_net::PacketMeta)> = self
            .scratch
            .packets()
            .take(raw_count)
            .map(|(data, meta)| (data.to_vec(), *meta))
            .collect();

        let mut delivered = 0usize;
        let mut bytes = 0u64;

        for (i, (mut payload, mut meta)) in packets.into_iter().enumerate() {
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
                self.fault.corrupt_payload(&mut payload);
                self.packets_corrupted += 1;
                self.emit(SimEventKind::PacketCorrupted { nic: name, seq });
                self.metrics
                    .record_counter(&SimMetricKey::PacketsCorrupted, 1);
            }

            let spike = self.fault.should_spike();
            if spike > 0 {
                self.pending_spike_ns = self.pending_spike_ns.saturating_add(spike);
            }

            // Prefer simulator clock over the inner source's synthetic stamp.
            meta.timestamp_ns = self.clock_ns.saturating_add(i as u64);
            match batch.push(&payload, meta) {
                PushResult::Ok | PushResult::Truncated => {
                    delivered += 1;
                    bytes += payload.len() as u64;
                    self.emit(SimEventKind::PacketGenerated {
                        nic: name,
                        len: payload.len(),
                        seq,
                    });
                }
                PushResult::Full => {
                    self.packets_dropped += 1;
                    batch.record_drop();
                    self.emit(SimEventKind::QueueOverflow { ring: name });
                    self.emit(SimEventKind::PacketDropped { nic: name, seq });
                    self.metrics
                        .record_counter(&SimMetricKey::PacketsDropped, 1);
                }
            }
        }

        if delivered > 0 {
            self.metrics
                .record_counter(&SimMetricKey::PacketsGenerated, delivered as u64);
            self.metrics
                .record_counter(&SimMetricKey::ThroughputBytes, bytes);
        }

        Ok(delivered)
    }

    fn backend_name(&self) -> &'static str {
        "virtual_nic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{NullEventSink, VecEventSink};
    use crate::traffic::TrafficPattern;

    fn make_nic(fault: FaultSpec) -> VirtualNic<VecEventSink> {
        let sink = VecEventSink::new();
        let cfg = VirtualNicConfig {
            traffic: TrafficConfig {
                batch_size: 8,
                payload_size: 8,
                ..TrafficConfig::default()
            },
            fault,
            ..VirtualNicConfig::default()
        };
        VirtualNic::new(cfg, sink)
    }

    #[test]
    fn poll_without_init_returns_error() {
        let mut nic = make_nic(FaultSpec::default());
        let mut batch = RawBatch::new(8, 128);
        assert!(nic.poll_batch(&mut batch).is_err());
    }

    #[test]
    fn clean_nic_emits_packets_generated_events() {
        let sink = VecEventSink::new();
        let cfg = VirtualNicConfig {
            traffic: TrafficConfig {
                batch_size: 4,
                payload_size: 8,
                pattern: TrafficPattern::FullSpeed,
            },
            fault: FaultSpec::default(),
            ..VirtualNicConfig::default()
        };
        let mut nic = VirtualNic::new(cfg, sink.clone());
        nic.init().unwrap();
        nic.set_tick_context(1_000_000, 0);

        let mut batch = RawBatch::new(8, 128);
        let n = nic.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 4);
        assert_eq!(batch.len(), 4);

        let events = sink.events();
        let generated = events
            .iter()
            .filter(|e| matches!(e.kind, SimEventKind::PacketGenerated { .. }))
            .count();
        assert_eq!(generated, 4);
    }

    #[test]
    fn full_drop_rate_removes_all_packets_from_batch() {
        let mut nic = make_nic(FaultSpec {
            drop_rate: 1.0,
            ..FaultSpec::default()
        });
        nic.init().unwrap();
        // Force full-speed so we attempt packets even with FixedRate default.
        nic.pacer = TrafficPacer::new(TrafficConfig {
            pattern: TrafficPattern::FullSpeed,
            batch_size: 8,
            payload_size: 8,
        });
        nic.set_tick_context(1_000_000, 0);

        let mut batch = RawBatch::new(8, 128);
        let n = nic.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 0);
        assert!(batch.is_empty());
        assert!(nic.packets_dropped > 0);
        assert_eq!(nic.packets_dropped, nic.packets_generated);
    }

    #[test]
    fn corrupt_rate_mutates_payload() {
        let sink = VecEventSink::new();
        let cfg = VirtualNicConfig {
            traffic: TrafficConfig {
                pattern: TrafficPattern::FullSpeed,
                batch_size: 4,
                payload_size: 16,
            },
            fault: FaultSpec {
                corrupt_rate: 1.0,
                ..FaultSpec::default()
            },
            ..VirtualNicConfig::default()
        };
        let mut nic = VirtualNic::new(cfg, sink.clone());
        nic.init().unwrap();
        nic.set_tick_context(1_000_000, 0);

        let mut batch = RawBatch::new(8, 128);
        nic.poll_batch(&mut batch).unwrap();
        assert_eq!(nic.packets_corrupted, batch.len() as u64);

        let events = sink.events();
        let corrupted = events
            .iter()
            .filter(|e| matches!(e.kind, SimEventKind::PacketCorrupted { .. }))
            .count();
        assert_eq!(corrupted, batch.len());
    }

    #[test]
    fn latency_spike_accumulates() {
        let mut nic = make_nic(FaultSpec {
            latency_spike_rate: 1.0,
            latency_spike_ns: 50_000,
            ..FaultSpec::default()
        });
        nic.init().unwrap();
        nic.pacer = TrafficPacer::new(TrafficConfig {
            pattern: TrafficPattern::FullSpeed,
            batch_size: 2,
            payload_size: 8,
        });
        nic.set_tick_context(1_000_000, 0);
        let mut batch = RawBatch::new(8, 128);
        nic.poll_batch(&mut batch).unwrap();
        assert_eq!(nic.take_spike_ns(), 100_000);
        assert_eq!(nic.take_spike_ns(), 0);
    }

    #[test]
    fn pacing_zero_intended_returns_empty() {
        let mut nic = make_nic(FaultSpec::default());
        nic.init().unwrap();
        nic.pacer = TrafficPacer::new(TrafficConfig {
            pattern: TrafficPattern::FixedRate { pps: 1 },
            batch_size: 64,
            payload_size: 8,
        });
        // 1 pps needs 1s; a 1 µs tick yields 0 packets
        nic.set_tick_context(1_000, 0);
        let mut batch = RawBatch::new(8, 128);
        let n = nic.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 0);
        assert!(batch.is_empty());
    }

    #[test]
    fn backend_name_is_virtual_nic() {
        let sink = NullEventSink;
        let mut nic = VirtualNic::new(VirtualNicConfig::default(), sink);
        nic.init().unwrap();
        assert_eq!(nic.backend_name(), "virtual_nic");
    }

    #[test]
    fn shutdown_then_reinit_resets_counters() {
        let mut nic = make_nic(FaultSpec::default());
        nic.pacer = TrafficPacer::new(TrafficConfig {
            pattern: TrafficPattern::FullSpeed,
            batch_size: 4,
            payload_size: 8,
        });
        nic.init().unwrap();
        nic.set_tick_context(1_000_000, 0);
        let mut batch = RawBatch::new(4, 128);
        nic.poll_batch(&mut batch).unwrap();
        assert!(nic.packets_generated > 0);

        nic.shutdown().unwrap();
        nic.init().unwrap();
        assert_eq!(nic.packets_generated, 0);
    }
}
