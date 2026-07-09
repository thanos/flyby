# AF_XDP backend

AF_XDP is the Linux kernel's high-performance packet socket family. It
attaches an XSK (AF_XDP socket) to a NIC queue and, with zero-copy
mode, hands packet buffers directly between userspace and the driver.

## Why it exists

For packet ingest at line rate, AF_XDP avoids the `AF_PACKET` copy and
the kernel networking stack, while staying in pure Linux (no
out-of-tree driver).

## How it works

The real binding arrives with Part IV of the specification. It will be
gated behind the `af_xdp` feature and will isolate all `unsafe` in
clearly marked modules with a safety comment per block.

## Where it fits

A `Source` at the head of the pipeline.

## When not to use it

- You need kernel filtering / rerouting decisions. That is XDP (eBPF)
  territory, not AF_XDP.
- Your rates are modest. `AF_PACKET` is simpler and good enough.
- You need cross-vendor portability. AF_XDP is Linux-only.

## How to measure it

- Packets per second and bytes per second per queue.
- Fill/Completion ring occupancy and drops.
- `poll` latency and wakeups per second.
