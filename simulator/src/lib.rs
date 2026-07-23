//! FlyBy Simulator: declarative, deterministic simulation of the full
//! ingestion pipeline.
//!
//! The simulator lets you develop, test, and benchmark pipeline code without
//! real network hardware.  It provides:
//!
//! - **[`VirtualNic`]**: a [`NetworkSource`][flyby_net::NetworkSource]
//!   implementation driven by a configurable [`TrafficPattern`] and
//!   [`FaultSpec`].
//! - **[`VirtualStorageSource`]**: a [`StorageSource`][flyby_storage::StorageSource]
//!   implementation wrapping [`FileSource`][flyby_storage::FileSource] with
//!   fault injection.
//! - **[`SimScheduler`]**: drives virtual time, polls NICs, feeds a
//!   [`VirtualSharedMemory`] ring, and drains [`VirtualConsumer`]s.
//! - **[`Scenario`]**: declarative run configuration; built-in presets for
//!   common workloads.
//! - **[`SimClock`]**: real-time or virtual-time clock for deterministic tests.
//! - **[`FaultInjector`]**: LCG-based deterministic fault injection.
//! - **[`SimEvent`]** / **[`EventSink`]**: structured event tracing.
//! - **[`SimMetricKey`]**: metric namespace for the simulator subsystem.
//! - **[`SimReplay`]**: virtual-clock adapter over storage replay modes.
//! - **TUI dashboard** (feature `tui`): Ratatui view of queues, clock, events.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use flyby_simulator::{Scenario, SimScheduler, VirtualNic, VirtualNicConfig};
//! use flyby_simulator::events::NullEventSink;
//!
//! let scenario = Scenario::constant_rate();
//! let mut sched = SimScheduler::new(scenario.clone());
//! let nic = VirtualNic::new(
//!     VirtualNicConfig { traffic: scenario.traffic.clone(), ..VirtualNicConfig::default() },
//!     NullEventSink,
//! );
//! sched.add_nic(nic);
//! let stats = sched.run().unwrap();
//! println!("packets generated: {}", stats.packets_generated);
//! ```
//!
//! ## Simulated results
//!
//! Throughput and latency numbers from this crate are **simulated**.  They
//! are suitable for relative comparisons and correctness testing, not for
//! quoting as production hardware performance.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod clock;
pub mod consumer;
pub mod events;
pub mod fault;
pub mod generator;
pub mod metrics;
pub mod nic;
pub mod pcap;
pub mod replay;
pub mod ring;
pub mod scenario;
pub mod scheduler;
pub mod storage;
pub mod traffic;

#[cfg(feature = "tui")]
pub mod tui;

// Flat re-exports for the most commonly used types.
pub use clock::{ClockMode, SimClock};
pub use consumer::VirtualConsumer;
pub use events::{EventSink, NullEventSink, SimEvent, SimEventKind, VecEventSink};
pub use fault::{FaultInjector, FaultSpec};
pub use generator::{
    CustomPayloadFn, PayloadGenerator, PayloadSpec, ProtocolMessage, build_udp_frame,
};
pub use metrics::SimMetricKey;
pub use nic::{VirtualNic, VirtualNicConfig};
pub use pcap::{
    PcapConfig, PcapPacket, PcapSource, load_pcap, parse_pcap, write_pcap_bytes,
    write_pcap_bytes_ex,
};
pub use replay::SimReplay;
pub use ring::{RingError, VirtualSharedMemory};
pub use scenario::Scenario;
pub use scheduler::{DynNic, EduControls, SimScheduler, SimStats};
pub use storage::{VirtualStorageConfig, VirtualStorageSource};
pub use traffic::{TrafficConfig, TrafficPacer, TrafficPattern};
