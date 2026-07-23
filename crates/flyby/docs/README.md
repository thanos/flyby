# flyby

Public facade of the FlyBy framework. Re-exports the stable API from
`flyby-core`, portable net/storage types, and the optional shared-memory
sink.

## Status

- Builder `.run()` validates feature selection (skeleton).
- `SimplePipeline` drives source → decode → preprocess → place → sink
  with configurable back-pressure and runtime metrics (Part VII).
- `flyby::runtime` — `RuntimeConfig` (TOML ≡ builder), schedulers
  (single-thread + worker pool), lifecycle driver, placement helpers.
- Builder `.run_demo()` builds a `SimplePipeline` (sim → memory) when
  the `memory` feature is enabled.
- Multi-sink type-state builder remains incremental work.

## Docs

- Project guide: [`docs/src/runtime.md`](../../../docs/src/runtime.md)
- ADR-009 / ADR-010: batch-oriented runtime; runtime independent of backends
