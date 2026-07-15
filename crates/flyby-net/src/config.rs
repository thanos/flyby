//! Configuration types for network backends.
//!
//! Each backend has its own config struct. All are deliberately plain data
//! (no methods, no validation inside the struct) so they can be constructed
//! from TOML, environment variables, or code equally easily.
//!
//! ## Example (TOML)
//!
//! ```toml
//! [source]
//! kind = "af_xdp"
//! interface = "eth1"
//! queue_id = 0
//! mode = "copy"
//! poll_budget = 64
//!
//! [source.xdp]
//! program = "redirect"
//! filter_udp_port = 9000
//! attach_mode = "native"
//!
//! [source.umem]
//! frame_size = 2048
//! frame_count = 4096
//! ```

/// AF_XDP copy/zero-copy operating mode.
///
/// Always set this explicitly. When `Auto` is selected the driver tries
/// zero-copy and falls back to copy when unavailable. Silent downgrade is
/// not acceptable: the active mode **must** be logged and emitted as a
/// metric (ADR-0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XdpMode {
    /// The kernel copies each packet from the NIC ring into UMEM.
    /// Works on any NIC with a kernel driver. Preferred for initial
    /// development and testing.
    #[default]
    Copy,
    /// The NIC DMA's packets directly into UMEM. Requires a compatible
    /// NIC driver and correct queue setup. Benchmark before claiming
    /// any performance advantage over copy mode.
    ZeroCopy,
    /// Try zero-copy; fall back to copy with mandatory log + metric.
    Auto,
}

impl XdpMode {
    /// Returns `true` if this mode may perform zero-copy transfer.
    pub fn may_zero_copy(self) -> bool {
        matches!(self, XdpMode::ZeroCopy | XdpMode::Auto)
    }
}

/// XDP/eBPF program configuration.
///
/// The XDP program runs in the kernel and is responsible for redirecting
/// packets to the AF_XDP socket. It must stay minimal: packet filtering
/// only. Business logic belongs in userspace.
///
/// # Requirements
///
/// - Linux kernel ≥ 5.4 (≥ 5.10 recommended for zero-copy stability).
/// - `CAP_SYS_ADMIN` or `CAP_BPF` for program loading.
/// - `CAP_NET_ADMIN` for XDP attachment.
///
/// # Warning
///
/// Zero-copy mode additionally requires a NIC driver that exports AF_XDP
/// support (`ethtool --show-features eth0 | grep xdp`). Docker Desktop
/// on macOS and most GitHub-hosted CI runners do **not** provide this.
#[derive(Debug, Clone)]
pub struct XdpConfig {
    /// Which XDP program to load. `"redirect"` is the built-in
    /// pass-through-and-redirect program.
    pub program: String,
    /// If non-zero, only redirect UDP packets on this destination port.
    pub filter_udp_port: u16,
    /// XDP attachment mode: `"native"`, `"generic"`, or `"offload"`.
    ///
    /// `"native"` is preferred when the driver supports it.
    /// `"generic"` (SKB mode) works on any driver but has higher overhead.
    pub attach_mode: String,
}

impl Default for XdpConfig {
    fn default() -> Self {
        Self {
            program: "redirect".into(),
            filter_udp_port: 0,
            attach_mode: "native".into(),
        }
    }
}

/// UMEM (userspace memory) configuration for the AF_XDP backend.
///
/// UMEM is the memory region shared between the kernel and the AF_XDP
/// socket. It holds packet frame buffers. It is a **separate memory
/// domain** from the FlyBy shared-memory sink — do not confuse them.
///
/// True end-to-end zero-copy (UMEM → shared-memory sink without a copy)
/// is a separate and harder problem that is not claimed in v0.1.
#[derive(Debug, Clone)]
pub struct UmemConfig {
    /// Size in bytes of each UMEM frame. Must be a power of two.
    /// Typical values: 2048, 4096.
    pub frame_size: usize,
    /// Number of frames in the UMEM region. Must be a power of two.
    pub frame_count: usize,
}

impl Default for UmemConfig {
    fn default() -> Self {
        Self {
            frame_size: 2048,
            frame_count: 4096,
        }
    }
}

/// Full configuration for the AF_XDP source backend.
///
/// # Hardware requirements
///
/// - Linux host (not Docker Desktop on macOS).
/// - Kernel ≥ 5.4 for copy mode; ≥ 5.10 for zero-copy.
/// - `CAP_SYS_ADMIN` or `CAP_BPF` + `CAP_NET_ADMIN`.
/// - NIC driver with AF_XDP support for zero-copy mode.
///
/// # CI limitations
///
/// GitHub-hosted runners cannot run AF_XDP. Use the simulator or a
/// self-hosted Linux runner with a compatible NIC for hardware tests.
#[derive(Debug, Clone)]
pub struct AfXdpConfig {
    /// Network interface name (e.g. `"eth1"`, `"ens3"`).
    pub interface: String,
    /// NIC queue index to bind. Pinning to a specific queue is strongly
    /// recommended to avoid cross-queue coordination.
    pub queue_id: u32,
    /// Copy or zero-copy mode.
    pub mode: XdpMode,
    /// Maximum packets to pull from the RX ring per poll call.
    pub poll_budget: usize,
    /// XDP/eBPF program settings.
    pub xdp: XdpConfig,
    /// UMEM layout settings.
    pub umem: UmemConfig,
}

