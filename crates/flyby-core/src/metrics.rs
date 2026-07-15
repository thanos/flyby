//! The [`MetricsCollector`] trait: observability.
//!
//! Every stage reports counters, gauges, and histograms through a
//! metrics collector. The trait is intentionally minimal so that
//! backends can plug in `prometheus`, `metrics`, or a no-op collector
//! for benchmarks without coupling to a specific crate.
//!
//! Prefer monomorphized collectors on hot paths so [`NullCollector`]
//! compiles away. Dynamic collectors should be behind `Arc<dyn MetricsCollector>`.
//!
//! ## Key naming
//!
//! Prefer `'static` metric names to avoid per-call allocation. Backend
//! crates define their own key enums (`NetMetricKey`, `StorageMetricKey`).

use std::fmt;

/// The kind of a metric sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    /// A monotonically increasing counter.
    Counter,
    /// A value that can go up or down.
    Gauge,
    /// A distribution of observations.
    Histogram,
}

/// A named metric key. Implementors provide their own key type so they
/// can carry labels without forcing a dependency on a particular
/// labels crate.
pub trait MetricKey: fmt::Debug + Send + Sync + 'static {
    /// The stable, human-readable name of the metric.
    fn name(&self) -> &str;
}

/// Receives metric samples from pipeline stages.
///
/// Implementations of [`record`][Self::record] must be internally
/// synchronized if shared across threads (`Sync` alone is not enough
/// unless the body is lock-free or locked).
pub trait MetricsCollector: Send + Sync {
    /// Record a sample. For counters, prefer integer values that fit
    /// exactly in `f64` (≤ 2^53) or use [`record_counter`][Self::record_counter].
    fn record(&self, key: &dyn MetricKey, kind: MetricKind, value: f64);

    /// Record an integer counter increment (default converts to `f64`).
    fn record_counter(&self, key: &dyn MetricKey, value: u64) {
        self.record(key, MetricKind::Counter, value as f64);
    }

    /// Record a gauge sample.
    fn record_gauge(&self, key: &dyn MetricKey, value: f64) {
        self.record(key, MetricKind::Gauge, value);
    }

    /// Record a histogram observation.
    fn record_histogram(&self, key: &dyn MetricKey, value: f64) {
        self.record(key, MetricKind::Histogram, value);
    }
}

/// A no-op collector for benchmarks and tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullCollector;

impl MetricsCollector for NullCollector {
    fn record(&self, _key: &dyn MetricKey, _kind: MetricKind, _value: f64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_collector_smoke() {
        let c = NullCollector;
        c.record_counter(&TestKey, 1);
        c.record_gauge(&TestKey, 1.0);
        c.record_histogram(&TestKey, 0.5);
    }

    #[derive(Debug)]
    struct TestKey;
    impl MetricKey for TestKey {
        fn name(&self) -> &str {
            "test"
        }
    }
}
