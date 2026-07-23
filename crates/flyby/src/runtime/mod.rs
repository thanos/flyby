//! Part VII runtime: scheduling, configuration, back-pressure, and telemetry.
//!
//! The runtime coordinates every pipeline regardless of source or sink
//! (ADR-010). It owns lifecycle orchestration, replaceable schedulers,
//! batching defaults, flow-control policy, and metric hooks — never AF_XDP,
//! DPDK, io_uring, or SPDK details.
//!
//! ```text
//! RuntimeConfig  →  Scheduler  →  Pipeline::step_outcome
//!        ↓
//!  BackpressureStrategy + RuntimeMetrics
//! ```

mod affinity;
mod backpressure;
mod config;
mod driver;
mod scheduler;
mod telemetry;

pub use affinity::{AffinityRequest, CpuAffinityPolicy, apply_affinity};
pub use backpressure::BackpressureStrategy;
pub use config::{RuntimeConfig, SchedulerKind};
pub use driver::{Runtime, RuntimePhase, RuntimeStats, run_pipeline, run_with_worker_factory};
pub use scheduler::{
    Scheduler, SingleThreadScheduler, WorkerPoolScheduler, build_scheduler, run_single_thread,
    shutdown_flag,
};
pub use telemetry::{RuntimeEvent, RuntimeMetricKey, emit_lifecycle};
