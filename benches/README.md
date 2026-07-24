# flyby-benches

Criterion benchmark harnesses for FlyBy. Every optimization must have a
benchmark ([ADR-0012](../docs/src/adr/0012-benchmarks-are-part-of-the-api.md)).

```sh
cargo bench -p flyby-benches
cargo test -p flyby-benches --benches --no-run   # CI compile check
```

Methodology, metrics matrix, and report template:
[docs/src/benchmarks.md](../docs/src/benchmarks.md).
