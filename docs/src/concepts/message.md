# Message

A `Message` is the unit of work inside a FlyBy pipeline: a typed record
flowing from source to sink.

## Why it exists

The pipeline needs a single, stable notion of "a record" that every
stage can reason about without knowing the concrete payload type. The
`Message` trait provides that notion while keeping the payload itself
generic.

## How it works

Each message carries:

- a **schema identifier** ([`SchemaId`]) so decoders can dispatch
  without re-inspecting bytes,
- a **timestamp** ([`Timestamp`]) in nanoseconds since the epoch,
- **metadata** ([`Metadata`]) — a small, `Copy` struct with a sequence
  number and a suspect flag,
- optional **user extensions** on the concrete type.

The framework targets fixed-width messages first. Variable-length
payloads arrive once the fixed-width path is measured and stable.

## Where it fits

`Message` is produced by the decode stage and consumed by every
downstream stage: preprocessors transform it, placement routes it,
sinks write it.

## When not to use it

- You need to move raw, undecoded bytes end to end. In that case stay on
  the `Source` byte path and decode later.
- Your records are so tiny that per-record trait dispatch dominates.
  Consider batching in that case.

## How to measure it

- Per-decode latency (histogram).
- Decode error rate (counter, by [`SchemaId`]).
- End-to-end message residency time (timestamp at source vs. sink).

[`SchemaId`]: https://docs.rs/flyby-core/latest/flyby_core/trait.SchemaId.html
[`Timestamp`]: https://docs.rs/flyby-core/latest/flyby_core/struct.Timestamp.html
[`Metadata`]: https://docs.rs/flyby-core/latest/flyby_core/struct.Metadata.html
