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
