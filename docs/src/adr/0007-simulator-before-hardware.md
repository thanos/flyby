# ADR-007: Simulator Before Hardware Integration

**Status**: Accepted  
**Date**: 2026-07-14

## Context

FlyBy targets kernel-bypass networking (AF_XDP, DPDK) and high-throughput
storage (io_uring, SPDK).  These backends require dedicated hardware,
privileged kernel features, and environment-specific setup that is impossible
on a developer's macOS laptop.

Without a software simulation layer, every change to the pipeline would
require a round-trip to dedicated Linux hardware just to run a smoke test.
That feedback cycle is too slow for iterative development.

## Decision

Implement a feature-complete simulator (`flyby-simulator`) before investing
in hardware integration.  The simulator provides:

- **VirtualNic**: a `Lifecycle + NetworkSource` implementation with
  configurable traffic patterns and fault injection.  API-identical to the
  AF_XDP and DPDK backends.
- **VirtualStorageSource**: a `Lifecycle + StorageSource` implementation
  wrapping `FileSource` with fault injection.
- **SimScheduler**: drives virtual time ticks, polls all virtual sources,
  and returns aggregate statistics without sleeping.
- **Scenario presets**: named, version-controlled configurations
  (`constant_rate`, `market_open_burst`, `packet_loss`, etc.) plus the
  FlyScenario DSL (`scenarios/*.fly.toml`, optional Rhai scripts).
- **Deterministic fault injection**: LCG-seeded, fully reproducible.
- **Observability**: events, metrics, educational step controls, and a
  Ratatui TUI dashboard (`flyby-sim tui`).

## Consequences

### Benefits

- Development and testing work on any platform without hardware access.
- Deterministic scenarios reproduce bugs reliably; seed-based fault injection
  gives exact control over which packets fail.
- Integration tests run in the CI pipeline on macOS without kernel features
  or DPDK/AF_XDP drivers.
- Pipeline code is validated against the simulator's `NetworkSource` and
  `StorageSource` contracts before hardware backends are added.

### Trade-offs

- The simulator cannot reproduce real-hardware timing behaviour (interrupt
  coalescing, NUMA topology, PCIe latency).  Performance numbers from the
  simulator are indicative, not authoritative.
- Virtual-time mode runs much faster than wall-clock time, which may mask
  real-time scheduling bugs.  Complement with real-hardware benchmarks when
  available.
- A discrepancy between simulator and hardware behaviour requires debugging
  in two environments; the simulator may hide hardware-specific bugs.

## Alternatives Considered

**Hardware-first**: implement AF_XDP first and defer the simulator.  Rejected
because it blocks all non-Linux development and makes CI impossible without
specialised runners.

**Mocking without a simulator**: use `mockall` or manual mocks for unit tests.
Rejected because mocks cannot drive realistic packet rates, timing, or traffic
patterns, and tend to diverge from real implementations.
