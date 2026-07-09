# FlyBy documentation

This directory holds the **project-level** documentation for FlyBy: the
architecture, learning material, backend guides, and Architecture
Decision Records (ADRs).

API-level documentation lives in the source as rustdoc and is built with
`cargo doc`. This directory complements it with the *why*, *how*, *where
it fits*, and *when not to use it* that the specification requires of
every subsystem.

## Layout

```text
docs/
├── book.toml          # mdBook configuration
├── src/               # mdBook source pages
│   ├── SUMMARY.md     # table of contents (mdBook entry point)
│   ├── introduction.md
│   ├── getting-started.md
│   ├── architecture.md
│   ├── concepts/      # one page per core abstraction
│   ├── backends/      # one page per backend
│   ├── adr/           # Architecture Decision Records
│   ├── simulator.md
│   └── benchmarks.md
└── README.md          # this file
```

## Reading the docs

Two layers, two tools:

| Layer            | Tool          | Command                         |
|------------------|---------------|---------------------------------|
| API reference    | `cargo doc`   | `cargo doc --workspace --open`  |
| Project guide    | mdBook        | `mdbook serve docs/ --open`     |

The project guide is authored in [`src/SUMMARY.md`](src/SUMMARY.md) and
rendered with [mdBook](https://rust-lang.github.io/mdBook/). If you do
not have mdBook installed:

```sh
cargo install mdbook --locked
mdbook serve docs/ --open
```

## Architecture Decision Records (ADRs)

Any change to a public abstraction, a feature flag, or a cross-cutting
design rule requires an ADR. See
[`src/adr/0000-template.md`](src/adr/0000-template.md) for the format and
[`src/adr/0001-record-architecture-decisions.md`](src/adr/0001-record-architecture-decisions.md)
for the first record.
