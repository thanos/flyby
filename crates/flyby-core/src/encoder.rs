//! The [`Encode`] trait: serialising typed messages into raw bytes.
//!
//! `Encode` is the sink-side counterpart to [`crate::Decoder`]. Where a
//! decoder converts raw bytes from a [`crate::Source`] into a typed
//! [`crate::Message`], an encoder converts a typed message into bytes
//! suitable for storage or transmission.
//!
//! ## Who implements this?
//!
//! The supplier's concrete message type. The framework never implements
//! `Encode` on behalf of user types; it only calls it from sinks that
//! need to persist or forward messages as bytes (e.g. the shared-memory
//! sink, a file sink).
//!
//! ## Symmetry with `Decoder`
//!
//! For a message type that also has a paired `Decoder`, and for types
//! that implement `PartialEq`:
//!
//! ```text
//! decoder.decode(encode(m)) == Ok(Some(m))
//! ```
//!
//! This round-trip property is the recommended test for both impls.
//!
//! ## Buffer sizing
//!
//! Callers must not mutate the message between `encoded_len` and
//! `encode_into`. Undersized buffers must return
//! [`crate::ErrorKind::Encode`].

use crate::Result;

/// Serialises a typed message into a raw byte buffer.
///
/// Implementations must be deterministic: the same message must always
/// produce the same byte sequence. Not every [`crate::Message`] needs
/// `Encode`; sinks that write bytes require both bounds.
pub trait Encode {
    /// The exact number of bytes that `encode_into` will write.
    ///
    /// Must be stable for the lifetime of `self` (no mutation between
    /// length query and encode).
    fn encoded_len(&self) -> usize;

    /// Serialise `self` into `dst`.
    ///
    /// `dst` must be at least `encoded_len()` bytes.
    /// Writes exactly `encoded_len()` bytes and returns that count.
    ///
    /// Returns [`crate::ErrorKind::Encode`] if `dst` is too small or the
    /// message is in an unrepresentable state.
    fn encode_into(&self, dst: &mut [u8]) -> Result<usize>;
}
