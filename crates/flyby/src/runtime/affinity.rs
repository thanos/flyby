//! Optional CPU affinity / NUMA policy hooks (Part VII §6 — future).
//!
//! These types are backend-independent placeholders. Applying affinity is a
//! no-op on platforms without support; the runtime never requires pinning.

use crate::api::{Error, ErrorKind, Result};

/// High-level affinity policy selected via configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CpuAffinityPolicy {
    /// No pinning (default).
    #[default]
    None,
    /// Pin each worker to an isolated core when the OS allows it (future).
    PinWorkers,
    /// Prefer NUMA-local placement of workers (future).
    NumaLocal,
}

/// Concrete affinity request for a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffinityRequest {
    /// Worker index (`0..workers`).
    pub worker_index: usize,
    /// Optional CPU set (empty = leave unset).
    pub cpus: Vec<usize>,
    /// Policy that produced this request.
    pub policy: CpuAffinityPolicy,
}

impl AffinityRequest {
    /// Build a request for `worker_index` under `policy`.
    pub fn for_worker(policy: CpuAffinityPolicy, worker_index: usize) -> Self {
        Self {
            worker_index,
            cpus: Vec::new(),
            policy,
        }
    }
}

/// Attempt to apply affinity. Always succeeds as a documented no-op today.
///
/// Returns [`ErrorKind::NotImplemented`] only when `strict` is true and the
/// policy is not [`CpuAffinityPolicy::None`].
pub fn apply_affinity(req: &AffinityRequest, strict: bool) -> Result<()> {
    match req.policy {
        CpuAffinityPolicy::None => Ok(()),
        CpuAffinityPolicy::PinWorkers | CpuAffinityPolicy::NumaLocal if !strict => Ok(()),
        other => Err(Error::new(
            ErrorKind::NotImplemented,
            format!("CPU affinity policy {other:?} is not yet implemented"),
        )),
    }
}
