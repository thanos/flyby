# flyby

Public facade of the FlyBy framework. Re-exports the stable API from
`flyby-core`, portable net/storage types, and the optional shared-memory
sink.

## Status

- Builder `.run()` validates feature selection (skeleton).
- Builder `.run_demo()` exercises simulator → decoder → memory sink when
  the `memory` feature is enabled.
- Full multi-stage pipeline composition (placement, multi-sink) is WIP.
