# ADR-011: Simulator Required for New Features

- **Status:** Accepted
- **Date:** 2026-07-23
- **Related:** [ADR-0007](./0007-simulator-before-hardware.md), [ADR-0008](./0008-simulator-is-a-product-feature.md)

## Context

Pipeline features that only compile or run on privileged Linux hardware
(AF_XDP, DPDK, io_uring, SPDK) cannot be reviewed or regression-tested on
typical contributor machines or GitHub-hosted CI. Without a portable gate,
regressions land unseen until self-hosted hardware CI runs.

## Decision

New pipeline features **must first be validated in the simulator** (or
another portable path: in-memory sources/sinks, file backends) before they
require privileged hardware. Hardware backends remain the release
validation path, not the first development path.

## Consequences

### Positive

- Portable CI stays the primary gate.
- Contributors can exercise new behaviour without special NICs or NVMe.
- Aligns with ADR-0007 / ADR-0008.

### Negative

- Some hardware-only edge cases still need self-hosted jobs later.
- Authors must provide a simulator or portable test alongside the feature.

## Alternatives considered

**Hardware-first development:** rejected — stalls iteration and CI.

**Optional simulator coverage:** rejected — makes portable CI incomplete.
