# ADR-0001: Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-07-09

## Context

The FlyBy specification states that the core traits "are expected to
evolve, but changes require an Architecture Decision Record (ADR)."
Without a concrete format and location for those records, the
requirement is unenforceable and decisions get lost in commit messages
and chat.

We need a lightweight, version-controlled way to capture decisions that
affect the public abstractions, feature flags, and cross-cutting design
rules.

## Decision

Adopt the ADR pattern: numbered, single-file Markdown documents under
`docs/src/adr/`, starting with this record and a reusable template at
`docs/src/adr/0000-template.md`.

An ADR is required when:

- a core trait is added, removed, or changed;
- a feature flag is added, removed, or renamed;
- the workspace layout or the `flyby` facade's public surface changes;
- a new backend or a system-requiring external dependency is adopted;
- an earlier ADR is reversed.

ADRs are append-only. A reversed decision is marked **Superseded by
ADR-XXXX**, not deleted.

## Consequences

- The documentation guide gains an ADR section, listed in
  `docs/src/SUMMARY.md`.
- Pull requests touching the core traits or feature flags must include
  or reference an ADR; CI does not enforce this yet, but reviewers
  should.
- The set of ADRs becomes a durable history of *why* the architecture
  looks the way it does, which serves the project's educational
  objectives.

## Alternatives considered

- **RFCs in a separate repo.** Rejected: a second repo raises the
  friction to record a decision and splits the history.
- **Decision log in the README.** Rejected: a single file does not
  scale to many decisions and resists per-decision discussion.
- **No format, just commit messages.** Rejected: commit messages are
  hard to discover and do not survive squashes / history rewrites.
