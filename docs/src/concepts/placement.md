# Placement

`Placement` decides **where** a message goes after preprocessing. The
simplest placement is a fixed mapping from schema id to sink; more
sophisticated placements consider load, locality, and affinity.

## Why it exists

Routing logic is separated from sinks so it can be tested in isolation,
without a live backend, and so that sinks stay single-purpose.

## How it works

A placement implements [`Placement`] with a `route` method that maps a
message to a [`SinkId`]. Returning [`SinkId::NONE`] drops the message.

## Where it fits

Between [`PreProcessor`] and [`Sink`].

## When not to use it

- You have a single sink. A trivial "always route to sink 1" placement
  is fine, but don't grow routing logic inside a sink that should be
  here instead.

## How to measure it

- Route decision latency (should be negligible).
- Per-sink fan-out distribution (counter by [`SinkId`]).
- Drop rate.

[`Placement`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Placement.html
[`SinkId`]: https://docs.rs/flyby-core/latest/flyby_core/struct.SinkId.html
[`SinkId::NONE`]: https://docs.rs/flyby-core/latest/flyby_core/struct.SinkId.html#associatedconstant.NONE
[`PreProcessor`]: https://docs.rs/flyby-core/latest/flyby_core/trait.PreProcessor.html
[`Sink`]: https://docs.rs/flyby-core/latest/flyby_core/trait.Sink.html
