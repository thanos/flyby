//! [`NetworkSource`]: the batch-poll trait for network backends.
//!
//! Every network backend (AF_XDP, DPDK, simulator, pcap replay) implements
//! this trait. The pipeline drives it by calling [`poll_batch`] in a loop
//! and forwarding each [`RawBatch`] to the decoder stage.
//!
//! ## Relationship to [`flyby_core::Source`]
//!
//! [`flyby_core::Source`] returns one byte slice per poll. `NetworkSource`
//! extends the model with a batch API, which is essential for network
//! ingest where polling in bursts amortises system-call overhead.
//!
//! A future pipeline revision will drive `NetworkSource` directly.
//! For now, backends also implement `flyby_core::Source` as a compatibility
//! shim (returning the first packet of each batch).
//!
//! [`poll_batch`]: NetworkSource::poll_batch

use flyby_core::{Lifecycle, Result};

use crate::batch::RawBatch;

/// What the source should do when the downstream pipeline cannot keep up.
///
/// The default policy for new backends is [`DropNewest`][Self::DropNewest].
/// Production deployments must choose a policy explicitly and configure
/// their metrics to surface drop events — FlyBy never silently drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackpressurePolicy {
    /// Discard the incoming packet when the pipeline is full.
    /// Keeps the ring flowing; latency of older packets is unaffected.
    #[default]
    DropNewest,
    /// Evict the oldest unread packet to make room for the new one.
    /// Preserves recency at the cost of losing older data.
    DropOldest,
    /// Spin until the pipeline can accept the packet.
    /// Zero packet loss; may stall the source under sustained load.
    Block,
    /// Forward overflow packets to a configured overflow sink.
    /// Requires an overflow sink to be registered with the pipeline.
    Overflow,
}

/// A network packet source that produces raw bytes in batches.
///
/// All network backends — simulated, AF_XDP, DPDK, pcap replay — implement
/// this trait. The pipeline calls [`poll_batch`][Self::poll_batch] in a
/// tight loop. Back-pressure is reported via [`backpressure_policy`][Self::backpressure_policy]
/// and tracked via [`RawBatch::record_drop`].
pub trait NetworkSource: Lifecycle {
    /// Poll up to `batch.capacity()` packets into `batch`.
    ///
    /// The implementation must call [`batch.reset`][RawBatch::reset] before
    /// filling new packets. Returns the number of packets received (which
    /// equals `batch.len()` after the call).
    ///
    /// `Ok(0)` means no packets were available (the source is idle).
    /// `Err(_)` means a non-recoverable backend failure.
    fn poll_batch(&mut self, batch: &mut RawBatch) -> Result<usize>;

    /// The backpressure policy this source applies when the pipeline is full.
    fn backpressure_policy(&self) -> BackpressurePolicy {
        BackpressurePolicy::DropNewest
    }

    /// Human-readable name of this backend, used in logs and metrics.
    fn backend_name(&self) -> &'static str;
}
