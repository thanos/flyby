//! The [`Source`] trait: producers of raw bytes or records.
//!
//! A source sits at the head of a FlyBy pipeline. It is responsible for
//! acquiring raw data (from a socket, a file, shared memory, a
//! simulator, ...) and handing it to the pipeline as opaque byte slices
//! or already-decoded messages.
//!
//! Sources are expected to be back-pressure aware: when the pipeline
//! cannot accept more work, the source should slow down rather than
//! drop data, unless explicitly configured otherwise.

use crate::{Lifecycle, Result};

/// A producer of raw bytes or pre-decoded records.
///
/// The trait is intentionally synchronous in this skeleton. An async
/// variant (`async fn`) is planned but will be introduced behind an ADR
/// so that backends which are fundamentally synchronous (e.g. a
/// simulator reading from an in-memory ring) are not forced onto a
/// runtime.
pub trait Source: Lifecycle {
    /// Attempt to pull the next raw batch from the source.
    ///
    /// Returns `Ok(Some(batch))` when data is available, `Ok(None)` when
    /// the source is temporarily exhausted (e.g. a non-blocking socket
    /// with no data ready), and `Err` on a genuine failure.
    ///
    /// `batch` is an opaque, borrowed view; the pipeline owns decoding.
    fn poll(&mut self) -> Result<Option<&[u8]>>;
}
