//! Configuration types for the storage subsystem.
//!
//! ## TOML example
//!
//! ```toml
//! [source]
//! kind = "file"
//! path = "ticks.bin"
//! batch_size = 1024
//!
//! [source.replay]
//! mode = "original"
//!
//! [source.io_uring]
//! enabled = true
//! queue_depth = 256
//! registered_buffers = true
//! ```

use std::path::PathBuf;
use std::time::Duration;

use crate::replay::ReplayMode;

// ---------------------------------------------------------------------------
// File backend
// ---------------------------------------------------------------------------

/// Configuration for the [`FileSource`][crate::file::FileSource].
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// Path to the input file.
    pub path: PathBuf,

    /// Maximum number of records per batch.
    ///
    /// Default: 256.
    pub batch_size: usize,

    /// Maximum byte length of a single record, including any framing header.
    ///
    /// The read buffer is sized to `batch_size × max_record_size`.
    ///
    /// Default: 4096.
    pub max_record_size: usize,

    /// Replay mode.  Default: [`ReplayMode::FullSpeed`].
    pub replay: ReplayMode,

    /// What to do when EOF is reached.
    pub eof_policy: EofPolicy,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("input.bin"),
            batch_size: 256,
            max_record_size: 4096,
            replay: ReplayMode::FullSpeed,
            eof_policy: EofPolicy::Stop,
        }
    }
}

/// Behaviour when the [`FileSource`][crate::file::FileSource] reaches EOF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EofPolicy {
    /// Stop and signal the pipeline to shut down.
    Stop,
    /// Seek back to the start and replay from the beginning.
    Loop,
    /// Wait for more data to appear (tail-follow, like `tail -f`).
    ///
    /// The source polls again after `poll_interval`.
    Follow {
        /// How long to wait between read attempts when at EOF.
        poll_interval: Duration,
    },
}

// ---------------------------------------------------------------------------
// io_uring backend
// ---------------------------------------------------------------------------

/// Configuration for the io_uring storage backend.
///
/// Only relevant when the `io_uring` feature is enabled.  The file backend
/// falls back to buffered reads when this feature is absent.
#[derive(Debug, Clone)]
pub struct IoUringConfig {
    /// Number of SQ/CQ ring entries (must be a power of two).
    ///
    /// Higher values allow more in-flight read operations and can improve
    /// throughput on NVMe devices.  Default: 256.
    pub queue_depth: u32,

    /// Register read buffers with the kernel to reduce `copy_to_user` cost.
    ///
    /// Requires an additional `io_uring_register` call at startup.
    /// Default: `false` (disabled until the benefit is measured).
    pub registered_buffers: bool,

    /// Use `O_DIRECT` for all read operations.
    ///
    /// Bypasses the page cache.  Requires 512-byte-aligned buffers and a
    /// filesystem that supports `O_DIRECT` (most production Linux filesystems
    /// do).  Default: `false`.
    pub o_direct: bool,

    /// Number of in-flight read operations to keep queued at all times.
    ///
    /// Higher values increase parallelism but consume more memory.
    /// Default: 4.
    pub inflight: u32,
}

impl Default for IoUringConfig {
    fn default() -> Self {
        Self { queue_depth: 256, registered_buffers: false, o_direct: false, inflight: 4 }
    }
}

// ---------------------------------------------------------------------------
// SPDK backend
// ---------------------------------------------------------------------------

/// Configuration for the SPDK storage backend.
///
/// Only relevant when the `spdk` feature is enabled.  SPDK is deferred until
/// the replay engine and io_uring backend are stable (see ADR-0006).
#[derive(Debug, Clone)]
pub struct SpdkConfig {
    /// PCI address of the NVMe device (e.g. `"0000:00:1f.2"`).
    pub pci_addr: String,

    /// NVMe namespace ID.  Usually 1.
    pub namespace_id: u32,

    /// Number of I/O queue pairs to allocate per device.
    ///
    /// Each queue pair has its own submission and completion queue.
    /// Default: 1.
    pub queue_pairs: u32,

    /// Hugepage pool size in mebibytes.
    ///
    /// SPDK uses hugepages for DMA buffers.  Must be ≥ 512 MiB for
    /// most workloads.  Default: 1024 MiB.
    pub hugepage_mb: usize,
}

impl Default for SpdkConfig {
    fn default() -> Self {
        Self {
            pci_addr: "0000:00:1f.2".to_string(),
            namespace_id: 1,
            queue_pairs: 1,
            hugepage_mb: 1024,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_config_defaults() {
        let cfg = FileConfig::default();
        assert_eq!(cfg.batch_size, 256);
        assert_eq!(cfg.max_record_size, 4096);
        assert_eq!(cfg.replay, ReplayMode::FullSpeed);
        assert_eq!(cfg.eof_policy, EofPolicy::Stop);
    }

    #[test]
    fn io_uring_config_defaults() {
        let cfg = IoUringConfig::default();
        assert_eq!(cfg.queue_depth, 256);
        assert!(!cfg.registered_buffers);
        assert!(!cfg.o_direct);
    }

    #[test]
    fn spdk_config_defaults() {
        let cfg = SpdkConfig::default();
        assert_eq!(cfg.namespace_id, 1);
        assert_eq!(cfg.queue_pairs, 1);
    }
}
