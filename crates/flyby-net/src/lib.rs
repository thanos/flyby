#![forbid(unsafe_code)]
#![doc = include_str!("../docs/README.md")]
//!
//! # flyby-net
//!
//! Networking backends for FlyBy: AF_XDP and DPDK.
//!
//! Both backends are Linux-specific and require kernel / userspace
//! support that is not available in a generic dev container. They are
//! therefore gated behind the `af_xdp` and `dpdk` feature flags and
//! compile to a stub when neither is enabled.
//!
//! No `unsafe` lives in this crate yet. When the real backends land,
//! all `unsafe` will be isolated in clearly marked modules with a
//! safety-comment per block, per the project's design principles.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

/// AF_XDP backend.
///
/// Enabled by the `af_xdp` feature. Currently a placeholder so the
/// workspace compiles; the real XSK binding arrives with Part IV.
#[cfg(feature = "af_xdp")]
pub mod af_xdp {
    /// Placeholder for the future AF_XDP source.
    #[derive(Debug, Default)]
    pub struct AfXdpSource;
}

/// DPDK backend.
///
/// Enabled by the `dpdk` feature. Currently a placeholder so the
/// workspace compiles; the real DPDK binding arrives with Part IV.
#[cfg(feature = "dpdk")]
pub mod dpdk {
    /// Placeholder for the future DPDK source.
    #[derive(Debug, Default)]
    pub struct DpdkSource;
}
