//! DPDK source backend (design placeholder).
//!
//! DPDK (Data Plane Development Kit) is a userspace packet processing
//! framework. It bypasses the Linux kernel networking stack entirely,
//! polling NIC queues directly from userspace via poll-mode drivers (PMDs).
//!
//! ## Why DPDK is deferred (ADR-002)
//!
//! DPDK is powerful but operationally heavy. It introduces C/FFI
//! dependencies, requires hugepages and privileged driver setup, and is
//! difficult to validate in standard CI. AF_XDP is the better first real
//! networking backend because it is closer to the Linux networking stack
//! and has lower operational burden.
//!
//! DPDK's primary advantage (tight latency control with poll-mode drivers
//! and CPU pinning) is only relevant at packet rates that should first be
//! validated with AF_XDP.
//!
//! ## How DPDK works (conceptual)
//!
//! ```text
//! NIC bound to VFIO/UIO driver (kernel driver unloaded)
//!     ↓
//! EAL initialised (hugepages, core mask, device PCI address)
//!     ↓
//! mempool allocated (hugepage-backed mbuf storage)
//!     ↓
//! rte_eth_rx_burst() — polls RX queue directly from userspace
//!     ↓
//! mbuf batch → FlyBy RawBatch → Decode → Placement → Sink
//! ```
//!
//! ## Concepts to understand before implementing
//!
//! - **EAL** (Environment Abstraction Layer): DPDK's init layer. Takes
//!   `-c <core_mask> -n <mem_channels> -a <pci_addr>` arguments.
//! - **Hugepages**: 2M or 1G pages required for DMA-safe mbuf storage.
//!   Configure with `/sys/kernel/mm/hugepages/`.
//! - **mempools**: fixed-size object pools for mbufs. Must be
//!   NUMA-local to the core receiving packets.
//! - **mbufs**: DPDK's packet buffer type. Contains header fields and a
//!   data pointer into the mempool.
//! - **PMD** (Poll Mode Driver): userspace NIC driver. No interrupts.
//! - **lcores**: DPDK's logical core abstraction, mapped to CPU cores.
//! - **burst receive**: `rte_eth_rx_burst()` returns up to N mbufs per
//!   call. Empty-poll rate is a key efficiency metric.
//!
//! ## Operational requirements
//!
//! - Linux host with DPDK ≥ 22.11 installed.
//! - NIC bound to `vfio-pci` or `uio_pci_generic` (not the kernel driver).
//! - Hugepages configured: `echo 512 > /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages`.
//! - `dpdk-devbind.py --bind=vfio-pci <pci_addr>` before starting.
//! - `CAP_SYS_ADMIN` for VFIO.
//!
//! ## FlyBy implementation strategy
//!
//! The recommended approach (see §23 of Part IV):
//!
//! 1. Keep DPDK behind the `dpdk` feature flag (it is already).
//! 2. Isolate all FFI in a `ffi` submodule with explicit safety comments.
//! 3. Wrap mbufs in a safe `Mbuf` newtype; never expose raw `rte_mbuf *`
//!    in the public API.
//! 4. Use `RawBatch` as the handoff point: copy mbuf data into pre-allocated
//!    batch slots (copy mode first), then zero-copy as a follow-up.
//! 5. Prototype outside the main crate before integration.
//!
//! ## Status
//!
//! Design placeholder. Returns [`flyby_core::ErrorKind::FeatureNotEnabled`].
//! The concrete FFI binding is a future deliverable after AF_XDP is stable.

use flyby_core::{Error, ErrorKind, Lifecycle, Result, Source};

use crate::batch::RawBatch;
use crate::config::DpdkConfig;
use crate::source::{BackpressurePolicy, NetworkSource};

/// DPDK source: polls a NIC RX queue via a userspace poll-mode driver.
///
/// See the module documentation for the full design description.
///
/// # Current status
///
/// Placeholder. Returns [`ErrorKind::FeatureNotEnabled`] on all operations.
/// Enable the `dpdk` feature flag to compile this type.
pub struct DpdkSource {
    config: DpdkConfig,
}

impl DpdkSource {
    /// Construct a DPDK source with the given configuration.
    pub fn new(config: DpdkConfig) -> Self {
        Self { config }
    }

    /// Return the active configuration.
    pub fn config(&self) -> &DpdkConfig {
        &self.config
    }
}

impl Lifecycle for DpdkSource {
    fn init(&mut self) -> Result<()> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "DPDK backend is not yet implemented. \
             See ADR-002 (af_xdp before dpdk) and crates/flyby-net/src/dpdk.rs \
             for the design description and operational requirements.",
        ))
    }
}

impl Source for DpdkSource {
    fn poll(&mut self) -> Result<Option<&[u8]>> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "DPDK backend is not yet implemented",
        ))
    }
}

impl NetworkSource for DpdkSource {
    fn poll_batch(&mut self, _batch: &mut RawBatch) -> Result<usize> {
        Err(Error::new(
            ErrorKind::FeatureNotEnabled,
            "DPDK backend is not yet implemented",
        ))
    }

    fn backpressure_policy(&self) -> BackpressurePolicy {
        BackpressurePolicy::DropNewest
    }

    fn backend_name(&self) -> &'static str {
        "dpdk"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_returns_feature_not_enabled() {
        let mut src = DpdkSource::new(DpdkConfig::default());
        let err = src.init().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FeatureNotEnabled);
    }

    #[test]
    fn config_is_accessible() {
        let config = DpdkConfig { rx_queue_id: 3, burst_size: 16, ..DpdkConfig::default() };
        let src = DpdkSource::new(config);
        assert_eq!(src.config().rx_queue_id, 3);
        assert_eq!(src.config().burst_size, 16);
    }

    #[test]
    fn backend_name() {
        let src = DpdkSource::new(DpdkConfig::default());
        assert_eq!(src.backend_name(), "dpdk");
    }
}
