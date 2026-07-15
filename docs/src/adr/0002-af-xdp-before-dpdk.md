# ADR-0002: AF_XDP Before DPDK

- **Status:** Accepted
- **Date:** 2026-07-14

## Context

FlyBy needs a high-performance network source. Two mature options exist:
AF_XDP (a Linux socket family using XDP/eBPF for kernel-to-userspace packet
hand-off) and DPDK (a userspace toolkit that bypasses the kernel entirely
via poll-mode drivers).

Both provide packet rates beyond what standard `AF_PACKET` can deliver.
However, they differ significantly in operational complexity:

- AF_XDP works within the Linux networking stack using standard sockets and
  eBPF. It requires `CAP_BPF` + `CAP_NET_ADMIN` but no hugepages, no driver
  binding, and no out-of-tree software.
- DPDK bypasses the kernel. It requires hugepage configuration, NIC driver
  binding (VFIO/UIO), an EAL initialisation layer, and careful NUMA and
  core placement. It also introduces a heavy C/FFI dependency.

## Decision

Implement AF_XDP before DPDK.

## Consequences

- The first real networking backend is Linux-specific but has low operational
  burden relative to DPDK.
- The abstraction layer (`NetworkSource`, `RawBatch`) is validated against a
  real backend before DPDK adds its operational complexity.
- DPDK remains a future deliverable. The `dpdk` feature flag is reserved and
  the `DpdkConfig` type already defines the intended configuration surface.
- True zero-copy and the tightest latency control may require DPDK for some
  workloads. That trade-off is deferred until AF_XDP is measured and stable.

## Alternatives Considered

- **DPDK first**: rejected because the operational burden would slow down
  early abstraction work and is difficult to validate in standard CI.
- **Raw sockets first**: too slow; would not validate the high-performance
  path that justifies the complexity of the networking subsystem.
- **Simulator only**: cannot validate the real packet-acquisition path.
