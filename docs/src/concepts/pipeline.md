# Pipeline

A `Pipeline` is the composition of a [`Source`], zero or more
[`PreProcessor`] steps, a [`Placement`] strategy, and one or more
[`Sink`]s. The [`Pipeline`] trait is the contract that the public facade
(`flyby::FlyBy`) drives.

## Why it exists

A single composition point lets the framework own back-pressure,
lifecycle, and metrics wiring, so individual stages do not have to.

## How it works

A pipeline implements [`Pipeline`] (which extends [`Lifecycle`]) with a
`step` method (the smallest unit of progress) and a `register_sink`
method. Sinks are registered before `init`. The builder API in `flyby`
constructs concrete pipelines; users rarely implement this trait by
hand.

## Where it fits

It is the top-level object the application holds and runs.

## When not to use it

- You want direct, ad-hoc composition of two stages for a test. It is
  fine to wire them by hand in that case.

## How to measure it

- Step latency and throughput.
- Back-pressure propagation time (source stall -> sink drain).
- Per-stage counters surfaced through [`MetricsCollector`].

