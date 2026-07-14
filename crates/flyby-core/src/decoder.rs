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
//! ## Statefulness
//!
//! Decoders should be stateless where possible. Stateful decoders
//! (reassembly, decompression, multi-packet frames) carry their state
//! explicitly in `self` and must document their invariants.
//!
//! ## Filtering
//!
//! Returning `Ok(None)` silently drops a frame. Use this for malformed
//! packets, heartbeats, or any message the pipeline should not see.
//! Returning `Err` signals a genuine decode failure and may trigger
//! pipeline-level error handling.

use crate::{Message, Result};

/// Converts a raw byte slice produced by a [`crate::Source`] into a
/// typed [`Message`].
///
/// # Type parameter
///
/// `Output` is the concrete supplier message type. The framework never
/// inspects its internal fields; all pipeline stages downstream of the
/// decoder are generic over `Output`.
pub trait Decoder {
    /// The concrete message type this decoder produces.
    type Output: Message;

    /// Attempt to decode a raw byte slice into a typed message.
    ///
    /// - `Ok(Some(msg))` — a message was successfully decoded.
    /// - `Ok(None)` — the frame should be silently dropped (malformed,
    ///   filtered, or a protocol control frame the pipeline ignores).
    /// - `Err(e)` — a genuine decode failure; the pipeline surfaces the
    ///   error through its error-handling policy.
    fn decode(&mut self, raw: &[u8]) -> Result<Option<Self::Output>>;
}
