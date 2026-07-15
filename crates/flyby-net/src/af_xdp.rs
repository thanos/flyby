//! AF_XDP source backend (design stub).
//!
//! AF_XDP is a Linux socket family that enables high-performance packet
//! processing entirely in userspace. It works in conjunction with XDP/eBPF:
//! a small eBPF program running in the kernel redirects packets from the NIC
//! receive path directly into a userspace memory region called **UMEM**.
//!
//! ## How AF_XDP works
//!
//! ```text
//! NIC receives packet
//!     ↓
//! XDP/eBPF program runs (in kernel, on the driver receive path)
//!     ↓
//! program calls bpf_redirect_map() → AF_XDP socket
//!     ↓
//! kernel DMA's packet into UMEM frame (zero-copy) or copies it (copy mode)
//!     ↓
//! kernel writes descriptor to RX ring
//!     ↓
//! FlyBy userspace reads RX ring → decodes packet → routes to sink
//! ```
//!
//! ## Rings
//!
//! AF_XDP uses four rings (FlyBy v0.1 uses only the first two):
//!
//! | Ring | Direction | Purpose |
//! |------|-----------|---------|
//! | RX ring | kernel → userspace | Received packet descriptors |
//! | Fill ring | userspace → kernel | Free UMEM frame addresses |
//! | TX ring | userspace → kernel | Transmit descriptors (deferred) |
//! | Completion ring | kernel → userspace | Transmitted frame confirmations (deferred) |
//!
//! ## UMEM vs FlyBy shared-memory sink
//!
//! These are **different memory domains**:
//!
//! - **UMEM**: shared between the kernel and the AF_XDP socket. Contains
//!   raw packet frames. Managed by `mmap` + `setsockopt(XDP_UMEM_REG)`.
//! - **FlyBy shared-memory sink**: the typed message ring from Part III.
//!   Contains decoded, structured messages. Managed by flyby-memory.
//!
//! A packet flows from UMEM → decode → FlyBy ring. The copy happens at
//! the decode boundary. True end-to-end zero-copy into the FlyBy sink is
//! a separate and harder problem; it must not be claimed without measurement.
//!
//! ## Copy mode vs zero-copy
//!
//! | Mode | NIC requirement | Kernel requirement | Status |
//! |------|----------------|-------------------|--------|
//! | Copy | any driver | ≥ 4.18 | v0.2 target |
//! | Zero-copy | AF_XDP-capable driver | ≥ 5.4 | v0.3 target |
//!
//! Always start with copy mode. Benchmark both before claiming any
//! latency advantage.
//!
//! ## Operational requirements
//!
//! - Linux host (macOS and Docker Desktop are **not** supported).
//! - Kernel ≥ 5.4 recommended (≥ 5.10 for production zero-copy).
//! - `CAP_SYS_ADMIN` or (`CAP_BPF` + `CAP_NET_ADMIN`) to load the XDP program.
//! - NIC driver with AF_XDP support (check with `ethtool --show-features`).
//! - Setup scripts: `scripts/net/setup-afxdp-dev.sh`
//!
//! ## Status
//!
//! This is a **design stub**. The implementation requires a Linux host with
//! the capabilities listed above. The concrete binding — UMEM allocation,
//! ring mmap, XDP program load, and packet polling loop — is a subsequent
//! deliverable. All unsafe code will be isolated in clearly-marked modules
//! with a safety comment per block.

use flyby_core::{Error, ErrorKind, Lifecycle, Result, Source};

use crate::batch::RawBatch;
use crate::config::AfXdpConfig;
use crate::source::{BackpressurePolicy, NetworkSource};

/// AF_XDP source: receives packets from a Linux NIC queue via an XSK socket.
///
/// See the module documentation for the full design description and
/// operational requirements.
///
/// # Current status
///
/// Stub. Returns [`ErrorKind::FeatureNotEnabled`] until the binding is
/// implemented. Enable the `af_xdp` feature flag to compile this type.
pub struct AfXdpSource {
    config: AfXdpConfig,
}

impl AfXdpSource {
    /// Construct an AF_XDP source with the given configuration.
    pub fn new(config: AfXdpConfig) -> Self {
        Self { config }
    }

    /// Return the active configuration.
    pub fn config(&self) -> &AfXdpConfig {
        &self.config
    }
}

impl Lifecycle for AfXdpSource {
    fn init(&mut self) -> Result<()> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "AF_XDP backend is not yet implemented. \
             Enable the `af_xdp` feature and see crates/flyby-net/src/af_xdp.rs \
             for design documentation and operational requirements.",
        ))
    }
}

impl Source for AfXdpSource {
    fn poll(&mut self) -> Result<Option<&[u8]>> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "AF_XDP backend is not yet implemented",
        ))
    }
}

impl NetworkSource for AfXdpSource {
    fn poll_batch(&mut self, _batch: &mut RawBatch) -> Result<usize> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "AF_XDP backend is not yet implemented",
        ))
    }

    fn backpressure_policy(&self) -> BackpressurePolicy {
        BackpressurePolicy::DropNewest
    }

    fn backend_name(&self) -> &'static str {
        "af_xdp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_returns_feature_not_enabled() {
        let mut src = AfXdpSource::new(AfXdpConfig::default());
        let err = src.init().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FeatureNotEnabled);
    }

    #[test]
    fn config_is_accessible() {
        let config = AfXdpConfig { interface: "eth1".into(), queue_id: 2, ..AfXdpConfig::default() };
        let src = AfXdpSource::new(config);
        assert_eq!(src.config().interface, "eth1");
        assert_eq!(src.config().queue_id, 2);
    }

    #[test]
    fn backend_name() {
        let src = AfXdpSource::new(AfXdpConfig::default());
        assert_eq!(src.backend_name(), "af_xdp");
    }
}
