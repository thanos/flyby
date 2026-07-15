# io_uring backend

## Status

**Stub.** The `IoUringSource` type compiles behind the `io_uring` feature
and returns `ErrorKind::NotImplemented` until the binding lands
(ADR-0005: file reader first).

## Role

io_uring is planned as a **storage source** (high-throughput file/NVMe
ingest), not a sink. Durable write sinks may be added later as separate
types.

## Why it exists

Batch async I/O with low syscall overhead for sequential and random
reads on modern Linux.

## When not to use it

- Portable or early development: use `FileSource`.
- Kernels older than 5.1, or non-Linux hosts.

## How to measure it

- IOPS and bandwidth vs `FileSource`.
- Completion latency histograms at various queue depths.
