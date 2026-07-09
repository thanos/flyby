//! The [`Pipeline`] trait: wiring stages together.
//!
//! A pipeline is the composition of a [`crate::Source`], zero or more
//! [`crate::PreProcessor`] steps, a [`crate::Placement`] strategy, and one
//! or more [`crate::Sink`]s. The trait below is the contract that the
//! public facade (`flyby::FlyBy`) drives; concrete pipelines are
//! expected to be built via the builder API rather than implemented by
//! hand.

use crate::{Lifecycle, Result, Sink, SinkId};

/// A composed, runnable pipeline.
///
/// Implementations own their stages and drive them through the
/// [`Lifecycle`] phases. The pipeline is responsible for back-pressure
/// between stages and for surfacing per-stage metrics to the
/// [`crate::MetricsCollector`].
pub trait Pipeline: Lifecycle {
    /// The message type flowing through the pipeline.
    type Message: crate::Message;

    /// Drive one step of the pipeline.
    ///
    /// A step is the smallest unit of progress: pull from the source,
    /// decode, preprocess, route, and write to the chosen sink. Returning
    /// `Ok(true)` means progress was made; `Ok(false)` means the pipeline
    /// is idle and may be parked.
    fn step(&mut self) -> Result<bool>;

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
