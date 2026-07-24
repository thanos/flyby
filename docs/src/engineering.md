# Engineering standards

These standards are part of FlyBy's architecture (Part VIII). Every
contribution is expected to meet them.

## Principles

1. Correctness before performance.
2. Performance before cleverness.
3. Measure, never assume ([ADR-0012](./adr/0012-benchmarks-are-part-of-the-api.md)).
4. Documentation is a feature.
5. Unsafe Rust is a last resort.
6. Every abstraction must justify its existence.

## Rust standards

| Rule | Policy |
|---|---|
| Edition | Rust **2024** (workspace) |
| MSRV | **1.85** (`rust-version` in root `Cargo.toml`) |
| Toolchains in CI | **stable** and **MSRV** |
| Nightly | Avoided unless behind an explicit feature |

Document MSRV bumps in the changelog. Raising MSRV is a **minor** semver
change while the crate is `0.x` only if announced; treat it as breaking
for `1.x+`.

## Dependency policy

Dependencies must be actively maintained, documented, appropriately
licensed, and justified. Prefer:

- workspace-shared versions,
- small crates over kitchen-sink frameworks,
- optional features for heavy backends.

Avoid unnecessary transitive weight. License/advisory scanning may be
added via `cargo deny` as the tree grows.

## Unsafe policy

Every `unsafe` block must document:

1. **Safety rationale** — why `unsafe` is required.
2. **Invariants** — what must remain true.
3. **Preconditions** — what callers/builders guarantee.
4. **UB risks** — what goes wrong if invariants break.

Isolate `unsafe` behind safe APIs. Prefer `#![forbid(unsafe_code)]` on
crates that do not need it (core, facade, net, storage, simulator).
Shared-memory code in `flyby-memory` is the primary exception and must
keep `# Safety` / `// SAFETY:` comments current.

## Documentation standards

Every public item needs rustdoc. Prefer a short example where non-obvious.
Every subsystem should eventually have:

- architecture overview,
- tutorial / getting-started path,
- troubleshooting notes,
- benchmark notes (when performance-sensitive).

Project guide: `mdbook serve docs/ --open`. API: `cargo doc --workspace --open`.

## Quality gates

### Before merge

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace` (and `--all-features` when touching backends)
- [ ] Docs updated (rustdoc and/or mdBook) when behaviour changes
- [ ] ADR opened for public API / architecture breaks
- [ ] Simulator or portable test for new pipeline features ([ADR-0011](./adr/0011-simulator-required-for-new-features.md))
- [ ] Benchmark evidence for hot-path changes ([ADR-0012](./adr/0012-benchmarks-are-part-of-the-api.md))

### Before release

- [ ] Portable CI green on the release tag
- [ ] Examples compile (`cargo build -p flyby-examples`)
- [ ] Tutorial / getting-started still accurate
- [ ] Changelog + migration notes prepared
- [ ] Benchmark summary attached when performance changed
- [ ] Hardware validation complete **or** explicitly deferred with rationale
- [ ] Source tagged (`vX.Y.Z`)

See [Release process](./release.md).

## Semantic versioning

| Level | Meaning |
|---|---|
| Patch | Fixes, docs, non-API internal cleanups |
| Minor | Backward-compatible features |
| Major | Breaking API or architecture changes |

Breaking changes require an ADR ([ADR-0001](./adr/0001-record-architecture-decisions.md)).

## Related

- [Testing](./testing.md) · [Benchmarks](./benchmarks.md) · [Release](./release.md)
- [Contributing](../../CONTRIBUTING.md)
