//! The [`Source`] trait: producers of raw bytes.
//!
//! A source sits at the head of a FlyBy pipeline. It acquires raw data
//! (from a socket, a file, shared memory, a simulator, …) and hands it
//! to the pipeline as opaque byte slices. Decoding is performed by a
//! separate [`crate::Decoder`].
//!
//! ## Batch backends
//!
//! Production networking and storage backends expose **batch** poll APIs
//! (`poll_batch`) that fill a reusable batch buffer. Those traits live in
//! the backend crates (`flyby-net`, `flyby-storage`) and are the preferred
//! hot path. This trait is the scalar compatibility surface: one slice
//! per call.
//!
//! ## Semantics
//!
//! | Return | Meaning |
//! |--------|---------|
//! | `Ok(Some(bytes))` | One complete frame/record is available. `bytes` may be empty only if the protocol allows empty payloads. |
//! | `Ok(None)` | Temporarily idle (no data ready). Not EOF. |
//! | `Err` | Failure (I/O, lifecycle, …). |
//!
//! Finite sources (e.g. files) should expose exhaustion via backend-specific
//! APIs (`is_exhausted`) until a shared `PollOutcome` lands in core.
//!
//! Sources should be back-pressure aware: when the pipeline cannot accept
//! more work, slow down rather than drop, unless configured otherwise.

use crate::{Lifecycle, Result};

/// A producer of raw bytes for the pipeline.
///
/// The trait is intentionally synchronous in this skeleton. An async
/// variant is planned behind an ADR so synchronous backends are not forced
/// onto a runtime.
pub trait Source: Lifecycle {
    /// Attempt to pull the next raw frame from the source.
    ///
    /// The returned slice is borrowed from the source and is only valid
    /// until the next mutable use of `self`. Callers that need to retain
    /// bytes across another poll must copy them.
    ///
    /// Prefer batch APIs on concrete backends when available.
    fn poll(&mut self) -> Result<Option<&[u8]>>;
}
