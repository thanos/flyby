//! io_uring storage backend.
//!
//! Enabled by the `io_uring` Cargo feature.  Currently a design stub — the
//! interface compiles and returns [`ErrorKind::FeatureNotEnabled`] until the
//! real binding is implemented.
//!
//! ## io_uring primer
//!
//! io_uring (Linux 5.1+) is a kernel async I/O interface built around two
//! lock-free ring buffers shared between userspace and the kernel:
//!
//! - **Submission Queue (SQ)**: userspace writes I/O requests (SQEs) here.
//! - **Completion Queue (CQ)**: the kernel writes results (CQEs) here.
//!
//! The key advantage over `epoll`/`libaio` is that both submission and
//! completion can be batched with a single `io_uring_enter` syscall (or
//! zero syscalls in kernel-poll mode), dramatically reducing context-switch
//! overhead at high IOPS.
//!
//! ## Planned read path
//!
//! ```text
//! poll_batch()
//!   → peek CQ for completed reads → copy payload into RawRecordBatch slots
//!   → refill SQ with new read SQEs (registered buffers if enabled)
//!   → io_uring_enter(submit, min_complete=0)  // non-blocking
//! ```
//!
//! ## O_DIRECT
//!
//! When `IoUringConfig::o_direct` is `true`, the file is opened with
//! `O_DIRECT`.  Buffers must be 512-byte aligned (or the logical block size,
//! whichever is larger) and the read length must be a multiple of that
//! alignment.  The framer is responsible for understanding the padding.
//!
//! O_DIRECT bypasses the page cache, which eliminates double-buffering for
//! large sequential reads but adds alignment constraints.  Benchmark both
//! paths before choosing for a given workload.
//!
//! ## Registered buffers
//!
//! `io_uring_register(IORING_REGISTER_BUFFERS, ...)` tells the kernel the
//! physical addresses of the read buffers once, so each subsequent read does
//! not need to pin/unpin them.  This is most beneficial at queue depths > 32.
//!
//! ## References
//!
//! - Jens Axboe's original io_uring paper:
//!   <https://kernel.dk/io_uring.pdf>
//! - `liburing` C library (wraps the syscall interface):
//!   <https://github.com/axboe/liburing>
//! - Lord of the io_uring tutorial:
//!   <https://unixism.net/loti/>

use flyby_core::{Error, ErrorKind, Lifecycle, Result};

use crate::batch::RawRecordBatch;
use crate::config::{FileConfig, IoUringConfig};
use crate::source::StorageSource;

/// io_uring-backed storage source.
///
/// All methods currently return [`ErrorKind::FeatureNotEnabled`] because the
/// real io_uring binding has not yet been implemented.  The struct compiles
/// cleanly behind the `io_uring` feature flag so the workspace integration
/// tests can confirm the API shape.
pub struct IoUringSource {
    #[allow(dead_code)]
    file_config: FileConfig,
    #[allow(dead_code)]
    io_config: IoUringConfig,
}

impl IoUringSource {
    /// Create a new io_uring source with the given configs.
    pub fn new(file_config: FileConfig, io_config: IoUringConfig) -> Self {
        Self {
            file_config,
            io_config,
        }
    }
}

impl Lifecycle for IoUringSource {
    fn init(&mut self) -> Result<()> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "io_uring backend: feature 'io_uring' is enabled but the backend is not yet \
             implemented; use FileSource for now",
        ))
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

impl StorageSource for IoUringSource {
    fn poll_batch(&mut self, _batch: &mut RawRecordBatch) -> Result<usize> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "io_uring backend: not yet implemented",
        ))
    }

    fn backend_name() -> &'static str {
        "io_uring"
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
        let mut src = IoUringSource::new(FileConfig::default(), IoUringConfig::default());
        let err = src.init().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FeatureNotEnabled);
    }

    #[test]
    fn poll_returns_feature_not_enabled() {
        let mut src = IoUringSource::new(FileConfig::default(), IoUringConfig::default());
        let mut batch = RawRecordBatch::new(4, 64);
        let err = src.poll_batch(&mut batch).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FeatureNotEnabled);
    }

    #[test]
    fn backend_name_is_io_uring() {
        assert_eq!(IoUringSource::backend_name(), "io_uring");
    }
}
