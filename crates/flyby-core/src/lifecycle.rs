//! Lifecycle phases shared by every stage.
//!
//! Every source, sink, preprocessor, and placement implementation moves
//! through the same three phases:
//!
//! 1. [`Lifecycle::init`]  - acquire resources, validate config.
//! 2. [`Lifecycle::run`]   - process work.
//! 3. [`Lifecycle::shutdown`] - release resources deterministically.
//!
//! Stages are expected to be idempotent across `shutdown`: calling it
//! twice must not panic and must not double-free.

use crate::Result;

/// Lifecycle hooks shared by all stages.
///
/// This trait is intentionally separate from [`crate::Source`] /
/// [`crate::Sink`] so that stages which are neither (e.g. a pure
/// preprocessor) can still participate in startup and teardown.
pub trait Lifecycle: Send + Sync {
    /// Acquire resources and validate configuration.
    ///
    /// Called once before the pipeline enters its run loop. Must be cheap
    /// to call again after a successful `shutdown` so that a stage can be
    /// reused across runs.
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Process work.
    ///
    /// For sources and sinks this is the steady-state loop. For
    /// preprocessors and placement strategies it is typically a no-op;
    /// they are driven by the pipeline instead.
    fn run(&mut self) -> Result<()> {
        Ok(())
    }

    /// Release resources deterministically.
    ///
    /// Must be safe to call multiple times. After `shutdown` returns,
    /// the stage must not be used again until `init` succeeds once more.
    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}
