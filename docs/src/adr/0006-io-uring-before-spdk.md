# ADR-0006: io_uring Before SPDK

- **Status:** Accepted
- **Date:** 2026-07-14

## Context

FlyBy targets sub-10 µs storage latency for NVMe tick-data replay.  Two
advanced storage backends are candidates:

**io_uring** (Linux 5.1+):
- Operates through the kernel block layer; the kernel NVMe driver handles
  DMA, interrupt coalescing, and queue management.
- Accessed via two ring buffers (SQ/CQ) shared between userspace and the kernel.
- Latency: typically 10–20 µs for 512-byte reads on a consumer NVMe SSD.
- Deployment: ships with every modern Linux kernel; no external dependencies.
- Development: `liburing` provides a stable C API; Rust bindings exist.

**SPDK** (userspace NVMe):
- Binds the NVMe PCIe device directly into the process using VFIO/UIO;
  the kernel NVMe driver must be unbound.
- I/O is submitted by writing to MMIO registers — no syscall, no interrupt.
  The process must busy-poll the CQ; one CPU core is fully dedicated.
- Latency: 2–5 µs on the same hardware, but only under continuous polling.
- Deployment: requires hugepages, VFIO setup, compatible NIC, root access,
  and rebuilding SPDK from source.  Not available in most CI environments.
- Operational impact: the NVMe device is invisible to the OS while SPDK owns
  it; filesystem access and other processes cannot use it.

## Decision

Implement io_uring before SPDK.

SPDK is added as a later milestone, after:

1. io_uring is stable and its throughput/latency is measured against
   `FileSource`.
2. A production workload has been identified where io_uring latency is
   measurably insufficient.
3. A test environment with a compatible NVMe device and SPDK installation is
   available.

## Consequences

- The io_uring backend serves most production workloads where tick-data replay
  or large-file ingest is required.
- SPDK's 2–5 µs advantage over io_uring matters only if the total pipeline
  latency budget is already dominated by storage I/O — which requires
  measurement, not assumption.
- The `spdk` Cargo feature already exists; enabling it returns
  `ErrorKind::FeatureNotEnabled` until the backend is implemented.
- SPDK introduces significant operational complexity; its deployment guide will
  be a first-class deliverable when the time comes.

## Alternatives Considered

- **SPDK first**: rejected — SPDK's deployment requirements block development
  on all but a very specific hardware configuration, and there is no measured
  evidence that io_uring latency is insufficient.
- **Both in parallel**: rejected — the SPDK backend requires hardware that most
  contributors and CI runners do not have; parallel development would create a
  perennial "works on my machine" asymmetry.

## References

- Jens Axboe: *Efficient IO with io_uring* (2019)
  <https://kernel.dk/io_uring.pdf>
- SPDK documentation: <https://spdk.io/doc/>
- NVMe specification 2.0: <https://nvmexpress.org/specifications/>
