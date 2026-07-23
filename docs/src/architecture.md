# Architecture overview

```text
           +---------------------+
           |      Source         |
           +----------+----------+
                      |
        +-------------+-------------+
        | Decode / Parse            |
        +-------------+-------------+
                      |
        +-------------+-------------+
        | Preprocess / Enrichment   |
        +-------------+-------------+
                      |
        +-------------+-------------+
        | Placement / Routing        |
        +-------------+-------------+
                      |
        +------+------+------+------+
        | Shared | Arrow | Kafka ...|
        | Memory | Flight| Future   |
        +--------+-------+----------+
```

## Workspace layout

```text
flyby/
├── Cargo.toml
├── crates/
│   ├── flyby/            # Public facade
│   ├── flyby-core/       # Traits, errors, lifecycle
│   ├── flyby-memory/     # Shared memory implementation
│   ├── flyby-net/        # AF_XDP, DPDK
│   └── flyby-storage/    # File, io_uring, SPDK
├── examples/
├── benches/
├── docs/
├── scenarios/            # FlyScenario DSL tutorial files (*.fly.toml)
├── articles/             # Medium reproduction catalog
└── simulator/            # First-class simulator (flyby-sim CLI)
```

The `flyby` crate re-exports the public API and hosts the Part VII
**runtime** (`flyby::runtime`): scheduling, back-pressure, and lifecycle
around [`Pipeline`](./concepts/pipeline.md). Users should normally write:

```rust
use flyby::prelude::*;
```

rather than depending directly on internal crates.

## Design rules

- Keep `flyby-core` platform independent.
- Isolate Linux-specific code.
- Isolate all `unsafe`.
- Every optimization must have a benchmark.
- Every public abstraction must have documentation and examples.

## Stage independence

Each stage must be independently testable:

```text
Raw Bytes
    |
Parser
    |
Typed Message
    |
PreProcessor
    |
Placement
    |
Sink
```

## Educational objectives

Every subsystem must teach:

- **why** it exists,
- **how** it works,
- **where** it fits,
- **when not** to use it,
- and **how** to measure it.

The documentation is considered part of the implementation. Start with the
[Simulator](./simulator.md) for the product overview and the
[FlyScenario DSL](./scenario-dsl.md) for declarative tutorials.
