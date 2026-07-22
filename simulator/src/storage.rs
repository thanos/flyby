//! Virtual storage source: a [`StorageSource`] implementation backed by
//! the simulator.
//!
//! [`VirtualStorageSource`] wraps [`FileSource`] and adds:
//!
//! - **Fault injection**: drop / corrupt records via [`FaultInjector`].
//!   Dropped records are removed from the delivered batch.
//! - **Event tracing**: emits [`SimEvent`]s for each record read or dropped.
//! - **Metrics**: records [`SimMetricKey`] samples when a collector is attached.
//!
//! ## Production parity
//!
//! `VirtualStorageSource<F>` implements the same traits as `FileSource<F>`,
//! `IoUringSource`, and `SpdkSource`:
//!
//! ```text
//! Lifecycle + StorageSource
//! ```

use std::sync::Arc;

use flyby_core::{Error, ErrorKind, Lifecycle, MetricsCollector, NullCollector, Result};
use flyby_storage::{
    FileConfig, FileSource, Frame, PushResult, RawRecordBatch, RecordMeta, StorageSource,
};

use crate::events::{EventSink, SimEvent, SimEventKind};
use crate::fault::{FaultInjector, FaultSpec};
use crate::metrics::SimMetricKey;

/// Configuration for a virtual storage source.
#[derive(Debug, Clone)]
pub struct VirtualStorageConfig {
    /// Backend file configuration.
    pub file: FileConfig,
    /// Fault injection policy.
    pub fault: FaultSpec,
    /// Seed for the fault injector LCG.
    pub fault_seed: u64,
    /// Label used in events (e.g. `"storage0"`).
    pub name: &'static str,
}

impl Default for VirtualStorageConfig {
    fn default() -> Self {
        Self {
            file: FileConfig::default(),
            fault: FaultSpec::default(),
            fault_seed: 0,
            name: "storage0",
        }
    }
}

/// A simulated storage source that wraps [`FileSource`] and adds fault
/// injection and event emission.
pub struct VirtualStorageSource<F: Frame, E: EventSink> {
    config: VirtualStorageConfig,
    inner: FileSource<F>,
    /// Scratch batch filled by the inner source before fault filtering.
    scratch: RawRecordBatch,
    fault: FaultInjector,
    events: E,
    metrics: Arc<dyn MetricsCollector>,
    /// Total records read from the underlying source.
    pub records_read: u64,
    /// Records dropped by fault injection.
    pub records_dropped: u64,
    /// Records corrupted by fault injection.
    pub records_corrupted: u64,
    /// Latency spike nanoseconds accumulated since last drain.
    pub pending_spike_ns: u64,
    /// Simulator clock for event stamps (set by the scheduler).
    clock_ns: u64,
    initialized: bool,
}

impl<F: Frame, E: EventSink> VirtualStorageSource<F, E> {
    /// Create a virtual storage source.
    pub fn new(config: VirtualStorageConfig, framer: F, events: E) -> Self {
        Self::with_metrics(config, framer, events, Arc::new(NullCollector))
    }

    /// Create a virtual storage source with a custom metrics collector.
    pub fn with_metrics(
        config: VirtualStorageConfig,
        framer: F,
        events: E,
        metrics: Arc<dyn MetricsCollector>,
    ) -> Self {
        let fault = FaultInjector::new(config.fault.clone(), config.fault_seed);
        let max_rec = config.file.max_record_size.max(1);
        let batch_cap = config.file.batch_size.max(1);
        let inner = FileSource::new(config.file.clone(), framer);
        Self {
            config,
            inner,
            scratch: RawRecordBatch::new(batch_cap, max_rec),
            fault,
            events,
            metrics,
            records_read: 0,
            records_dropped: 0,
            records_corrupted: 0,
            pending_spike_ns: 0,
            clock_ns: 0,
            initialized: false,
        }
    }