impl Default for AfXdpConfig {
    fn default() -> Self {
        Self {
            interface: "eth0".into(),
            queue_id: 0,
            mode: XdpMode::Copy,
            poll_budget: 64,
            xdp: XdpConfig::default(),
            umem: UmemConfig::default(),
        }
    }
}

/// Configuration for the in-process simulated network source.
///
/// Useful for developing parsers, placement logic, and sinks without
/// real hardware. The simulator generates Ethernet/IP/UDP shaped packets.
///
/// Call [`SimNetConfig::validate`] before use (also invoked by
/// [`crate::sim::SimulatedNetSource::try_new`] and `init`).
#[derive(Debug, Clone)]
pub struct SimNetConfig {
    /// Payload bytes appended after the UDP header.
    /// Default: 8 bytes (a u64 sequence number, big-endian).
    pub payload_size: usize,
    /// Packets to attempt per [`poll_batch`][crate::source::NetworkSource::poll_batch]
    /// call. Must be > 0. When larger than the batch capacity, excess is
    /// counted as drops.
    pub batch_size: usize,
    /// Fraction of polls that return zero packets (simulate idle NIC).
    /// Must be in `[0.0, 1.0)`.
    pub idle_rate: f32,
    /// Fraction of packets to deliberately drop (simulates NIC drops).
    /// Must be in `[0.0, 1.0)`.
    pub drop_rate: f32,
    /// UDP destination port written into simulated packet headers.
    pub udp_dst_port: u16,
}

impl Default for SimNetConfig {
    fn default() -> Self {
        Self {
            payload_size: 8,
            batch_size: 32,
            idle_rate: 0.0,
            drop_rate: 0.0,
            udp_dst_port: 9000,
        }
    }
}

impl SimNetConfig {
    /// Validate configuration constraints.
    pub fn validate(&self) -> flyby_core::Result<()> {
        if self.batch_size == 0 {
            return Err(flyby_core::Error::config("batch_size must be > 0"));
        }
        if !(0.0..1.0).contains(&self.idle_rate) {
            return Err(flyby_core::Error::config("idle_rate must be in [0.0, 1.0)"));
        }
        if !(0.0..1.0).contains(&self.drop_rate) {
            return Err(flyby_core::Error::config("drop_rate must be in [0.0, 1.0)"));
        }
        Ok(())
    }
}

impl UmemConfig {
    /// Validate UMEM geometry.
    pub fn validate(&self) -> flyby_core::Result<()> {
        if self.frame_size == 0 || !self.frame_size.is_power_of_two() {
            return Err(flyby_core::Error::config(
                "umem frame_size must be a non-zero power of two",
            ));
        }
        if self.frame_count == 0 || !self.frame_count.is_power_of_two() {
            return Err(flyby_core::Error::config(
                "umem frame_count must be a non-zero power of two",
            ));
        }
        Ok(())
    }
}

impl AfXdpConfig {
    /// Validate AF_XDP configuration.
    pub fn validate(&self) -> flyby_core::Result<()> {
        if self.interface.is_empty() {
            return Err(flyby_core::Error::config("interface must not be empty"));
        }
        if self.poll_budget == 0 {
            return Err(flyby_core::Error::config("poll_budget must be > 0"));
        }
        self.umem.validate()?;
        Ok(())
    }
}

impl DpdkConfig {
    /// Validate DPDK configuration.
    pub fn validate(&self) -> flyby_core::Result<()> {
        if self.pci_addr.is_empty() {
            return Err(flyby_core::Error::config("pci_addr must not be empty"));
        }
        if self.burst_size == 0 {
            return Err(flyby_core::Error::config("burst_size must be > 0"));
        }
        Ok(())
    }
}

/// Configuration for the DPDK source backend (design placeholder).
///
/// # Requirements
///
/// - External DPDK installation (≥ 22.11 recommended).
/// - Hugepages configured (`/sys/kernel/mm/hugepages/`).
/// - NIC bound to a VFIO or UIO driver.
/// - EAL arguments (core mask, memory channels, device PCI address).
///
/// # Status
///
/// DPDK is deferred after AF_XDP (see ADR-002). This struct defines the
/// intended configuration surface; the binding is a future deliverable.
#[derive(Debug, Clone)]
pub struct DpdkConfig {
    /// PCI address of the NIC (e.g. `"0000:00:1f.6"`).
    pub pci_addr: String,
    /// EAL core mask (e.g. `"0x3"` for cores 0 and 1).
    pub core_mask: String,
    /// Number of hugepages to pre-allocate.
    pub hugepage_count: usize,
    /// RX queue index to bind.
    pub rx_queue_id: u16,
    /// Maximum packets per burst receive call.
    pub burst_size: u16,
}

impl Default for DpdkConfig {
    fn default() -> Self {
        Self {
            pci_addr: String::new(),
            core_mask: "0x1".into(),
            hugepage_count: 512,
            rx_queue_id: 0,
            burst_size: 32,
        }
    }
}
