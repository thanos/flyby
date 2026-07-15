//! [`StorageSource`]: the storage analogue of [`flyby_net::NetworkSource`].
//!
//! Any storage backend (file, io_uring, SPDK) must implement this trait.
//! The pipeline treats all backends identically — only the configuration and
//! the feature gate differ.
//!
//! ## Relationship to [`Source`][flyby_core::Source]
//!
//! [`Source`][flyby_core::Source] is the *typed* trait: it emits decoded
//! [`Message`][flyby_core::Message] values.  `StorageSource` sits one layer
//! below: it emits raw byte slices in a [`RawRecordBatch`] that the pipeline's
//! decoder then parses into typed messages.  This separation keeps each layer
//! independently testable.

use flyby_core::{Lifecycle, Result};

use crate::batch::RawRecordBatch;

// ---------------------------------------------------------------------------
// StorageSource trait
// ---------------------------------------------------------------------------

/// A batch-pull source backed by persistent storage.
///
/// Implementations must also implement [`Lifecycle`], which provides
/// `init` and `shutdown` hooks.
pub trait StorageSource: Lifecycle {
    /// Fill `batch` with the next available records.
    ///
    /// Returns the number of records written into `batch`.  Zero is a valid
    /// return when no records are currently available (e.g. when
    /// [`ReplayMode::OriginalTiming`][crate::replay::ReplayMode::OriginalTiming]
    /// is holding back the next record, or when the source is at EOF and
    /// [`EofPolicy::Follow`][crate::config::EofPolicy::Follow] is active).
    ///
    /// `batch` must be [`reset`][RawRecordBatch::reset] by the caller before
    /// each call.
    fn poll_batch(&mut self, batch: &mut RawRecordBatch) -> Result<usize>;

    /// Human-readable backend identifier returned in metrics and logs.
    ///
    /// Examples: `"file"`, `"io_uring"`, `"spdk"`.
    fn backend_name() -> &'static str
    where
        Self: Sized;

    /// `true` when the source has no more data and will not produce further
    /// records.
    ///
    /// A source that supports [`EofPolicy::Loop`][crate::config::EofPolicy::Loop]
    /// never returns `true` here.
    fn is_exhausted(&self) -> bool;
}
