//! SPDK storage backend.
//!
//! Enabled by the `spdk` Cargo feature.  Currently a design stub — all
//! methods return [`ErrorKind::FeatureNotEnabled`].
//!
//! ## SPDK primer
//!
//! SPDK (Storage Performance Development Kit) is an Intel-originated
//! userspace storage stack.  It bypasses the kernel block layer entirely:
//!
//! - **Userspace NVMe driver**: maps the NVMe PCIe BAR directly into the
//!   process address space; I/O is submitted by writing to MMIO registers
//!   without any syscall or kernel involvement.
//! - **DMA buffers**: packet/record buffers must be physically contiguous and
//!   pinned.  SPDK allocates them from hugepages via `spdk_dma_malloc`.
//! - **Queue pairs**: each NVMe namespace exposes one or more I/O queue
//!   pairs, each with a Submission Queue and a Completion Queue.  FlyBy
//!   dedicates one queue pair per polling thread.
//! - **Polling**: unlike io_uring there is no CQ interrupt — the driver must
//!   busy-poll the completion queue.  This is only appropriate when a CPU
//!   core can be dedicated to storage I/O.
//!
//! ## When to use SPDK vs io_uring
//!
//! | Criterion | io_uring | SPDK |
//! |---|---|---|
//! | Kernel required | ≥ 5.1 | Any (MMIO) |
//! | Interrupt-driven | Yes (or kernel poll) | No (busy poll only) |
//! | CPU dedication | Not required | Required per queue |
//! | NVMe latency | ~10–20 µs | ~2–5 µs |
//! | Deployment complexity | Low | High |
//!
//! SPDK is appropriate only after the io_uring backend is validated and a
//! measured latency requirement cannot be met by io_uring (ADR-0006).
//!
//! ## Planned read path
//!
//! ```text
//! poll_batch()
//!   → poll NVMe CQ for completed DMA transfers
//!   → copy completed DMA buffers into RawRecordBatch slots
//!   → submit new NVMe read commands for the next block range
//! ```
//!
//! ## Hugepage requirement
//!
//! SPDK needs hugepages for DMA memory.  Provision before starting:
//!
//! ```sh
//! echo 1024 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages
//! ```
//!
//! ## References
//!
//! - SPDK documentation: <https://spdk.io/doc/>
//! - NVMe specification: <https://nvmexpress.org/specifications/>

use flyby_core::{Error, ErrorKind, Lifecycle, Result};

use crate::batch::RawRecordBatch;
use crate::config::SpdkConfig;
use crate::source::StorageSource;

/// SPDK-backed storage source.
///
/// All methods return [`ErrorKind::FeatureNotEnabled`] until the real SPDK
/// binding is implemented.
pub struct SpdkSource {
    #[allow(dead_code)]
    config: SpdkConfig,
}

impl SpdkSource {
    /// Create a new SPDK source with the given config.
    pub fn new(config: SpdkConfig) -> Self {
        Self { config }
    }
}

impl Lifecycle for SpdkSource {
    fn init(&mut self) -> Result<()> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "SPDK backend: not yet implemented; enable io_uring first (ADR-0006)",
        ))
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

impl StorageSource for SpdkSource {
    fn poll_batch(&mut self, _batch: &mut RawRecordBatch) -> Result<usize> {
        Err(Error::new(ErrorKind::FeatureNotEnabled, "SPDK backend: not yet implemented"))
    }

    fn backend_name() -> &'static str {
        "spdk"
    }

    fn is_exhausted(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::RawRecordBatch;

    #[test]
    fn init_returns_feature_not_enabled() {
        let mut src = SpdkSource::new(SpdkConfig::default());
        let err = src.init().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FeatureNotEnabled);
    }

    #[test]
    fn poll_returns_feature_not_enabled() {
        let mut src = SpdkSource::new(SpdkConfig::default());
        let mut batch = RawRecordBatch::new(4, 64);
        let err = src.poll_batch(&mut batch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FeatureNotEnabled);
    }

    #[test]
    fn backend_name_is_spdk() {
        assert_eq!(SpdkSource::backend_name(), "spdk");
    }
}
