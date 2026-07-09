# PreProcessor

A `PreProcessor` runs after decoding and before placement. It is the
natural home for normalization, enrichment, filtering, and any
CPU-bound transform that does not depend on routing decisions.

## Why it exists

Keeping transforms out of sources and sinks lets them be unit-tested in
isolation and reordered without touching I/O code.

## How it works

A preprocessor implements [`PreProcessor`] with a single `process`
method that takes a message and returns either a transformed message, a
drop decision (`Ok(None)`), or an error. Preprocessors must not perform
I/O; that belongs in a source or sink.

## Where it fits

Between the parser and [`Placement`].

## When not to use it

- The transform depends on the chosen sink (e.g. sink-specific framing).
  That belongs in the sink.
- The transform is genuinely I/O-bound (e.g. an enrichment lookup that
  hits a network). That should be a stage of its own, not a
  preprocessor.

## How to measure it

- Per-call latency (histogram).
- Drop rate (counter).
- CPU cost relative to decode and write.

[`PreProcessor`]: https://docs.rs/flyby-core/latest/flyby_core/trait.PreProcessor.html
[`Placement`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Placement.html
