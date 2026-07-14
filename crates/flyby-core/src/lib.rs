#![forbid(unsafe_code)]
#![doc = include_str!("../docs/README.md")]
//!
//! # flyby-core
//!
//! Platform-independent core of the FlyBy framework.
//!
//! This crate defines the stable programming model that every backend
//! (AF_XDP, io_uring, DPDK, SPDK, simulator, future transports) must
//! implement. It deliberately contains **no** backend-specific code and
//! **no** `unsafe` blocks: it is the contract layer, not the
//! implementation layer.
//!
//! The guiding abstraction is:
//!
//! ```text
//! Source -> Decode -> Transform -> Route -> Sink
//! ```
//!
//! Backends may change. The traits in this crate should not, without an
//! accompanying Architecture Decision Record (ADR).
//!
//! ## Contents
//!
//! | Module      | Purpose                                                  |
//! |-------------|----------------------------------------------------------|
//! | [`message`]     | The [`Message`] trait: typed records flowing downstream. |
//! | [`source`]      | The [`Source`] trait: producers of raw bytes / records.  |
//! | [`sink`]        | The [`Sink`] trait: terminal destinations.               |
//! | [`preprocessor`]| The [`PreProcessor`] trait: enrichment / transform.      |
//! | [`placement`]   | The [`Placement`] trait: routing decisions.              |
//! | [`pipeline`]    | The [`Pipeline`] trait: wiring stages together.          |
//! | [`metrics`]     | The [`MetricsCollector`] trait: observability.           |
//! | [`lifecycle`]   | Lifecycle phases shared by all stages.                   |
//! | [`error`]       | Explicit, typed, contextual errors.                      |
//!
//! ## Design rules
//!
//! - Keep `flyby-core` platform independent.
//! - No `unsafe` ever (enforced by `#![forbid(unsafe_code)]`).
//! - No backend-specific code leaks in here.
//! - Every public item is documented before stabilization.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod decoder;
pub mod encoder;
pub mod error;
pub mod lifecycle;
pub mod message;
pub mod metrics;
pub mod pipeline;
pub mod placement;
pub mod preprocessor;
pub mod sink;
pub mod source;

pub use decoder::Decoder;
pub use encoder::Encode;
pub use error::{Error, ErrorKind, Result};
pub use lifecycle::Lifecycle;
pub use message::{DefaultSchemaId, Message, Metadata, SchemaId, Timestamp};
pub use metrics::{MetricKey, MetricKind, MetricsCollector, NullCollector};
pub use pipeline::Pipeline;
pub use placement::{Placement, SinkId};
pub use preprocessor::PreProcessor;
pub use sink::Sink;
pub use source::Source;

/// Prelude containing the core traits and error types.
///
/// Users should normally write:
///
/// ```rust
/// use flyby_core::prelude::*;
/// ```
pub mod prelude {
    pub use crate::decoder::Decoder;
    pub use crate::encoder::Encode;
    pub use crate::error::{Error, Result};
    pub use crate::lifecycle::Lifecycle;
    pub use crate::message::{DefaultSchemaId, Message, Metadata, SchemaId, Timestamp};
    pub use crate::metrics::{MetricKey, MetricKind, MetricsCollector, NullCollector};
    pub use crate::pipeline::Pipeline;
    pub use crate::placement::{Placement, SinkId};
    pub use crate::preprocessor::PreProcessor;
    pub use crate::sink::Sink;
    pub use crate::source::Source;
}
