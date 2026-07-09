# SPDK backend

SPDK (Storage Performance Development Kit) is a userspace storage
toolkit that polls NVMe devices directly, bypassing the kernel block
layer.

## Why it exists

For the lowest-latency, highest-IOPS durable path, kernel-bypass NVMe
via SPDK is the ceiling, at the cost of dedicating cores and the device
to userspace.

## How it works

The real binding arrives with Part V of the specification. It requires
an external SPDK installation and is gated behind the `spdk` feature.
All `unsafe` will be isolated with a safety comment per block.

## Where it fits

A `Sink` for ultra-low-latency durable writes.

## When not to use it

- You cannot dedicate a core to busy-polling. `io_uring` is the better
  fit.
- You need the device shared with the kernel block layer.
- You are not on NVMe. SPDK is NVMe-centric.

## How to measure it

- IOPS and bytes per second per queue pair.
- Submission / completion latency (histogram).
- Core busy-poll efficiency (useful work vs. empty polls).
