//! The [`Placement`] trait: routing decisions.
//!
//! Placement decides *where* a message goes after preprocessing. The
//! simplest placement is a fixed mapping from schema id to sink; more
//! sophisticated placements consider load, locality, and affinity.
//!
//! Placement is separated from the sink so that routing logic can be
//! tested in isolation, without a live backend.

use crate::{Message, Result};

/// An identifier for a sink within a pipeline.
///
/// Backed by a `u32` so that routing tables can stay compact. The value
/// `0` is reserved as "no sink / drop".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SinkId(pub u32);

impl SinkId {
    /// The reserved "no sink" identifier.
    pub const NONE: SinkId = SinkId(0);

    /// Construct a sink identifier. `0` is reserved; use a non-zero value.
    pub const fn new(id: u32) -> Self {
        SinkId(id)
    }

    /// The raw identifier value.
    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

/// A routing strategy that maps each message to a [`SinkId`].
///
/// Returning [`SinkId::NONE`] drops the message.
pub trait Placement: Send + Sync {
    /// The message type this placement routes.
    type Message: Message;

    /// Decide which sink should receive the message.
    fn route(&mut self, message: &Self::Message) -> Result<SinkId>;
}
