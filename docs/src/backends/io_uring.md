# io_uring backend

`io_uring` is Linux's modern async I/O interface: a pair of shared
ring buffers between userspace and the kernel for submitting and
completing I/O without per-call syscalls.

## Why it exists

For file-backed ingest and durable sinks, `io_uring` delivers high
throughput and low syscall overhead, with batched submission and
completion.

## How it works

The real binding arrives with Part V of the specification. It is
Linux-only and gated behind the `io_uring` feature. All `unsafe` will
be isolated with a safety comment per block.

## Where it fits

A `Sink` (durable writes) and potentially a `Source` (replay from
file).

## When not to use it

- On non-Linux platforms (use the portable file backend).
- For tiny, synchronous writes where `std::fs` is simpler and the
  syscall overhead is irrelevant.

## How to measure it

- IOPS and bytes per second.
- Submission / completion queue depth.
- Completion latency (histogram) and `io_uring`-specific drops.
