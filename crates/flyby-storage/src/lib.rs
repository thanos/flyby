//! Storage backends for the FlyBy framework.
//!
//! Provides high-throughput ingest from persistent storage using the same
//! programming model as the network subsystem:
//!
//! ```text
//! Storage → StorageSource → RawRecordBatch → Decoder → Typed Message → Sink
//! ```
//!
//! ## Backends
//!
//! | Backend | Feature flag | Status |
//! |---|---|---|
//! | [`FileSource`] | always available | implemented |
//! | [`IoUringSource`][io_uring::IoUringSource] | `io_uring` | stub (ADR-0005) |
//! | [`SpdkSource`][spdk::SpdkSource] | `spdk` | stub (ADR-0006) |
//!
//! ## Replay
//!
//! The [`ReplayEngine`] is backend-independent: it
//! controls *when* records are released to the pipeline, not *how* they are
//! read from storage.  Any backend can be combined with any replay mode.
//!
//! ## Framing
//!
//! Records are extracted from the raw byte stream by a [`Frame`]
//! implementation.  Four built-in strategies are provided; custom framers are
//! supported via [`framing::Custom`].
//!
//! ## Feature flags
//!
//! - `io_uring` — compile the io_uring backend (Linux ≥ 5.1).
//! - `spdk` — compile the SPDK backend (requires an external SPDK installation).
//!
//! Neither flag enables the corresponding feature in production code today;
//! both stubs return [`ErrorKind::NotImplemented`][flyby_core::ErrorKind::NotImplemented].
//! The flags exist to keep the API surface stable as the backends are developed.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod batch;
pub mod config;
pub mod file;
pub mod framing;
pub mod metrics;
pub mod replay;
pub mod source;

#[cfg(feature = "io_uring")]
pub mod io_uring;

#[cfg(feature = "spdk")]
pub mod spdk;

// ---------------------------------------------------------------------------
// Flat re-exports for the most commonly used types.
// ---------------------------------------------------------------------------

pub use batch::{RawRecordBatch, RecordMeta};
pub use config::{EofPolicy, FileConfig, IoUringConfig, SpdkConfig};
pub use file::FileSource;
pub use framing::{
    Custom as CustomFramer, Delimiter, FixedLength, Frame, LengthPrefixed, PrefixWidth,
};
pub use metrics::StorageMetricKey;
pub use replay::{ReplayEngine, ReplayMode};
pub use source::StorageSource;
