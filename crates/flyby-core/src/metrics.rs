//! The [`MetricsCollector`] trait: observability.
//!
//! Every stage reports counters, gauges, and histograms through a
//! metrics collector. The trait is intentionally minimal so that
//! backends can plug in `prometheus`, `metrics`, or a no-op collector
//! for benchmarks without coupling to a specific crate.

use core::fmt;

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
/// Implementations must be cheap to call from hot paths: the default
/// [`NullCollector`] does no work and compiles away.
pub trait MetricsCollector: Send + Sync {
    /// Record a sample.
    fn record(&self, key: &dyn MetricKey, kind: MetricKind, value: f64);
}

/// A no-op collector for benchmarks and tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullCollector;

impl MetricsCollector for NullCollector {
    fn record(&self, _key: &dyn MetricKey, _kind: MetricKind, _value: f64) {}
}
