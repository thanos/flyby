# Shared memory backend

The shared-memory sink is the first production backend and the default
(enabled by the `memory` feature on the `flyby` facade).

## Why it exists

Ingested data is most useful when downstream readers (other processes,
analytic engines) can access it without crossing a kernel boundary on
every read. A shared-memory ring buffer gives that, with a simple
safety story.

## How it works

The concrete ring-buffer and slot-layout implementation arrives with
Part III of the specification. The current `flyby-memory` crate exposes
a stub sink so the pipeline can be wired up and measured end to end
today.

## Where it fits

It is the default `Sink`, selected by `Placement`.

## When not to use it

- You need durable storage. Use a file / io_uring / SPDK sink instead.
- Your consumers are on a different host. Use a network sink (Arrow
  Flight, Kafka, ...).

## How to measure it

- Writes per second and bytes per second.
- Ring-buffer occupancy and back-pressure events.
- Tail latency on `write`.
