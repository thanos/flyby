# Release process

FlyBy releases are **source tags** of the workspace crates (currently
`0.x`). Every release must be reproducible from a tagged commit.

## SemVer

| Level | When |
|---|---|
| Patch | Bug fixes, docs, internal cleanups |
| Minor | Backward-compatible features |
| Major | Breaking API or architecture (requires ADR) |

## Checklist

1. **Portable CI green** on the commit to tag (`fmt`, `clippy`, `test`,
   `doc`, `mdbook`, MSRV, coverage dry-run / Coveralls upload,
   crates.io `--dry-run`).
2. **Examples** — `cargo build -p flyby-examples`.
3. **Docs** — mdBook and rustdoc reflect the release; getting-started still works.
4. **Changelog** — update [`CHANGELOG.md`](../../CHANGELOG.md) (`## [X.Y.Z] - YYYY-MM-DD`).
5. **Migration notes** — for breaking or behaviour-changing minors/majors.
6. **Benchmarks** — if performance claims changed, attach a short summary
   (machine, kernel, Criterion comparison vs previous tag). See
   [Benchmarks](./benchmarks.md).
7. **Hardware validation** — complete on self-hosted runners **or** record
   an explicit deferral in the release notes.
8. **Tag** — `git tag vX.Y.Z && git push origin vX.Y.Z`.
9. **CI Release workflow** — on `v*` tags, [`.github/workflows/release.yml`](../../.github/workflows/release.yml)
   creates a GitHub Release and publishes workspace crates to crates.io
   (requires `CARGO_REGISTRY_TOKEN` repository secret).
10. **Announce** — GitHub Release notes from the changelog section; confirm
    [crates.io/crates/flyby](https://crates.io/crates/flyby) and docs.rs.

## Published crates

Publish order (also used by CI dry-run and the release workflow):

1. `flyby-core`
2. `flyby-memory`
3. `flyby-net`
4. `flyby-storage`
5. `flyby` (facade)

Non-published workspace members: `flyby-examples`, `flyby-benches`,
`flyby-simulator`.

## Coverage

CI generates LCOV with `cargo llvm-cov` and uploads to
[Coveralls](https://coveralls.io/github/anomalyco/flyby) (public repos
can use `GITHUB_TOKEN`; enable the GitHub app / repo on Coveralls for a
live badge).

## Pre-release gates (summary)

```text
fmt + clippy + test + doc + mdbook + MSRV
+ coverage (Coveralls) + crates.io dry-run
        ↓
examples + tutorials + changelog
        ↓
optional: benches summary, hardware CI
        ↓
tag vX.Y.Z → GitHub Release + crates.io publish
```

## Related

- [Engineering standards](./engineering.md) · [Testing](./testing.md)
- [ADR-0011](./adr/0011-simulator-required-for-new-features.md) ·
  [ADR-0012](./adr/0012-benchmarks-are-part-of-the-api.md)
