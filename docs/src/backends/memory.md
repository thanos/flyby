# Shared memory backend

The shared-memory sink is the first production backend and the default
(enabled by the `memory` feature on the `flyby` facade).

## Status

**Implemented:** anonymous mmap SPSC ring, cache-line-aligned slots with
magic headers, `SharedMemorySink` write path, same-process `pop`.

**Planned:** named/file-backed multi-process regions, typed producer/
consumer split, MPSC.

## Why it exists

Ingested data is most useful when downstream readers can access it
without crossing a kernel boundary on every read. A shared-memory ring
buffer gives that, with a simple safety story for SPSC.

## How it works

```text
SharedMemorySink
  └── Region (anonymous mmap)
        ├── header + head/tail atomics (separate cache lines)
        └── Slot[i] = SlotHeader (32B) + payload + padding
```

Full ring returns `ErrorKind::BackPressure` (retryable), not a hard sink
failure. Oversized payloads are rejected.

## Where it fits

It is the default `Sink`, selected by `Placement` (or directly in demos).

## When not to use it

- You need durable storage. Use a file / io_uring / SPDK path instead.
- Your consumers are on a different host. Use a network sink.

## How to measure it

- Writes per second and bytes per second.
- Ring-buffer occupancy and back-pressure events.
- Tail latency on `write` (see `benches` package).
