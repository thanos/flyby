#![forbid(unsafe_code)]
#![doc = include_str!("../docs/README.md")]
//!
//! # flyby-storage
//!
//! Storage backends for FlyBy: plain files, io_uring, and SPDK.
//!
//! The file backend is portable. The `io_uring` and `spdk` backends are
//! Linux-specific and gated behind feature flags of the same name. They
//! compile to a stub when their feature is disabled.
//!
//! No `unsafe` lives in this crate yet. When the real backends land, all
//! `unsafe` will be isolated in clearly marked modules with a
//! safety-comment per block, per the project's design principles.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

/// Portable file backend.
///
/// Always available. The real implementation (buffered writes, optional
/// `mmap`, fsync policy) arrives with Part V.
pub mod file {
    /// Placeholder for the future file sink.
    #[derive(Debug, Default)]
    pub struct FileSink;
}

/// io_uring backend.
///
/// Enabled by the `io_uring` feature. Currently a placeholder so the
/// workspace compiles; the real io_uring binding arrives with Part V.
#[cfg(feature = "io_uring")]
pub mod io_uring {
    /// Placeholder for the future io_uring sink.
    #[derive(Debug, Default)]
    pub struct IoUringSink;
}

/// SPDK backend.
///
/// Enabled by the `spdk` feature. Currently a placeholder so the
/// workspace compiles; the real SPDK binding arrives with Part V.
#[cfg(feature = "spdk")]
pub mod spdk {
    /// Placeholder for the future SPDK sink.
    #[derive(Debug, Default)]
    pub struct SpdkSink;
}
