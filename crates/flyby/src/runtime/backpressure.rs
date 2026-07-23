//! Explicit sink back-pressure strategies (Part VII §8).

use serde::{Deserialize, Serialize};

/// How the runtime reacts when a sink returns [`ErrorKind::BackPressure`][crate::api::ErrorKind::BackPressure].
///
/// The chosen strategy must be explicit in [`RuntimeConfig`][super::RuntimeConfig]
/// and is observable via [`RuntimeMetricKey`][super::RuntimeMetricKey].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressureStrategy {
    /// Retry the write (spin or yield) until success, retry budget, or shutdown.
    #[default]
    Block,
    /// Busy-loop retries without sleeping (same as block with zero yield).
    Spin,
    /// Discard the current message (keep older buffered work).
    DropNewest,
    /// Discard the current message; for single-message retry this matches
    /// drop-newest. Ring-level eviction is a sink concern.
    DropOldest,
    /// Forward to the configured overflow sink when present; otherwise drop.
    Overflow,
    /// Shrink effective work and retry once (observability hook; same as block
    /// with a single retry today).
    AdaptiveBatching,
}

impl BackpressureStrategy {
    /// Human-readable name for metrics / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Spin => "spin",
            Self::DropNewest => "drop_newest",
            Self::DropOldest => "drop_oldest",
            Self::Overflow => "overflow",
            Self::AdaptiveBatching => "adaptive_batching",
        }
    }
}
