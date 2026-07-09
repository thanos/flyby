# Introduction

FlyBy is a high-performance Rust framework for building composable
data-ingestion pipelines.

Its primary abstraction is:

```text
Source -> Decode -> Transform -> Route -> Sink
```

Shared memory is the first production sink, but it is **not** the
defining abstraction of the project.

## Why FlyBy?

- Build a reusable systems framework rather than a single application.
- Teach modern Linux systems programming through implementation.
- Provide clean abstractions over AF_XDP, io_uring, DPDK and SPDK.
- Prefer measurable performance over theoretical optimisation.
- Keep unsafe Rust isolated and well documented.
- Make simulation a first-class development workflow.

## Design principles

1. Safe Rust first.
2. Unsafe is isolated.
3. Zero-copy first, not zero-copy at all costs.
4. Benchmark every optimisation.
5. Simulator before hardware.
6. Hardware validation before release.
7. Portable core, platform-specific adapters.
8. Excellent documentation is a feature.
9. APIs should outlive implementations.
10. Simplicity before cleverness.

## Non-goals

- Replacing the Linux kernel.
- Claiming "zero-copy" unless demonstrably true.
- Hiding operational complexity.
- Building every backend before the architecture is stable.

## Where to go next

- [Getting started](./getting-started.md) for a first running pipeline.
- [Architecture overview](./architecture.md) for the system shape.
- [Concepts](./concepts/README.md) for one page per core abstraction.
- The API reference: `cargo doc --workspace --open`.
