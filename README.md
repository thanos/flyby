# FlyBy

A high-performance Rust framework for building composable
data-ingestion pipelines.

```text
Source -> Decode -> Transform -> Route -> Sink
```

Shared memory is the first production sink, but it is **not** the
defining abstraction of the project. FlyBy aims to make advanced Linux
I/O technologies (AF_XDP, io_uring, DPDK, SPDK) approachable while
preserving high performance, safety, and excellent documentation.

## Status

| Area | Status |
|------|--------|
| `flyby-core` traits / errors | Implemented (contract layer) |
| Shared-memory SPSC sink | **Implemented** |
| Network simulator (`flyby-net`) | **Implemented** |
| Product simulator (`flyby-simulator`) | **Implemented** (CLI, TUI, FlyScenario DSL) |
| File source + framing + replay engine | **Implemented** |
| Facade builder `.run()` | Skeleton (config validation) |
| `SimplePipeline` | **Implemented** (source→decode→place→sink) |
| Facade `run_demo()` | Builds `SimplePipeline` (sim → memory) |
| AF_XDP / DPDK / io_uring / SPDK | Stubs (`NotImplemented`) |

## Workspace layout

```text
flyby/
├── Cargo.toml
├── crates/
│   ├── flyby/            # Public facade + builder
│   ├── flyby-core/       # Traits, errors, lifecycle (platform independent)
│   ├── flyby-memory/     # Shared-memory sink (default backend)
│   ├── flyby-net/        # Simulator + AF_XDP/DPDK stubs
│   └── flyby-storage/    # File source + io_uring/SPDK stubs
├── examples/             # Runnable examples
├── benches/              # Criterion benchmarks
├── simulator/            # First-class simulator (flyby-sim CLI + TUI + DSL)
├── scenarios/            # FlyScenario tutorial files (*.fly.toml)
├── articles/             # Medium reproduction catalog
├── docs/                 # Project guide (mdBook) + ADRs
├── .github/workflows/    # CI: fmt, clippy, test, doc, mdbook
├── Dockerfile            # Linux dev container
└── .devcontainer/        # VS Code / Codespaces config
```

## Quick start

```sh
cargo build --workspace
cargo run -p flyby-examples --bin hello_pipeline

# Product simulator (no privileged networking required)
cargo run -p flyby-simulator --bin flyby-sim -- tui constant_rate
cargo run -p flyby-simulator --bin flyby-sim -- run scenarios/constant_rate.fly.toml
```

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

## Feature flags

| Feature      | Default | Backend                            |
|--------------|---------|------------------------------------|
| `memory`     | yes     | In-process shared-memory sink.     |
| `af_xdp`     | no      | AF_XDP source stub (Linux).        |
| `dpdk`       | no      | DPDK source stub.                  |
| `io_uring`   | no      | io_uring storage stub.             |
| `spdk`       | no      | SPDK storage stub.                 |
| `simulator`  | no      | Builder selector for net simulator.|
| `benchmarks` | no      | Reserved (benches package always builds). |

Portable file + net-sim APIs always compile. Heavy stubs are opt-in.

## Developer container

```sh
docker build -t flyby-dev -f Dockerfile .
docker run --rm -it -v "$PWD":/workspace -w /workspace flyby-dev
```

VS Code / Codespaces users can open the repo in the configured
`.devcontainer/`.

## Documentation

- **Project guide:** `mdbook serve docs/ --open` (see [`docs/README.md`](docs/README.md))
- **Simulator:** [`docs/src/simulator.md`](docs/src/simulator.md) ·
  [FlyScenario DSL](docs/src/scenario-dsl.md)
- **API reference:** `cargo doc --workspace --open`
- **Architecture decisions:** [`docs/src/adr/`](docs/src/adr/)

## License

Dual-licensed under MIT or Apache-2.0, at your option.
