//! The [`Decoder`] trait: converting raw bytes into typed messages.
//!
//! A decoder sits between a [`crate::Source`] and the rest of the pipeline.
//! It is the only place in a pipeline where supplier-specific wire-format
//! knowledge lives. Everything downstream operates on the decoded
//! `Output` type and never inspects raw bytes again.
//!
//! ## Pairing with a source
//!
//! A decoder is always paired with its source at builder / configuration
//! time. The pairing is enforced by the type system: `Decoder::Output`
//! must match the message type `M` expected by every downstream stage
//! (`PreProcessor<Message = M>`, `Placement<Message = M>`,
//! `Sink<Message = M>`).
//!
//! ## Input assumption
//!
//! `decode` receives one complete framed record. Framing (splitting a
//! byte stream into records) is the source / framer's responsibility.
//!
//! ## Filtering
//!
//! Returning `Ok(None)` drops a frame (malformed, filtered, or control).
//! Returning `Err` signals a genuine decode failure.

use crate::{Message, Result};

/// Converts a raw byte slice produced by a [`crate::Source`] into a
/// typed [`Message`].
///
/// Decoders that live on a worker thread must be `Send` (required here).
pub trait Decoder: Send {
    /// The concrete message type this decoder produces.
    type Output: Message;

    /// Attempt to decode a raw byte slice into a typed message.
    ///
    /// - `Ok(Some(msg))` — a message was successfully decoded.
    /// - `Ok(None)` — the frame should be dropped (malformed, filtered, or
    ///   a protocol control frame the pipeline ignores).
    /// - `Err(e)` — a genuine decode failure.
    fn decode(&mut self, raw: &[u8]) -> Result<Option<Self::Output>>;
}
