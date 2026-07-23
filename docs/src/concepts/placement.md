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

Between [`PreProcessor`] and [`Sink`]. Concrete helpers live in
`flyby::pipeline`: `FixedPlacement`, `RoundRobinPlacement`, `HashPlacement`,
`CallbackPlacement`, `schema_hash_placement`.

## When not to use it

- You have a single sink. A trivial "always route to sink 1" placement
  is fine, but don't grow routing logic inside a sink that should be
  here instead.

## How to measure it

- Route decision latency (should be negligible).
- Per-sink fan-out distribution (counter by [`SinkId`]).
- Drop rate.

