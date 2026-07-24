//! Lifecycle phases shared by stages that own resources.
//!
//! Source, sink, and pipeline implementations move through:
//!
//! 1. [`Lifecycle::init`]  — acquire resources, validate config.
//! 2. [`Lifecycle::run`]   — optional steady-state convenience loop.
//! 3. [`Lifecycle::shutdown`] — release resources deterministically.
//!
//! Not every pipeline concept implements this trait: pure
//! [`crate::PreProcessor`], [`crate::Placement`], [`crate::Decoder`],
//! [`crate::Encode`], and metrics collectors are typically driven by the
//! pipeline without their own resource lifecycle.
//!
//! ## Event-loop ownership
//!
//! The preferred driver is [`crate::Pipeline::step`]: one unit of
//! progress (pull → decode → route → write). [`Lifecycle::run`] is an
//! optional convenience that may loop `step` (or equivalent). Sources
//! expose `poll` / `poll_batch` for the pipeline to call; they should not
//! own the outer event loop when composed into a pipeline.
//!
//! ## State machine
//!
//! Illegal transitions (e.g. `run` before `init`, use after failed
//! `init`) should return [`crate::ErrorKind::Lifecycle`]. After
//! `shutdown`, the stage must not be used until `init` succeeds again.
//! `shutdown` is idempotent: calling it twice must not panic or
//! double-free. Partial failure during `init` must leave the stage safe
//! to drop or re-`init`.

use crate::Result;

/// Lifecycle hooks for stages that own resources.
pub trait Lifecycle: Send + Sync {
    /// Acquire resources and validate configuration.
    ///
    /// Called once before the pipeline enters its run loop. Must be safe
    /// to call again after a successful `shutdown` so that a stage can be
    /// reused across runs.
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Optional steady-state entry point.
    ///
    /// Prefer [`crate::Pipeline::step`] as the composition driver. Default
    /// is a no-op.
    fn run(&mut self) -> Result<()> {
        Ok(())
    }

    /// Release resources deterministically.
    ///
    /// Must be safe to call multiple times. After `shutdown` returns,
    /// the stage must not be used again until `init` succeeds once more.
    /// Implementations that buffer should flush here or document why not.
    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DefaultsOnly;

    impl Lifecycle for DefaultsOnly {}

    #[test]
    fn default_hooks_are_noops() {
        let mut stage = DefaultsOnly;
        stage.init().unwrap();
        stage.run().unwrap();
        stage.shutdown().unwrap();
        // Idempotent shutdown.
        stage.shutdown().unwrap();
    }
}
