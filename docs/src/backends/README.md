# Backends

FlyBy backends implement the core traits for specific transports and
storage targets. The core (`flyby-core`) is platform independent and
contains no backend code; every backend is an adapter that plugs into
the stable trait model.

| Backend     | Trait target        | Feature     | Platform | Status            |
|-------------|---------------------|-------------|----------|-------------------|
| Shared mem  | `Sink`              | `memory`    | any      | **implemented**   |
| Net sim     | `NetworkSource`     | always*     | any      | **implemented**   |
| Product sim | full pipeline sim   | (workspace) | any      | **implemented**†  |
| File        | `StorageSource`     | always*     | any      | **implemented**   |
| AF_XDP      | `Source`            | `af_xdp`    | Linux    | stub              |
| DPDK        | `Source`            | `dpdk`      | Linux    | stub              |
| io_uring    | `StorageSource`     | `io_uring`  | Linux    | stub              |
| SPDK        | `StorageSource`     | `spdk`      | Linux    | stub              |

\* Portable file and net-sim APIs always compile via `flyby-storage` /
`flyby-net`; the facade re-exports them as `flyby::storage` /
`flyby::net`. The `simulator` feature only toggles a builder selector.

† The product simulator is the `flyby-simulator` workspace crate
(`flyby-sim` CLI, TUI, FlyScenario DSL). It shares the same source traits
as production backends — see [Simulator](../simulator.md) and
[FlyScenario DSL](../scenario-dsl.md).

Heavy dependencies are never enabled by default. Each backend page
explains **why** it exists, **how** it works, **where** it fits, **when
not** to use it, and **how** to measure it.
