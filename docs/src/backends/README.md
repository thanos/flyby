# Backends

FlyBy backends implement the core traits for specific transports and
storage targets. The core (`flyby-core`) is platform independent and
contains no backend code; every backend is an adapter that plugs into
the stable trait model.

| Backend     | Trait target        | Feature     | Platform | Status       |
|-------------|---------------------|-------------|----------|--------------|
| Shared mem  | `Sink`              | `memory`    | any      | skeleton     |
| AF_XDP      | `Source`            | `af_xdp`    | Linux    | planned      |
| DPDK        | `Source`            | `dpdk`      | Linux    | planned      |
| io_uring    | `Sink`              | `io_uring`  | Linux    | planned      |
| SPDK        | `Sink`              | `spdk`      | Linux    | planned      |
| Simulator   | `Source`            | `simulator` | any      | planned      |

Heavy dependencies are never enabled by default. Each backend page
explains **why** it exists, **how** it works, **where** it fits, **when
not** to use it, and **how** to measure it.
