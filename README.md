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

Early-stage skeleton. The workspace, CI, dev container, documentation
structure, and core trait model are in place. Concrete backend
implementations arrive in subsequent parts of the specification.

## Workspace layout

```text
flyby/
├── Cargo.toml
├── crates/
│   ├── flyby/            # Public facade + builder
│   ├── flyby-core/       # Traits, errors, lifecycle (platform independent)
│   ├── flyby-memory/     # Shared-memory sink (default backend)
│   ├── flyby-net/        # AF_XDP, DPDK
│   └── flyby-storage/    # File, io_uring, SPDK
├── examples/             # Runnable examples
├── benches/              # Criterion benchmarks
├── simulator/            # In-process simulator source
├── docs/                 # Project guide (mdBook) + ADRs
├── .github/workflows/    # CI: fmt, clippy, test, doc, mdbook
├── Dockerfile            # Linux dev container
└── .devcontainer/        # VS Code / Codespaces config
```

## Quick start

```sh
cargo build --workspace
cargo run -p flyby-examples --bin hello_pipeline
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
| `af_xdp`     | no      | AF_XDP source (Linux eBPF / XSK).  |
| `dpdk`       | no      | DPDK source.                       |
| `io_uring`   | no      | io_uring storage backend.          |
| `spdk`       | no      | SPDK storage backend.              |
| `simulator`  | no      | In-process simulator source.       |
| `benchmarks` | no      | Build the benchmark harnesses.     |

Heavy dependencies are never enabled by default.

## Developer container

```sh
docker build -t flyby-dev -f Dockerfile .
docker run --rm -it -v "$PWD":/workspace -w /workspace flyby-dev
```

VS Code / Codespaces users can open the repo in the configured
`.devcontainer/`.

## Documentation

- **Project guide:** `mdbook serve docs/ --open` (see [`docs/README.md`](docs/README.md))
- **API reference:** `cargo doc --workspace --open`
- **Architecture decisions:** [`docs/src/adr/`](docs/src/adr/)

## License

Dual-licensed under MIT or Apache-2.0, at your option.
