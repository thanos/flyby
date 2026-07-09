# Simulator

Simulation is a first-class development workflow (design principle #5):
the simulator lets a pipeline run end to end without any hardware or
kernel dependencies.

## Why it exists

AF_XDP, DPDK, io_uring, and SPDK all require specific hardware, kernel
support, or root privileges. Development and CI should not. The
simulator provides an in-process `Source` that replays or synthesizes
data so the rest of the pipeline can be built and measured on any
machine.

## How it works

The concrete replay format, synthetic source, and clock model arrive
with Part VI of the specification. The `simulator/` package already
links against `flyby-core` and provides a runnable binary skeleton:

```sh
cargo run -p flyby-simulator
```

## Where it fits

An alternative `Source` at the head of the pipeline, used during
development, testing, and CI.

## When not to use it

- For performance numbers that will be quoted as production figures.
  The simulator is for correctness and relative comparison, not
  hardware validation (design principle #6).

## How to measure it

- Replay throughput (records/s) and clock skew vs. wall clock.
- Determinism: identical inputs must produce identical timings across
  runs.
