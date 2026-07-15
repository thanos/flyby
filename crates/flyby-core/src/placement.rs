//! The [`Placement`] trait: routing decisions.
//!
//! Placement decides *where* a message goes after preprocessing. The
//! simplest placement is a fixed mapping from schema id to sink; more
//! sophisticated placements consider load, locality, and affinity.
//!
//! Placement is separated from the sink so that routing logic can be
//! tested in isolation, without a live backend.
//!
//! ## Fan-out
//!
//! The current API returns a single [`SinkId`] (1:1 routing). Multi-sink
//! fan-out is a planned extension; compose multiple pipelines or sinks
//! externally until then.

use crate::{Error, Message, Result};

/// An identifier for a sink within a pipeline.
///
/// Backed by a `u32` so that routing tables can stay compact. The value
/// `0` is reserved as "no sink / drop" ([`SinkId::NONE`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SinkId(u32);

impl SinkId {
    /// The reserved "no sink" identifier (drop the message).
    pub const NONE: SinkId = SinkId(0);

    /// Construct a non-zero sink identifier.
    ///
    /// Returns an error if `id` is zero; use [`SinkId::NONE`] for drops.
    pub fn try_new(id: u32) -> Result<Self> {
        if id == 0 {
            return Err(Error::config("SinkId 0 is reserved; use SinkId::NONE"));
        }
        Ok(SinkId(id))
    }

    /// Construct a sink identifier.
    ///
    /// # Panics
    ///
    /// Panics if `id` is zero. Prefer [`try_new`][Self::try_new] in library code.
    pub const fn new(id: u32) -> Self {
        assert!(id != 0, "SinkId 0 is reserved; use SinkId::NONE");
        SinkId(id)
    }

    /// The raw identifier value.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// `true` if this is the reserved drop identifier.
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// A routing strategy that maps each message to a [`SinkId`].
///
/// Returning [`SinkId::NONE`] drops the message intentionally.
/// Returning `Err` indicates a routing failure (unknown schema policy, …).
pub trait Placement: Send + Sync {
    /// The message type this placement routes.
    type Message: Message;

    /// Decide which sink should receive the message.
    fn route(&mut self, message: &Self::Message) -> Result<SinkId>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_zero() {
        assert_eq!(SinkId::NONE.as_u32(), 0);
        assert!(SinkId::NONE.is_none());
    }

    #[test]
    fn try_new_rejects_zero() {
        assert!(SinkId::try_new(0).is_err());
        assert_eq!(SinkId::try_new(1).unwrap().as_u32(), 1);
    }

    #[test]
    #[should_panic]
    fn new_panics_on_zero() {
        let _ = SinkId::new(0);
    }
}
