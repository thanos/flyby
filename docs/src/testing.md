# Testing

FlyBy uses a layered testing pyramid. Prefer portable tests; escalate to
Linux and hardware only when the behaviour cannot be exercised otherwise
([ADR-0011](./adr/0011-simulator-required-for-new-features.md)).

## Pyramid

```text
            ┌──────────────┐
            │   Hardware   │  AF_XDP / DPDK / SPDK / latency labs
            │  validation  │  (self-hosted / on-demand)
            └──────▲───────┘
            ┌──────┴───────┐
            │ Linux / priv │  mmap, io_uring, AF_XDP *build* & smoke
            │   (CI stub)  │  (self-hosted when available)
            └──────▲───────┘
            ┌──────┴───────┐
            │  Simulator   │  scenarios, faults, DSL, scheduling
            │  + runtime   │  `simulator/tests`, `flyby` runtime tests
            └──────▲───────┘
            ┌──────┴───────┐
            │ Unit /       │  `#[cfg(test)]`, crate `tests/`
            │ property /   │  framing, slots, placement, parsers
            │ parser       │
            └──────────────┘
```

## Portable (GitHub-hosted)

| Kind | Where |
|---|---|
| Unit | `crates/*/src/**` modules |
| Integration | `crates/*/tests/`, `simulator/tests/` |
| Runtime | `crates/flyby/tests/runtime.rs` |
| Simulator scenarios | `simulator/tests/scenarios.rs` |
| FlyScenario DSL | `simulator/tests/dsl.rs` |
| Examples compile | `cargo build -p flyby-examples` |

Run:

```sh
cargo test --workspace
cargo test --workspace --all-features
```

## Coverage

CI generates LCOV with [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)
and uploads it to Coveralls (see
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)).

Install once:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

Match CI (writes `lcov.info`):

```sh
cargo llvm-cov --workspace --all-features --lcov \
  --output-path lcov.info \
  --ignore-filename-regex '(examples/|benches/|simulator/src/main\.rs)'
```

Useful local variants:

```sh
# Terminal summary
cargo llvm-cov --workspace --all-features

# HTML report (opens target/llvm-cov/html/index.html)
cargo llvm-cov --workspace --all-features --html --open
```

Coveralls upload only runs in GitHub Actions. Locally, inspect the
terminal summary, HTML report, or `lcov.info`.

## Linux / privileged

Feature stubs must keep compiling under `--all-features`. Privileged
smoke (real AF_XDP / io_uring) is defined in
[`.github/workflows/hardware.yml`](../../.github/workflows/hardware.yml)
and runs only on self-hosted runners when available.

## Hardware

Hardware latency and backend validation is **on demand / nightly /
release-branch**, never a required PR gate. See [Release](./release.md)
and [Benchmarks](./benchmarks.md).

## Conventions

- Prefer deterministic seeds (simulator faults, DSL).
- Use `NullCollector` when measuring or when metrics would dominate cost.
- New pipeline behaviour needs a portable test before hardware-only paths.
- Property / fuzz suites are welcome for framing, encode/decode, and DSL
  parsers; they are not yet mandatory.

## Related

- [Engineering standards](./engineering.md)
- [Simulator](./simulator.md) · [Runtime](./runtime.md)
