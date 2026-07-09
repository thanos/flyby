# DPDK backend

DPDK is a userspace networking toolkit that bypasses the kernel
entirely, polling NIC queues from userspace with hugepages and
VFIO-managed devices.

## Why it exists

For the highest packet rates and the tightest latency control, kernel
bypass via busy-polling beats interrupt-driven sockets. DPDK is the
mature choice in that regime.

## How it works

The real binding arrives with Part IV of the specification. It requires
an external DPDK installation and is gated behind the `dpdk` feature.
All `unsafe` will be isolated with a safety comment per block.

## Where it fits

A `Source` at the head of the pipeline.

## When not to use it

- You cannot dedicate cores to busy-polling. AF_XDP is the better fit.
- You want to stay in pure Linux without hugepages / VFIO setup.
- Your workload is not latency-critical at the microsecond level.

## How to measure it

- Packets per second and bytes per second per core.
- Poll-cycle cost and empty-poll ratio.
- Hugepage utilization and cache-miss rate.