    /// Name of this storage source.
    pub fn name(&self) -> &'static str {
        self.config.name
    }

    /// Stamp subsequent events with the simulator clock.
    pub fn set_clock_ns(&mut self, clock_ns: u64) {
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

impl<F: Frame, E: EventSink> Lifecycle for VirtualStorageSource<F, E> {
    fn init(&mut self) -> Result<()> {
        self.inner.init()?;
        self.records_read = 0;
        self.records_dropped = 0;
        self.records_corrupted = 0;
        self.pending_spike_ns = 0;
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        self.inner.shutdown()?;
        self.initialized = false;
        Ok(())
    }
}

impl<F: Frame, E: EventSink> StorageSource for VirtualStorageSource<F, E> {
    fn poll_batch(&mut self, batch: &mut RawRecordBatch) -> Result<usize> {
        if !self.initialized {
            return Err(Error::new(
                ErrorKind::Lifecycle,
                format!(
                    "VirtualStorageSource '{}': call init() before poll_batch()",
                    self.config.name
                ),
            ));
        }

        let n = self.inner.poll_batch(&mut self.scratch)?;
        let backend = self.config.name;
        batch.reset();

        let records: Vec<(Vec<u8>, RecordMeta)> = self
            .scratch
            .records()
            .take(n)
            .map(|(data, meta)| (data.to_vec(), *meta))
            .collect();

        let mut delivered = 0usize;

        for (mut payload, meta) in records {
            self.records_read += 1;
            let offset = meta.file_offset;

            if self.fault.should_drop() {
                self.records_dropped += 1;
                self.emit(SimEventKind::RecordDropped { backend, offset });
                self.metrics
                    .record_counter(&SimMetricKey::RecordsDropped, 1);
                continue;
            }

            if self.fault.should_corrupt() {
                self.fault.corrupt_payload(&mut payload);
                self.records_corrupted += 1;
            }

            let spike = self.fault.should_spike();
            if spike > 0 {
                self.pending_spike_ns = self.pending_spike_ns.saturating_add(spike);
            }

            match batch.push(&payload, meta) {
                PushResult::Ok => {
                    delivered += 1;
                    self.emit(SimEventKind::RecordRead {
                        backend,
                        offset,
                        len: payload.len(),
                    });
                    self.metrics.record_counter(&SimMetricKey::RecordsRead, 1);
                    self.metrics
                        .record_counter(&SimMetricKey::ThroughputBytes, payload.len() as u64);
                }
                PushResult::Full | PushResult::Oversized => {
                    self.records_dropped += 1;
                    self.emit(SimEventKind::RecordDropped { backend, offset });
                    self.metrics
                        .record_counter(&SimMetricKey::RecordsDropped, 1);
                }
            }
        }

        Ok(delivered)
    }

    fn backend_name() -> &'static str
    where
        Self: Sized,
    {
        "virtual_storage"
    }

    fn is_exhausted(&self) -> bool {
        self.inner.is_exhausted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{NullEventSink, VecEventSink};
    use flyby_storage::{Delimiter, EofPolicy, FileConfig};
    use std::io::Write as IoWrite;
    use tempfile::NamedTempFile;

    fn make_delimited_file(records: &[&[u8]], delimiter: u8) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        for rec in records {
            f.write_all(rec).unwrap();
            f.write_all(&[delimiter]).unwrap();
        }
        f.flush().unwrap();
        f
    }

    fn make_source(path: std::path::PathBuf) -> VirtualStorageSource<Delimiter, VecEventSink> {
        let file_cfg = FileConfig {
            path,
            eof_policy: EofPolicy::Stop,
            ..FileConfig::default()
        };
        let cfg = VirtualStorageConfig {
            file: file_cfg,
            ..VirtualStorageConfig::default()
        };
        let sink = VecEventSink::new();
        VirtualStorageSource::new(cfg, Delimiter::new(b'\n', 1024), sink)
    }

    #[test]
    fn poll_without_init_returns_error() {
        let tmp = make_delimited_file(&[b"hello"], b'\n');
        let mut src = make_source(tmp.path().to_path_buf());
        let mut batch = RawRecordBatch::new(8, 1024);
        assert!(src.poll_batch(&mut batch).is_err());
    }

    #[test]
    fn reads_records_and_emits_events() {
        let tmp = make_delimited_file(&[b"line1", b"line2", b"line3"], b'\n');
        let sink = VecEventSink::new();
        let file_cfg = FileConfig {
            path: tmp.path().to_path_buf(),
            eof_policy: EofPolicy::Stop,
            ..FileConfig::default()
        };
        let cfg = VirtualStorageConfig {
            file: file_cfg,
            ..VirtualStorageConfig::default()
        };
        let mut src = VirtualStorageSource::new(cfg, Delimiter::new(b'\n', 1024), sink.clone());
        src.init().unwrap();

        let mut batch = RawRecordBatch::new(8, 1024);
        let n = src.poll_batch(&mut batch).unwrap();
        assert!(n > 0);
        assert_eq!(batch.len(), n);

        let events = sink.events();
        let read_events = events
            .iter()
            .filter(|e| matches!(e.kind, SimEventKind::RecordRead { .. }))
            .count();
        assert!(read_events > 0);
    }

    #[test]
    fn drop_fault_removes_records_from_batch() {
        let tmp = make_delimited_file(&[b"a", b"b", b"c", b"d", b"e", b"f", b"g", b"h"], b'\n');
        let file_cfg = FileConfig {
            path: tmp.path().to_path_buf(),
            eof_policy: EofPolicy::Stop,
            ..FileConfig::default()
        };
        let fault = FaultSpec {
            drop_rate: 1.0,
            ..FaultSpec::default()
        };
        let cfg = VirtualStorageConfig {
            file: file_cfg,
            fault,
            ..VirtualStorageConfig::default()
        };
        let mut src = VirtualStorageSource::new(cfg, Delimiter::new(b'\n', 1024), NullEventSink);
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(8, 1024);
        let n = src.poll_batch(&mut batch).unwrap();
        assert_eq!(n, 0);
        assert!(batch.is_empty());
        assert!(src.records_dropped > 0);
        assert_eq!(src.records_dropped, src.records_read);
    }

    #[test]
    fn is_exhausted_after_eof_stop() {
        let tmp = make_delimited_file(&[b"only"], b'\n');
        let mut src = make_source(tmp.path().to_path_buf());
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(8, 1024);
        loop {
            let n = src.poll_batch(&mut batch).unwrap();
            if n == 0 {
                break;
            }
        }
        assert!(src.is_exhausted());
    }

    #[test]
    fn reinit_resets_counters() {
        let tmp = make_delimited_file(&[b"x", b"y"], b'\n');
        let mut src = make_source(tmp.path().to_path_buf());
        src.init().unwrap();
        let mut batch = RawRecordBatch::new(8, 1024);
        src.poll_batch(&mut batch).unwrap();
        assert!(src.records_read > 0);

        src.shutdown().unwrap();
        src.init().unwrap();
        assert_eq!(src.records_read, 0);
    }
}
