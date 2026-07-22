# Summary

[Introduction](./introduction.md)

# Getting started

- [Getting started](./getting-started.md)

# Architecture

- [Architecture overview](./architecture.md)

# Concepts

- [Concepts](./concepts/README.md)
- [Message](./concepts/message.md)
- [Source](./concepts/source.md)
- [Decoder](./concepts/decoder.md)
- [Encode](./concepts/encode.md)
- [Sink](./concepts/sink.md)
- [PreProcessor](./concepts/preprocessor.md)
- [Placement](./concepts/placement.md)
- [Pipeline](./concepts/pipeline.md)
- [Metrics](./concepts/metrics.md)

# Backends

- [Backends](./backends/README.md)
- [Shared memory](./backends/memory.md)
- [AF_XDP](./backends/af_xdp.md)
- [DPDK](./backends/dpdk.md)
- [io_uring](./backends/io_uring.md)
- [SPDK](./backends/spdk.md)

# Workflows

- [Simulator](./simulator.md)
- [Benchmarks](./benchmarks.md)

# Decisions

- [Architecture Decision Records](./adr/README.md)
  - [ADR-0001: Record architecture decisions](./adr/0001-record-architecture-decisions.md)
  - [ADR-0002: AF_XDP before DPDK](./adr/0002-af-xdp-before-dpdk.md)
  - [ADR-0003: eBPF is an implementation detail](./adr/0003-ebpf-is-implementation-detail.md)
  - [ADR-0004: Copy mode before zero-copy](./adr/0004-copy-mode-before-zero-copy.md)
  - [ADR-0005: File reader before io_uring](./adr/0005-file-before-io-uring.md)
  - [ADR-0006: io_uring before SPDK](./adr/0006-io-uring-before-spdk.md)
  - [ADR-0007: Simulator before hardware](./adr/0007-simulator-before-hardware.md)
  - [ADR-0008: Simulator is a product feature](./adr/0008-simulator-is-a-product-feature.md)
