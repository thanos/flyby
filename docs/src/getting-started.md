# Getting started

## Prerequisites

- Rust **1.95** or newer (workspace MSRV; CI also runs on **stable**).
- A Linux host for the AF_XDP / io_uring / DPDK / SPDK backends. The
  shared-memory and simulator backends build anywhere Rust does.
- (Optional) Docker for the [dev container](../README.md).

Contributor checklist: [Engineering standards](./engineering.md) ·
[Contributing](./contributing.md).

## Build

```sh
git clone https://github.com/anomalyco/flyby
cd flyby
cargo build --workspace
```

The default feature set enables only the in-process `memory` backend, so
the first build stays small. Heavy backends are opt-in:

```sh
cargo build --workspace --features af_xdp,io_uring
```

## Run the example

```sh
cargo run -p flyby-examples --bin hello_pipeline
```

## Simulator

Interactive Ratatui dashboard (clock, queues, events, sparklines):

```sh
cargo run -p flyby-simulator --bin flyby-sim -- tui constant_rate
```

Keys: `Space` run/pause · `s` step · `+/-` speed · `r` restart · `q` quit.

Headless built-in or FlyScenario DSL file:

```sh
cargo run -p flyby-simulator --bin flyby-sim -- constant_rate
cargo run -p flyby-simulator --bin flyby-sim -- run scenarios/constant_rate.fly.toml
```

See [Simulator](./simulator.md) (TUI screenshots, components) and
[FlyScenario DSL](./scenario-dsl.md) (TOML + Rhai reference).

## Checks

The project enforces `cargo fmt` and `clippy -D warnings`:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Coverage (`cargo llvm-cov`): see [Testing → Coverage](./testing.md#coverage).

Runtime (scheduling / back-pressure) docs: [Runtime](./runtime.md).

## Feature flags

| Feature      | Default | Backend                                     |
|--------------|---------|---------------------------------------------|
| `memory`     | yes     | In-process shared-memory sink.              |
| `af_xdp`     | no      | AF_XDP source (Linux eBPF / XSK).           |
| `dpdk`       | no      | DPDK source.                                |
| `io_uring`   | no      | io_uring storage backend.                   |
| `spdk`       | no      | SPDK storage backend.                       |
| `simulator`  | no      | In-process simulator source.               |
| `benchmarks` | no      | Build the benchmark harnesses.              |

Heavy dependencies are never enabled by default.

## Developer container

A ready-made Linux image with the toolchain and backend system
dependencies is provided:

```sh
docker build -t flyby-dev -f Dockerfile .
docker run --rm -it -v "$PWD":/workspace -w /workspace flyby-dev
```

VS Code / Codespaces users can open the repo in the configured
`.devcontainer/` instead.
