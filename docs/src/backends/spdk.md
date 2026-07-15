# SPDK backend

## Status

**Stub.** The `SpdkSource` type compiles behind the `spdk` feature and
returns `ErrorKind::NotImplemented` until the binding lands
(ADR-0006: io_uring before SPDK).

## Role

SPDK is planned as a **storage source** for userspace NVMe access, not a
sink. Same ingest path as file/io_uring: framed records → decoder →
pipeline.

## Why it exists

Bypass the kernel block stack for maximum NVMe throughput when the
operational cost is justified.

## When not to use it

- Prefer `FileSource` / io_uring until measured need exists.
- Environments without hugepages / VFIO binding.

## How to measure it

- Compare against io_uring on the same device.
- Track poller CPU and queue-depth saturation.
