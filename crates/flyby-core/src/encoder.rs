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
//! For any message `m` and a properly-sized buffer:
//!
//! ```text
//! decode(encode(m)) == Ok(Some(m))
//! ```
//!
//! This round-trip property is the recommended test for both impls.

use crate::Result;

/// Serialises a typed message into a raw byte buffer.
///
/// Implementations must be deterministic: the same message must always
/// produce the same byte sequence.
pub trait Encode {
    /// The exact number of bytes that [`encode_into`][Self::encode_into]
    /// will write.
    ///
    /// Callers use this to allocate or bounds-check the destination
    /// before calling `encode_into`. The value must be stable for the
    /// lifetime of `self`.
    fn encoded_len(&self) -> usize;

    /// Serialise `self` into `dst`.
    ///
    /// `dst` must be at least [`encoded_len`][Self::encoded_len] bytes.
    /// Writes exactly `encoded_len()` bytes and returns that count.
    ///
    /// Returns an error only if `dst` is too small or the message is in
    /// an unrepresentable state.
    fn encode_into(&self, dst: &mut [u8]) -> Result<usize>;
}
