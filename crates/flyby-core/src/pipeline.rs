//! The [`Pipeline`] trait: driving a composed set of stages.
//!
//! A pipeline owns (or is given) a source, decoder, optional
//! preprocessors, placement, and sinks. Concrete pipelines are normally
//! built via the `flyby` facade builder rather than implemented by hand.
//!
//! ## Wiring
//!
//! Stage registration may be build-time only. [`register_sink`][Pipeline::register_sink]
//! is the late-attach hook for plugin sinks; sources, decoders, and
//! placement are typically fixed at construction.
//!
//! This trait is not object-safe (associated type + methods taking
//! `Self::Message`); use generics.
//!
//! ## Event loop
//!
//! Prefer driving the pipeline with [`step`][Pipeline::step] (or
//! [`step_outcome`][Pipeline::step_outcome]) from the application or
//! builder. [`crate::Lifecycle::run`] is optional convenience.

use crate::{Lifecycle, Result, Sink, SinkId};

/// Outcome of one pipeline step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// At least one message progressed through the pipeline.
    Progress,
    /// Source had no data (idle); safe to park briefly.
    Idle,
    /// A sink returned back-pressure; caller should slow down.
    BackPressured,
    /// Source is exhausted (finite sources such as files).
    Exhausted,
}

/// A composed, runnable pipeline.
///
/// Implementations own their stages and drive them through the
/// [`Lifecycle`] phases. The pipeline is responsible for back-pressure
/// between stages and for surfacing per-stage metrics.
pub trait Pipeline: Lifecycle {
    /// The message type flowing through the pipeline.
    type Message: crate::Message;

    /// Drive one step of the pipeline.
    ///
    /// A step is the smallest unit of progress: pull from the source,
    /// decode, preprocess, route, and write to the chosen sink.
    ///
    /// Returning `Ok(true)` means progress was made; `Ok(false)` means
    /// the pipeline is idle, exhausted, or back-pressured. Use
    /// [`step_outcome`][Self::step_outcome] for a structured result.
    fn step(&mut self) -> Result<bool>;

    /// Drive one step and return a structured outcome.
    ///
    /// Default maps the boolean from [`step`][Self::step] to
    /// [`StepOutcome::Progress`] / [`StepOutcome::Idle`]. Implementations
    /// should override when they can distinguish exhausted / back-pressure.
    fn step_outcome(&mut self) -> Result<StepOutcome> {
        if self.step()? {
            Ok(StepOutcome::Progress)
        } else {
            Ok(StepOutcome::Idle)
        }
    }

    /// Register a sink under a given [`SinkId`].
    ///
    /// Must be called before [`Lifecycle::init`]. Returns an error if the
    /// id is already taken or reserved ([`SinkId::NONE`]).
    fn register_sink(
        &mut self,
        id: SinkId,
        sink: Box<dyn Sink<Message = Self::Message>>,
    ) -> Result<()>;
}
