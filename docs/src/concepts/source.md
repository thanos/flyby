# Source

A `Source` sits at the head of the pipeline. It acquires raw data (from
a socket, a file, shared memory, a simulator, ...) and hands it to the
pipeline as opaque byte slices.

## Why it exists

Decoupling acquisition from decoding lets the same decoder run against
any source: a live AF_XDP socket, a pcap replay, or an in-process
simulator.

## How it works

A source implements [`Source`] (which extends [`Lifecycle`]). The
pipeline calls `poll` repeatedly; the source returns `Ok(Some(bytes))`
when data is ready, `Ok(None)` when temporarily exhausted, and `Err` on
genuine failure.

Sources are **back-pressure aware**: when the pipeline cannot accept
more work, the source should slow down rather than drop data, unless
explicitly configured otherwise.

## Where it fits

It is the first stage. Its output feeds the parser / decode step.

## When not to use it

- You already have decoded records in memory (e.g. from another
  pipeline). Feed them straight into a `Sink` or `PreProcessor` instead.

## How to measure it

- Poll latency and `None` rate (how often the source is idle).
- Bytes per second and records per second.
- Back-pressure stalls (time spent unable to push downstream).

