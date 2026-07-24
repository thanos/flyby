# Contributing to FlyBy

Thanks for contributing. Engineering standards are part of the
architecture — please read them before opening a PR.

## Quick start

```sh
git clone https://github.com/anomalyco/flyby
cd flyby
cargo build --workspace
cargo test --workspace
```

See [Getting started](docs/src/getting-started.md).

## Before you push

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --all-features --no-deps
mdbook build docs   # if you changed the project guide
```

## What we expect in a PR

1. **Correctness** — tests for the behaviour you changed.
2. **Portable first** — new pipeline features need a simulator or
   in-memory/file test ([ADR-0011](docs/src/adr/0011-simulator-required-for-new-features.md)).
3. **Docs** — rustdoc for public items; update mdBook pages when behaviour
   or workflows change.
4. **ADRs** — required for public API or architecture breaks
   ([ADR-0001](docs/src/adr/0001-record-architecture-decisions.md)).
5. **Benchmarks** — evidence for hot-path / optimisation claims
   ([ADR-0012](docs/src/adr/0012-benchmarks-are-part-of-the-api.md)).

## Handbook

| Topic | Doc |
|---|---|
| Engineering standards | [docs/src/engineering.md](docs/src/engineering.md) |
| Testing pyramid | [docs/src/testing.md](docs/src/testing.md) |
| Benchmarks | [docs/src/benchmarks.md](docs/src/benchmarks.md) |
| Release checklist | [docs/src/release.md](docs/src/release.md) |
| Unsafe / MSRV / SemVer | [engineering.md](docs/src/engineering.md) |
| Security reports | [SECURITY.md](SECURITY.md) |

## Code of conduct

Be respectful. Assume good intent. Prefer technical disagreement over
personal criticism. (A fuller CoC may land with project governance.)

## License

By contributing, you agree that your contributions are dual-licensed
under MIT OR Apache-2.0, the same as the project.
