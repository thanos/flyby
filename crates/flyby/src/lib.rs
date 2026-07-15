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
//! | `af_xdp`      | no      | AF_XDP source stub (Linux eBPF / XSK).       |
//! | `dpdk`        | no      | DPDK source stub.                            |
//! | `io_uring`    | no      | io_uring storage backend stub.               |
//! | `spdk`        | no      | SPDK storage backend stub.                   |
//! | `simulator`   | no      | Builder flag for the in-process net simulator. |
//! | `benchmarks`  | no      | Reserved for optional bench wiring.          |
//!
//! Portable APIs (`flyby-net` simulator, `flyby-storage` file source) always
//! compile as dependencies of this facade. Heavy stubs are feature-gated.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use flyby_core as core;

/// Core traits, errors, and lifecycle.
pub mod api {
    pub use flyby_core::{
        Decoder, DefaultSchemaId, Encode, Error, ErrorKind, Lifecycle, Message, Metadata,
        MetricKey, MetricKind, MetricsCollector, NullCollector, Pipeline, Placement, PreProcessor,
        Result, SchemaId, Sink, SinkId, Source, StepOutcome, Timestamp,
    };
}

/// The public prelude.
pub mod prelude {
    pub use crate::api::*;
    pub use crate::builder::{FlyBy, FlyByBuilder};
}

pub mod builder;

// --- Backend re-exports ----------------------------------------------------

/// Shared-memory sink (default backend).
#[cfg(feature = "memory")]
pub mod memory {
    pub use flyby_memory::*;
}

/// Networking backends: always re-exports the portable simulator and batch
/// types; AF_XDP/DPDK appear when their features are enabled.
pub mod net {
    pub use flyby_net::*;
}

/// Storage backends: always re-exports the portable file source and framing;
/// io_uring/SPDK appear when their features are enabled.
pub mod storage {
    pub use flyby_storage::*;
}
