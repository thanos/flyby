# ADR-009: Batch-Oriented Runtime

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

Pipeline stages can be driven one message at a time or in batches. Kernel-bypass
sources (AF_XDP, DPDK), storage pollers (io_uring), and the simulator already
expose batch APIs. A message-at-a-time runtime would add function-call and
cache overhead on the hot path and fight those backends.

## Decision

Operate primarily on **batches** rather than individual messages. The runtime
and `SimplePipeline` poll sources in batches, decode/preprocess/place over the
batch where practical, and expose configurable `batch_size`. Per-message APIs
remain available for correctness and educational use.

## Consequences

### Positive

- Fewer cross-stage calls and better cache behaviour.
- Aligns with AF_XDP, DPDK, io_uring, and replay engines.
- Throughput-oriented defaults; latency tuned via smaller batches.

### Negative

- Slightly more complex back-pressure handling inside a batch.
- Latency-sensitive demos must shrink `batch_size` explicitly.

## Alternatives considered

**Message-at-a-time only:** rejected — fights every high-performance backend.

**Backend-specific batch loops:** rejected — violates ADR-010 (one runtime).
