# Architecture Decision Records

An **Architecture Decision Record (ADR)** captures a decision that
affects FlyBy's public abstractions, feature flags, or cross-cutting
design rules.

The specification mandates that changes to the core traits require an
ADR. We extend that to any decision with lasting architectural
consequences.

## When to write an ADR

- Adding, removing, or changing a core trait.
- Adding, removing, or renaming a feature flag.
- Changing the workspace layout or the facade's public surface.
- Adopting a new backend or a new external dependency with system
  requirements.
- Reversing an earlier ADR.

## Format

ADRs are numbered, single-file Markdown documents. Copy
[`0000-template.md`](./0000-template.md) to start. Use the form
`NNNN-kebab-case-title.md`, where `NNNN` is the next free number.

## Index

| Number | Title | Status |
|--------|--------|--------|
| 0001 | Record architecture decisions | Accepted |
| 0002 | AF_XDP before DPDK | Accepted |
| 0003 | eBPF is an implementation detail | Accepted |
| 0004 | Copy mode before zero-copy | Accepted |
| 0005 | File reader before io_uring | Accepted |
| 0006 | io_uring before SPDK | Accepted |
| 0007 | Simulator before hardware | Accepted |
| 0008 | Simulator is a product feature | Accepted |
| 0009 | Batch-oriented runtime | Accepted |
| 0010 | Runtime independent of backends | Accepted |
| 0011 | Simulator required for new features | Accepted |
| 0012 | Benchmarks are part of the API | Accepted |
