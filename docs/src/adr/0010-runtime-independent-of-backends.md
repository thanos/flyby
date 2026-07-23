# ADR-010: Runtime Independent of Backends

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

FlyBy supports many transports (simulator, AF_XDP, DPDK, file, io_uring, SPDK).
If lifecycle, scheduling, back-pressure, and placement embed backend details,
every new transport forks the execution model and the simulator cannot reuse
the same path as production.

## Decision

The **runtime owns execution**, not networking or storage. Schedulers,
`RuntimeConfig`, back-pressure strategies, placement helpers, and telemetry
hooks live in the facade (`flyby::runtime`) and speak only core traits
(`Pipeline`, `Source`/`RawBatchSource`, `Sink`, `Placement`). Backends plug
in as adapters; the product simulator remains a separate subsystem that
shares traits, not runtime internals.

## Consequences

### Positive

- Same execution model for every backend.
- Easier testing with in-memory sources/sinks and the simulator.
- Future transports integrate by implementing traits, not forking the loop.

### Negative

- Some backend-specific optimisations (e.g. AF_XDP busy-poll hints) need
  optional policy hooks rather than hard-wiring.
- CPU affinity / NUMA remain optional, backend-independent policy stubs
  until measured.

## Alternatives considered

**Runtime inside each backend crate:** rejected — duplicates lifecycle and
breaks “one model”.

**Merge `SimScheduler` into the core runtime:** rejected — simulation time
and NIC fault injection are product-sim concerns (ADR-007/008), not the
pipeline execution layer.
