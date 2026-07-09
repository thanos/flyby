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

| Number | Title                                  | Status |
|--------|----------------------------------------|--------|
| 0001   | Record architecture decisions          | Accepted |
