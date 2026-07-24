# ADR-012: Benchmarks Are Part of the API

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

FlyBy is a performance-oriented ingestion framework. Silent throughput or
latency regressions are as harmful as functional bugs, yet they are easy to
merge when PRs only show green unit tests.

## Decision

Treat **performance-sensitive changes as API-impacting**. Such changes
require benchmark evidence (Criterion harness in `benches/`, or an
equivalent documented measurement) before merge. Performance regressions
are reviewed like correctness regressions. See [Benchmarks](../benchmarks.md).

## Consequences

### Positive

- Optimisations must be measured, not assumed.
- Reviewers have a concrete artifact to inspect.
- Aligns with “measure, never assume” and design principle #4.

### Negative

- Slightly higher PR cost for hot-path changes.
- Absolute numbers remain machine-dependent; comparisons are relative.

## Alternatives considered

**Benches optional forever:** rejected — regressions accumulate unnoticed.

**Mandatory CI bench regression store:** deferred — useful later; Criterion
local comparison + PR evidence is enough for Part VIII.
