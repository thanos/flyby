# Sink

A `Sink` is a terminal destination for decoded messages. Shared memory
is the first production sink, but the abstraction is generic so that
Arrow Flight, Kafka, files, or future transports can be added without
changing the pipeline shape.

## Why it exists

Terminal concerns (serialization, durability, transport framing) must
not leak back into the pipeline core. A `Sink` isolates them behind one
trait.

## How it works

A sink implements [`Sink`] (which extends [`Lifecycle`]) with a
`write` method and an optional `flush`. Sinks must respect
back-pressure: returning an error of kind `Sink` signals the pipeline to
slow down.

## Where it fits

It is the last stage, selected per message by [`Placement`].

## When not to use it

- You need fan-out with per-branch logic. Compose multiple sinks through
  `Placement` rather than building a "smart" sink.

## How to measure it

- Write latency (histogram) and throughput (records/s, bytes/s).
- Flush latency and buffering depth.
- Error rate by kind.

[`Sink`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Sink.html
[`Lifecycle`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Lifecycle.html
[`Placement`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Placement.html
