# Simulator

Simulation is a first-class development workflow (design principle #5):
the simulator lets a pipeline run end to end without any hardware or
kernel dependencies.

## Where the real simulator lives

The production-ready synthetic source is
`flyby_net::SimulatedNetSource` (also re-exported as `flyby::net::SimulatedNetSource`).
It generates Ethernet/IP/UDP frames with configurable batch size, idle
rate, and drop rate.

The workspace package `flyby-simulator` (`simulator/`) is a thin marker /
CLI stub. Prefer `SimulatedNetSource` for tests and demos.

## Why it exists

AF_XDP, DPDK, io_uring, and SPDK all require specific hardware, kernel
support, or root privileges. Development and CI should not.

## How it works

```rust,ignore
use flyby_net::{SimulatedNetSource, SimNetConfig, NetworkSource, RawBatch};
use flyby_core::Lifecycle;

let mut src = SimulatedNetSource::new(SimNetConfig::default());
src.init()?;
let mut batch = RawBatch::new(32, 2048);
let n = src.poll_batch(&mut batch)?;
```

## When not to use it

- For performance numbers quoted as production figures. The simulator is
  for correctness and relative comparison, not hardware validation.

## How to measure it

- Synthetic throughput (packets/s) with drop/idle rates at zero.
- Determinism of sequence numbers across runs.
