# Benchmarks

Every optimization must have a benchmark (design principle #4). The
`benches/` package holds the Criterion harnesses for the framework.

## Running

```sh
cargo bench -p flyby-benches
```

## Layout

```text
benches/
├── Cargo.toml
├── benches/
│   ├── builder.rs    # builder construction + validation cost
│   ├── memory.rs     # shared-memory sink write/pop
│   ├── net.rs        # SimulatedNetSource poll_batch
│   └── storage.rs    # FileSource / framing paths
└── src/
    └── lib.rs
```

## Rules

- Benchmarks compare against a baseline, not against nothing.
- `NullCollector` is the default metrics collector in benches so the
  collector does not pollute the measurement.
- A change that claims an optimization must include or update a
  benchmark that demonstrates it.
