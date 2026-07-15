# flyby

Public facade of the FlyBy framework. Re-exports the stable API from
`flyby-core`, portable net/storage types, and the optional shared-memory
sink.

## Status

- Builder `.run()` validates feature selection (skeleton).
- `SimplePipeline` drives source → decode → preprocess → place → sink.
- Builder `.run_demo()` builds a `SimplePipeline` (sim → memory) when
  the `memory` feature is enabled.
- Multi-sink registration and type-state builder remain incremental work.
