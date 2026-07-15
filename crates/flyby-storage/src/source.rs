//! [`StorageSource`]: the storage analogue of `flyby_net::NetworkSource`.
//!
//! Any storage backend (file, io_uring, SPDK) must implement this trait.
//! The pipeline treats all backends identically — only the configuration and
//! the feature gate differ.
//!
//! ## Relationship to [`Source`][flyby_core::Source]
//!
//! [`Source`][flyby_core::Source] is the core **raw-bytes** trait
//! (`poll() -> Option<&[u8]>`). `StorageSource` is the batch-oriented
//! storage surface: it fills a [`RawRecordBatch`] with framed raw records
//! that a [`Decoder`][flyby_core::Decoder] later turns into typed messages.
//! This separation keeps each layer independently testable.

use flyby_core::{Lifecycle, Result};

use crate::batch::RawRecordBatch;

/// A batch-pull source backed by persistent storage.
///
/// Implementations must also implement [`Lifecycle`], which provides
/// `init` and `shutdown` hooks.
pub trait StorageSource: Lifecycle {
    /// Fill `batch` with the next available records.
    ///
    /// Returns the number of records written into `batch`. Zero is valid
    /// when no records are currently available (e.g. at EOF with
    /// [`EofPolicy::Follow`][crate::config::EofPolicy::Follow], or when
    /// a caller-side [`ReplayEngine`][crate::replay::ReplayEngine] holds
    /// emission). This trait does not apply replay timing itself.
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
