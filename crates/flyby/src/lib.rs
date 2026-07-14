#![forbid(unsafe_code)]
#![doc = include_str!("../docs/README.md")]
//!
//! # flyby
//!
//! The public facade of the FlyBy framework.
//!
//! Users should normally depend on this crate and write:
//!
//! ```rust
//! use flyby::prelude::*;
//! ```
//!
//! rather than depending on the internal crates (`flyby-core`,
//! `flyby-memory`, ...) directly. The facade re-exports the stable API
//! and gates backend-specific items behind feature flags.
//!
//! ## Feature flags
//!
//! | Feature       | Default | Description                                  |
//! |---------------|---------|----------------------------------------------|
//! | `memory`      | yes     | In-process shared-memory sink.               |
//! | `af_xdp`      | no      | AF_XDP source (Linux eBPF / XSK).            |
//! | `dpdk`        | no      | DPDK source.                                 |
//! | `io_uring`    | no      | io_uring storage backend.                    |
//! | `spdk`        | no      | SPDK storage backend.                        |
//! | `simulator`   | no      | In-process simulator source.                |
//! | `benchmarks`  | no      | Build the benchmark harnesses.              |
//!
//! Heavy dependencies are never enabled by default.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use flyby_core as core;

/// Core traits, errors, and lifecycle.
pub mod api {
    pub use flyby_core::{
        Decoder, DefaultSchemaId, Error, ErrorKind, Lifecycle, Message, Metadata, MetricKey,
        MetricKind, MetricsCollector, NullCollector, Pipeline, Placement, PreProcessor, Result,
        SchemaId, Sink, SinkId, Source, Timestamp,
    };
}

/// The public prelude.
///
/// Import this to get the full stable API in one line:
///
/// ```rust
/// use flyby::prelude::*;
/// ```
pub mod prelude {
    pub use crate::api::*;
    pub use crate::builder::{FlyBy, FlyByBuilder};
}

pub mod builder;

// --- Backend re-exports (gated by feature flags) ---------------------------

/// Shared-memory sink.
#[cfg(feature = "memory")]
pub mod memory {
    pub use flyby_memory::*;
}

/// Networking backends (AF_XDP, DPDK).
#[cfg(any(feature = "af_xdp", feature = "dpdk"))]
pub mod net {
    pub use flyby_net::*;
}

/// Storage backends (io_uring, SPDK).
#[cfg(any(feature = "io_uring", feature = "spdk"))]
pub mod storage {
    pub use flyby_storage::*;
}
