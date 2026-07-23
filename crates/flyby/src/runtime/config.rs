//! Runtime configuration (builder + TOML parity).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::backpressure::BackpressureStrategy;
use crate::api::{Error, Result};

/// Which scheduler drives the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerKind {
    /// Single-thread cooperative loop on the calling thread (default).
    #[default]
    Default,
    /// Explicit single-thread (alias of [`Default`][Self::Default]).
    SingleThread,
    /// Worker pool (one pipeline instance per worker via factory).
    WorkerPool,
}

/// Runtime execution configuration.
///
/// Builders and TOML files must remain equivalent:
///
/// ```toml
/// [runtime]
/// workers = 4
/// batch_size = 512
/// backpressure = "block"
/// scheduler = "default"
/// metrics = true
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Worker count for [`SchedulerKind::WorkerPool`] (ignored by single-thread).
    #[serde(default = "default_workers")]
    pub workers: usize,
    /// Preferred batch size hint for adapters / sources.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Sink back-pressure strategy.
    #[serde(default)]
    pub backpressure: BackpressureStrategy,
    /// Scheduler selection.
    #[serde(default)]
    pub scheduler: SchedulerKind,
    /// Emit runtime metrics when a collector is attached.
    #[serde(default = "default_true")]
    pub metrics: bool,
    /// Max spin / block retries on back-pressure before giving up a step
    /// (`None` = unlimited for block/spin until progress or shutdown).
    #[serde(default)]
    pub backpressure_retries: Option<u32>,
    /// Yield between block retries (milliseconds). `0` = spin.
    #[serde(default = "default_yield_ms")]
    pub backpressure_yield_ms: u64,
    /// Cooperative poll idle pause when the source is idle (milliseconds).
    #[serde(default)]
    pub idle_sleep_ms: Option<u64>,
    /// Overflow sink id used when `backpressure = "overflow"` (`None` = drop).
    #[serde(default)]
    pub overflow_sink: Option<u32>,
}

fn default_workers() -> usize {
    1
}
fn default_batch_size() -> usize {
    512
}
fn default_true() -> bool {
    true
}
fn default_yield_ms() -> u64 {
    0
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            workers: default_workers(),
            batch_size: default_batch_size(),
            backpressure: BackpressureStrategy::default(),
            scheduler: SchedulerKind::default(),
            metrics: true,
            backpressure_retries: None,
            backpressure_yield_ms: 0,
            idle_sleep_ms: None,
            overflow_sink: None,
        }
    }
}

/// Wrapper matching the `[runtime]` TOML table.
#[derive(Debug, Clone, Default, Deserialize)]
struct RuntimeFile {
    #[serde(default)]
    runtime: RuntimeConfig,
}

impl RuntimeConfig {
    /// Parse a full document containing a `[runtime]` table.
    pub fn from_toml_str(toml_src: &str) -> Result<Self> {
        let file: RuntimeFile =
            toml::from_str(toml_src).map_err(|e| Error::config(format!("runtime TOML: {e}")))?;
        file.runtime.validate()?;
        Ok(file.runtime)
    }

    /// Load from a TOML file path.
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::config(format!(
                "failed to read runtime config '{}': {e}",
                path.display()
            ))
        })?;
        Self::from_toml_str(&text)
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<()> {
        if self.workers == 0 {
            return Err(Error::config("runtime.workers must be ≥ 1"));
        }
        if self.batch_size == 0 {
            return Err(Error::config("runtime.batch_size must be ≥ 1"));
        }
        if matches!(self.scheduler, SchedulerKind::WorkerPool) && self.workers < 1 {
            return Err(Error::config("worker pool requires workers ≥ 1"));
        }
        Ok(())
    }

    /// Idle sleep duration when configured.
    pub fn idle_sleep(&self) -> Option<Duration> {
        self.idle_sleep_ms.map(Duration::from_millis)
    }

    /// Yield duration between back-pressure block retries.
    pub fn backpressure_yield(&self) -> Duration {
        Duration::from_millis(self.backpressure_yield_ms)
    }

    /// Builder-style setters.
    pub fn with_workers(mut self, n: usize) -> Self {
        self.workers = n.max(1);
        self
    }

    /// Set batch size hint.
    pub fn with_batch_size(mut self, n: usize) -> Self {
        self.batch_size = n.max(1);
        self
    }

    /// Set back-pressure strategy.
    pub fn with_backpressure(mut self, bp: BackpressureStrategy) -> Self {
        self.backpressure = bp;
        self
    }

    /// Set scheduler kind.
    pub fn with_scheduler(mut self, kind: SchedulerKind) -> Self {
        self.scheduler = kind;
        self
    }

    /// Enable or disable metric emission.
    pub fn with_metrics(mut self, on: bool) -> Self {
        self.metrics = on;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_toml_example() {
        let cfg = RuntimeConfig::from_toml_str(
            r#"
            [runtime]
            workers = 4
            batch_size = 512
            backpressure = "block"
            scheduler = "default"
            metrics = true
            "#,
        )
        .unwrap();
        assert_eq!(cfg.workers, 4);
        assert_eq!(cfg.batch_size, 512);
        assert_eq!(cfg.backpressure, BackpressureStrategy::Block);
        assert!(cfg.metrics);
    }

    #[test]
    fn rejects_zero_batch() {
        let cfg = RuntimeConfig {
            batch_size: 0,
            ..RuntimeConfig::default()
        };
        assert!(cfg.validate().is_err());
    }
}
