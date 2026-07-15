# ADR-0003: eBPF/XDP Is an Implementation Detail

- **Status:** Accepted
- **Date:** 2026-07-14

## Context

The AF_XDP backend uses a small XDP/eBPF program to redirect packets from
the NIC receive path into an AF_XDP socket (XSK). This program runs in the
kernel, and some frameworks expose eBPF as a first-class programming model,
allowing users to write kernel-side filtering logic.

FlyBy must decide how much of the eBPF surface to expose to users.

## Decision

Treat eBPF/XDP as an implementation detail of the AF_XDP backend.

FlyBy users define messages, decoders, preprocessors, placement strategies,
and sinks — all in userspace, all in safe Rust. They do not write eBPF
programs for ordinary use.

The XDP program bundled with FlyBy performs one job: redirect matching
packets to the configured AF_XDP socket. Optional minimal filtering (by
UDP port, for example) may be configured via `XdpConfig`, but no application
protocol logic belongs in the kernel.

## Consequences

- Simpler user model: FlyBy users never need to learn eBPF to use the
  networking backend.
- Less verifier complexity: the bundled XDP program is small and easy to
  audit.
- Minimal risk of embedding business logic in kernel space, where debugging
  is harder and failures are more severe.
- Users who need advanced kernel-side filtering can load their own XDP
  programs before starting FlyBy; the backend will coexist with existing
  XDP attachments where the kernel supports it.
- The eBPF program strategy (C with clang/LLVM, Rust with Aya, or a
  prebuilt object) is a separate decision, deferred until the AF_XDP
  binding is implemented.

## Alternatives Considered

- **Expose eBPF as a FlyBy extension point**: rejected because it couples
  the user model to kernel-programming concepts and makes testing much
  harder.
- **Use Aya for eBPF**: promising, but the choice of eBPF toolchain is a
  build-time concern, not an API decision. It can be made later without
  affecting this ADR.
