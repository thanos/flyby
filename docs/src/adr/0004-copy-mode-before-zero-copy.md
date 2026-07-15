# ADR-0004: Copy Mode Before Zero-Copy

- **Status:** Accepted
- **Date:** 2026-07-14

## Context

AF_XDP supports two operating modes:

- **Copy mode**: the kernel copies each received packet from the NIC ring
  into a UMEM frame. Works on any NIC with a standard kernel driver.
  Available since Linux 4.18.
- **Zero-copy mode**: the NIC DMA's packets directly into UMEM frames,
  bypassing any kernel copy. Requires a NIC driver with explicit AF_XDP
  support (`ethtool --show-features | grep xdp-zc`), kernel ≥ 5.4, and
  correct queue setup. Not available on most virtual machines, Docker
  Desktop, or standard CI runners.

Zero-copy's latency advantage exists in theory. In practice, the benefit
depends on memory bandwidth, CPU cache behaviour, and workload. It must
be measured, not assumed.

## Decision

Implement AF_XDP in copy mode first.

Zero-copy is added as a later milestone, after:
1. Copy-mode AF_XDP is stable and tested.
2. A benchmark exists comparing copy mode against the simulator baseline.
3. A real Linux host with a compatible NIC is available for measurement.

## Consequences

- The first AF_XDP implementation works on more hardware and is easier to
  test and debug.
- All benchmark claims from the copy-mode implementation must be
  conservative; they represent the **copy-mode** path, not the theoretical
  zero-copy ceiling.
- The `XdpMode` enum in `AfXdpConfig` already includes `ZeroCopy` and
  `Auto` variants, so zero-copy can be added later without a breaking API
  change.
- `Auto` mode (try zero-copy, fall back to copy) must always log and
  expose a metric indicating which mode is actually active — silent
  downgrade is not acceptable.

## Alternatives Considered

- **Zero-copy first**: rejected because hardware requirements are high and
  it would block all progress on AF_XDP until a compatible test machine is
  available.
- **Only copy mode (forever)**: rejected because zero-copy is the goal for
  production workloads; it must remain an explicit planned milestone.
