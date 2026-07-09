# Benchmarks

Every optimization must have a benchmark (design principle #4). The
`benches/` package holds the Criterion harnesses for the framework.

## Running

```sh
cargo bench -p flyby-benches
```

The current harness measures the builder skeleton. Real pipeline,
memory, and networking benchmarks arrive with their respective parts of
the specification and are gated so they only run when the matching
feature is enabled.

## Layout

```text
benches/
├── Cargo.toml
├── benches/
│   └── builder.rs        # builder construction + validation cost
└── src/
    └── lib.rs
```

## Rules

- Benchmarks compare against a baseline, not against nothing.
- `NullCollector` is the default metrics collector in benches so the
  collector does not pollute the measurement.
- A change that claims an optimization must include or update a
  benchmark that demonstrates it.
