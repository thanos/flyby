//! Runtime telemetry keys and lifecycle event hooks.

use crate::api::{MetricKey, MetricsCollector};

/// Metric keys emitted by the runtime / pipeline driver.
#[derive(Debug, Clone, Copy)]
pub enum RuntimeMetricKey {
    /// Pipeline steps completed.
    Steps,
    /// Messages successfully written to sinks.
    MessagesOut,
    /// Messages dropped by back-pressure policy.
    MessagesDropped,
    /// Back-pressure events observed.
    BackpressureEvents,
    /// Decode / preprocess idle skips.
    IdleSkips,
    /// Wall-time nanoseconds for one step (histogram).
    StepDurationNs,
    /// Current runtime phase as a gauge (ordinal).
    Phase,
    /// Scheduler worker count.
    Workers,
}

impl MetricKey for RuntimeMetricKey {
    fn name(&self) -> &str {
        match self {
            Self::Steps => "runtime.steps",
            Self::MessagesOut => "runtime.messages_out",
            Self::MessagesDropped => "runtime.messages_dropped",
            Self::BackpressureEvents => "runtime.backpressure_events",
            Self::IdleSkips => "runtime.idle_skips",
            Self::StepDurationNs => "runtime.step_duration_ns",
            Self::Phase => "runtime.phase",
            Self::Workers => "runtime.workers",
        }
    }
}

/// Lifecycle / scheduling events for logs and future OpenTelemetry spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    /// Runtime constructed.
    Built,
    /// Configuration validated.
    Validated,
    /// Pipeline and sinks initialized.
    Initialized,
    /// Sources started / run loop entered.
    Started,
    /// Draining remaining pending work after source exhaustion.
    Draining,
    /// Shutdown requested or completed.
    Shutdown,
    /// Resources cleaned up.
    Cleanup,
    /// Scheduler tick / step completed.
    Step {
        /// Step counter.
        n: u64,
    },
}

/// Emit a lifecycle counter when metrics are enabled.
pub fn emit_lifecycle(metrics: &dyn MetricsCollector, enabled: bool, event: &RuntimeEvent) {
    if !enabled {
        return;
    }
    let key = match event {
        RuntimeEvent::Built => "runtime.event.built",
        RuntimeEvent::Validated => "runtime.event.validated",
        RuntimeEvent::Initialized => "runtime.event.initialized",
        RuntimeEvent::Started => "runtime.event.started",
        RuntimeEvent::Draining => "runtime.event.draining",
        RuntimeEvent::Shutdown => "runtime.event.shutdown",
        RuntimeEvent::Cleanup => "runtime.event.cleanup",
        RuntimeEvent::Step { .. } => "runtime.event.step",
    };
    metrics.record_counter(&StrKey(key), 1);
}

#[derive(Debug)]
struct StrKey(&'static str);
impl MetricKey for StrKey {
    fn name(&self) -> &str {
        self.0
    }
}
