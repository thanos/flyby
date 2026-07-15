//! # flyby-net
//!
//! Networking backends for FlyBy: AF_XDP, DPDK, and the in-process simulator.
//!
//! ## Architecture
//!
//! ```text
//! NIC → Backend Adapter → RawBatch → Decode → Placement → Sink
//!        (af_xdp | dpdk | sim)
//! ```
//!
//! The networking subsystem owns **packet acquisition only**. It does not
//! own application protocol parsing, shared-memory layout, placement
//! semantics, enrichment logic, or consumer behaviour.
//!
//! ## Feature flags
//!
//! | Feature | Enables |
//! |---------|---------|
//! | *(always)* | [`NetworkSource`], [`RawBatch`], [`SimulatedNetSource`], config types |
//! | `net`   | Parent feature (no-op today; reserved for optional gating) |
//! | `af_xdp` | [`AfXdpSource`] stub (implies `net`) |
//! | `dpdk`  | [`DpdkSource`] stub (implies `net`) |
//!
//! Portable sim/batch types always compile. Heavy bindings are never default.
//!
//! ## Hardware requirements
//!
//! The simulator works on any platform. AF_XDP and DPDK require a real
//! Linux host with specific kernel versions and NIC drivers. Docker Desktop
//! on macOS and standard GitHub-hosted CI are **not** sufficient for
//! hardware-backed networking tests. See the module documentation for each
//! backend for the full requirements list.
//!
//! ## Backpressure
//!
//! Networking sources must not drop packets without accounting. Drops are
//! tracked via [`RawBatch::record_drop`] / [`RawBatch::dropped`] and should
//! be exposed via [`NetMetricKey`]. Oversized frames may be truncated with
//! [`PacketMeta::original_len`] preserved; prefer sizing slots correctly.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod batch;
pub mod config;
pub mod metrics;
pub mod sim;
pub mod source;

#[cfg(feature = "af_xdp")]
pub mod af_xdp;

#[cfg(feature = "dpdk")]
pub mod dpdk;

// ---------------------------------------------------------------------------
// Flat re-exports for the common case: `use flyby_net::*`
// ---------------------------------------------------------------------------

pub use batch::{PacketMeta, PushResult, RawBatch};
pub use config::{AfXdpConfig, DpdkConfig, SimNetConfig, UmemConfig, XdpConfig, XdpMode};
pub use metrics::NetMetricKey;
pub use sim::SimulatedNetSource;
pub use source::{BackpressurePolicy, NetworkSource};

#[cfg(feature = "af_xdp")]
pub use af_xdp::AfXdpSource;

#[cfg(feature = "dpdk")]
pub use dpdk::DpdkSource;
