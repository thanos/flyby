# Benchmarks

Every optimization must have a benchmark ([ADR-0012](./adr/0012-benchmarks-are-part-of-the-api.md)).
The `benches/` package holds Criterion harnesses for the framework.

## Running

```sh
cargo bench -p flyby-benches
# Faster smoke (compile + short run):
cargo test -p flyby-benches --benches
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

Hardware backends (AF_XDP, DPDK, io_uring, SPDK) are **not** measured in
portable CI; their harnesses land with self-hosted jobs
([hardware workflow](../../.github/workflows/hardware.yml)).

## Required metrics

| Metric | Portable today | Notes |
|---|---|---|
| Throughput | Yes | Criterion Elements / Bytes |
| Latency (mean / distribution) | Partial | Criterion iter times; report p50/p95/p99 from Criterion plots or `criterion` HTML when claiming latency wins |
| Allocations | Deferred | Document manual `dhat` / heaptrack when investigating; not CI-gated yet |
| CPU utilization | Deferred | Capture with `perf` / `htop` notes in HW reports |
| Batch size | Yes | Varied in net/storage benches |
| Queue depth / slot count | Partial | Memory ring capacity sweeps |
| Drops | Deferred | Prefer simulator scenario counters for drop behaviour |

## Report template

Attach this when a PR or release claims a performance change:

```text
Commit:        <sha>
Machine:       <model / vCPU / RAM>
OS / kernel:   <uname -sr>
CPU governor:  <optional>
NIC / storage: <none | model>   # for hardware runs
Command:       cargo bench -p flyby-benches -- <filter>
Baseline:      <previous tag or main sha>
Result:        <throughput / latency delta vs baseline>
Notes:         NullCollector used? batch size? warm-up?
```

## Rules

- Compare against a **baseline**, not against nothing.
- Use `NullCollector` (or equivalent) so metrics plumbing does not dominate.
- A change that claims an optimization must include or update a harness
  that demonstrates it.
- Absolute numbers are machine-specific; review **deltas**.
- Simulator throughput is **simulated** — never quote it as hardware.

## Related

- [Engineering standards](./engineering.md) · [Testing](./testing.md)
- [Release process](./release.md)
- Package README: [`benches/README.md`](../../benches/README.md)
