# ADR-008: The Simulator Is a Product Feature

**Status**: Accepted  
**Date**: 2026-07-14

## Context

Simulators are often treated as internal test tooling — a means to an end,
not something that ships.  They are kept in `tests/` directories, have no
public API, and are discarded once real hardware is available.

FlyBy's simulator could follow that pattern.  Alternatively, it could be
treated as a first-class, user-facing product feature shipped alongside the
pipeline library.

## Decision

Treat the FlyBy simulator as a product feature, not internal test plumbing:

1. **Public API**: all simulator types are `pub` and documented to the same
   standard as the pipeline types.  Users can build custom scenarios, register
   custom NICs via `DynNic`, and plug in their own `EventSink`.

2. **Binary**: the `flyby-sim` CLI (`flyby-simulator` package) runs named
   built-in scenarios (`flyby-sim constant_rate`) and FlyScenario DSL files
   (`flyby-sim run scenarios/….fly.toml`) without writing Rust.

3. **Scenario versioning**: built-in scenarios and the FlyScenario DSL IR are
   part of the public surface. Changes to scenario defaults or breaking DSL
   fields follow semantic versioning.

4. **Integration tests**: the simulator's integration test suite (`tests/scenarios.rs`)
   is part of the CI gate, not an afterthought.

5. **Colocation with production code**: `flyby-simulator` is a workspace crate
   at the same level as `flyby-net` and `flyby-storage`, not nested under
   `tests/`.

## Consequences

### Benefits

- Users can reproduce production traffic patterns in their own test suites,
  giving them confidence that their decoder, router, and sink logic works
  under realistic load.
- The simulator's public API forces trait-boundary discipline: if a type only
  works with the simulator, it will fail with a real backend.  Making the
  simulator first-class surfaces these mismatches early.
- Pipeline benchmarks can be run locally with zero hardware setup.
- The CLI binary makes the simulator discoverable and usable by non-Rust
  tooling (shell scripts, CI, notebooks).

### Trade-offs

- The public API commitment means we cannot make breaking changes to
  `Scenario`, `VirtualNic`, or `EventSink` without a semver bump.
- Documentation and changelog maintenance cost increases.
- The simulator's binary adds build time to the workspace.

## Alternatives Considered

**Internal-only simulator**: keep all simulator types `pub(crate)` and delete
the binary.  Rejected because it prevents users from reproducing production
workloads in their own code, and it makes the simulator invisible to anyone
not reading source.

**Separate `flyby-simulator-test` crate**: publish the simulator as a
`[dev-dependencies]`-only crate.  Rejected because `dev-dependencies` cannot
be re-exported or used in downstream binaries, limiting the simulator's reach.
