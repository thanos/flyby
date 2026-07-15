//! Metric keys for the networking subsystem.
//!
//! Every network backend records these counters through the
//! [`flyby_core::MetricsCollector`] trait. Using a typed enum prevents
//! typos in metric names and makes the full schema auditable at compile time.
//!
//! ## Required metrics
//!
//! Per the spec (Part IV §19), the networking backend must expose:
//!
//! | Key | Kind | Description |
//! |-----|------|-------------|
//! | `net.packets_received` | Counter | Frames successfully read from the backend |
//! | `net.packets_dropped` | Counter | Frames discarded due to back-pressure or errors |
//! | `net.bytes_received` | Counter | Total bytes across all received frames |
//! | `net.batches_polled` | Counter | Number of `poll_batch` calls |
//! | `net.batch_size` | Histogram | Packets per non-empty poll |
//! | `net.rx_ring_occupancy` | Gauge | Fill level of the RX ring (0.0–1.0) |
//! | `net.fill_ring_starvation` | Counter | Times the fill ring ran out of free buffers |
//! | `net.parse_failures` | Counter | Decoder errors on received frames |
//! | `net.sink_write_failures` | Counter | Frames lost because the sink was full |

use flyby_core::{MetricKey, MetricKind};

/// A typed metric key for networking observability.
///
/// Pass to [`flyby_core::MetricsCollector::record`] with the appropriate
/// [`MetricKind`] (see the table in the module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetMetricKey {
    /// Frames successfully read from the backend.
    PacketsReceived,
    /// Frames discarded by back-pressure or read error.
    PacketsDropped,
    /// Cumulative bytes across all received frames.
    BytesReceived,
    /// Number of `poll_batch` invocations.
    BatchesPolled,
    /// Distribution of packets per non-empty poll.
    BatchSize,
    /// Fill level of the RX ring (0.0 = empty, 1.0 = full).
    RxRingOccupancy,
    /// Times the fill ring ran dry (AF_XDP-specific).
    FillRingStarvation,
    /// Decoder failures on received frames.
    ParseFailures,
    /// Frames lost because the downstream sink was full.
    SinkWriteFailures,
}

impl NetMetricKey {
    /// The canonical metric kind for this key.
    pub fn kind(self) -> MetricKind {
        match self {
            Self::RxRingOccupancy => MetricKind::Gauge,
            Self::BatchSize => MetricKind::Histogram,
            _ => MetricKind::Counter,
        }
    }
}

impl MetricKey for NetMetricKey {
    fn name(&self) -> &str {
        match self {
            Self::PacketsReceived => "net.packets_received",
            Self::PacketsDropped => "net.packets_dropped",
            Self::BytesReceived => "net.bytes_received",
            Self::BatchesPolled => "net.batches_polled",
            Self::BatchSize => "net.batch_size",
            Self::RxRingOccupancy => "net.rx_ring_occupancy",
            Self::FillRingStarvation => "net.fill_ring_starvation",
            Self::ParseFailures => "net.parse_failures",
            Self::SinkWriteFailures => "net.sink_write_failures",
        }
    }
}
