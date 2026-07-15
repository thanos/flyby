# ADR-0005: File Reader Before io_uring

- **Status:** Accepted
- **Date:** 2026-07-14

## Context

The storage subsystem needs a read path that can be tested portably — on macOS,
Windows CI runners, and Linux development machines — before any Linux-specific
I/O optimisation is introduced.

Two candidates for the first storage backend:

- **Buffered file reader** (`std::fs::File` + `std::io::Read`): portable, no
  kernel version requirement, well-understood performance characteristics.
  Read latency is ~50–200 µs on an SSD under the page cache.
- **io_uring**: Linux 5.1+, not available on macOS or Windows.  Requires
  familiarity with submission/completion ring semantics.  Benchmarking io_uring
  against a correct buffered baseline is the *point* of adding it; without a
  baseline there is nothing to compare against.

## Decision

Implement the buffered `FileSource` first.

io_uring is added as a second backend after:

1. `FileSource` is stable and its throughput/latency is measured.
2. A benchmark exists that isolates the file-read cost from framing and parsing.
3. An io_uring-capable Linux host is available for measurement.

## Consequences

- `FileSource` works on every platform FlyBy targets for development.
- The replay engine, framing strategies, and `RawRecordBatch` are validated
  without requiring Linux.
- io_uring performance claims will be grounded in a measured comparison rather
  than theoretical improvement over buffered I/O.
- The `io_uring` Cargo feature already exists; enabling it returns
  `ErrorKind::FeatureNotEnabled` until the backend is implemented.

## Alternatives Considered

- **io_uring first**: rejected because it prevents development on non-Linux
  machines and makes it impossible to establish a meaningful benchmark baseline.
- **mmap-based reader**: considered but deferred — `mmap` has different
  performance characteristics for large files and adds complexity (page-fault
  driven I/O).  It is a candidate for a third backend after io_uring.
