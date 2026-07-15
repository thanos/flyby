//! Metric keys for the storage subsystem.

use flyby_core::MetricKey;

/// Metric keys emitted by storage backends.
///
/// All keys are in the `"storage.*"` namespace.  Counters are cumulative
/// since the source was last initialised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageMetricKey {
    /// Total bytes read from the underlying file or device.
    BytesRead,
    /// Total records emitted by the source.
    RecordsRead,
    /// Records skipped due to framing or parse errors.
    ParseErrors,
    /// Source has reached EOF.
    EofReached,
    /// Number of times the source has looped (only non-zero with [`EofPolicy::Loop`][crate::config::EofPolicy::Loop]).
    LoopCount,
    /// Replay lag in nanoseconds: wall time minus the most recent record timestamp.
    ReplayLagNs,
    /// Nanoseconds per read syscall (latency percentile requires external aggregation).
    ReadLatencyNs,
    /// io_uring submission queue full events (io_uring backend only).
    IoUringSqFull,
    /// io_uring completion queue overflow events (io_uring backend only).
    IoUringCqOverflow,
}

impl MetricKey for StorageMetricKey {
    fn name(&self) -> &'static str {
        match self {
            Self::BytesRead => "storage.bytes_read",
            Self::RecordsRead => "storage.records_read",
            Self::ParseErrors => "storage.parse_errors",
            Self::EofReached => "storage.eof_reached",
            Self::LoopCount => "storage.loop_count",
            Self::ReplayLagNs => "storage.replay_lag_ns",
            Self::ReadLatencyNs => "storage.read_latency_ns",
            Self::IoUringSqFull => "storage.io_uring.sq_full",
            Self::IoUringCqOverflow => "storage.io_uring.cq_overflow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_have_storage_prefix() {
        let keys = [
            StorageMetricKey::BytesRead,
            StorageMetricKey::RecordsRead,
            StorageMetricKey::ParseErrors,
            StorageMetricKey::EofReached,
            StorageMetricKey::LoopCount,
            StorageMetricKey::ReplayLagNs,
            StorageMetricKey::ReadLatencyNs,
            StorageMetricKey::IoUringSqFull,
            StorageMetricKey::IoUringCqOverflow,
        ];
        for key in keys {
            assert!(
                key.name().starts_with("storage."),
                "metric {:?} name '{}' does not start with 'storage.'",
                key,
                key.name(),
            );
        }
    }

    #[test]
    fn all_names_unique() {
        use std::collections::HashSet;
        let names: HashSet<_> = [
            StorageMetricKey::BytesRead,
            StorageMetricKey::RecordsRead,
            StorageMetricKey::ParseErrors,
            StorageMetricKey::EofReached,
            StorageMetricKey::LoopCount,
            StorageMetricKey::ReplayLagNs,
            StorageMetricKey::ReadLatencyNs,
            StorageMetricKey::IoUringSqFull,
            StorageMetricKey::IoUringCqOverflow,
        ]
        .iter()
        .map(|k| k.name())
        .collect();
        assert_eq!(names.len(), 9);
    }
}
