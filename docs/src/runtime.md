# Runtime

The FlyBy **runtime** coordinates every pipeline regardless of source or
sink (ADR-010). It owns lifecycle, scheduling, batching defaults,
back-pressure policy, and telemetry hooks. It never embeds AF_XDP, DPDK,
io_uring, SPDK, or simulator internals.

Crate path: `flyby::runtime`.

## Principles

- One execution model for every backend
- Batch-oriented processing (ADR-009)
- Deterministic behaviour when requested (cooperative single-thread + simulator)
- Explicit, observable back-pressure
- Configuration over hard-coded policy (builder ≡ TOML)
- Observability built in (`RuntimeMetricKey`, lifecycle events)

## Lifecycle

```text
Build → Validate → Initialize → Start/Run → Drain → Shutdown → Cleanup
```

[`Runtime`] drives these phases around any [`Pipeline`](./concepts/pipeline.md):

```rust,ignore
use flyby::prelude::*;

let mut rt = Runtime::build(pipeline, RuntimeConfig::default())?;
let stats = rt.run()?; // validate → init → run → drain → shutdown → cleanup
assert_eq!(rt.phase(), RuntimePhase::CleanedUp);
```

## Configuration

Builders and files are equivalent:

```toml
[runtime]
workers = 4
batch_size = 512
backpressure = "block"
scheduler = "default"
metrics = true
```

```rust,ignore
let cfg = RuntimeConfig::default()
    .with_workers(4)
    .with_batch_size(512)
    .with_backpressure(BackpressureStrategy::Block)
    .with_scheduler(SchedulerKind::Default);
// or: RuntimeConfig::from_toml_path("runtime.toml")?;
```

| Field | Meaning |
|---|---|
| `workers` | Worker-pool size (`≥ 1`) |
| `batch_size` | Batch hint / pending cap |
| `backpressure` | `block` \| `spin` \| `drop_newest` \| `drop_oldest` \| `overflow` \| `adaptive_batching` |
| `scheduler` | `default` \| `single_thread` \| `worker_pool` |
| `metrics` | Emit `runtime.*` metrics |
| `overflow_sink` | Sink id for overflow strategy |
| `idle_sleep_ms` | Optional park when source idle |

## Scheduling

| Kind | Behaviour |
|---|---|
| `SingleThreadScheduler` / `default` | Cooperative loop on the calling thread |
| `WorkerPoolScheduler` | N threads, each owning a pipeline from a factory (OS threads stay inside the runtime) |

```rust,ignore
let pool = WorkerPoolScheduler::new(cfg)?;
pool.run_factory(|worker_idx| Ok(build_pipeline(worker_idx)))?;
```

CPU affinity / NUMA (`CpuAffinityPolicy`) are optional stubs — documented
no-ops until measured (Part VII §6).

## Placement

Routing stays outside business backends. Built-ins:

| Type | Role |
|---|---|
| `FixedPlacement` | Always one sink |
| `DropAllPlacement` | `SinkId::NONE` |
| `RoundRobinPlacement` | Cycle sinks |
| `HashPlacement` / `schema_hash_placement` | Key → sink |
| `CallbackPlacement` | Custom callback (rules stay in app code) |

## Back-pressure

When a sink returns `ErrorKind::BackPressure`, `SimplePipeline` applies the
configured strategy and records `runtime.backpressure_events` /
`runtime.messages_dropped`. Block/spin **do not lose** the current frame
(pending index advances only after write or intentional drop).

## Telemetry

| Key | Kind |
|---|---|
| `runtime.steps` | counter |
| `runtime.messages_out` | counter |
| `runtime.messages_dropped` | counter |
| `runtime.backpressure_events` | counter |
| `runtime.step_duration_ns` | histogram |
| `runtime.phase` | gauge |
| `runtime.event.*` | lifecycle counters |

Tracing spans / OpenTelemetry are future consumers of the same hooks.

## Error handling

The runtime distinguishes fatal startup (`validate` / `init`), recoverable
sink back-pressure (strategy), decode skips (`Idle`), and shutdown errors
(first error wins while still attempting remaining cleanup).

## Testing

See `crates/flyby/tests/runtime.rs`: lifecycle, shutdown, scheduling,
back-pressure, batching, placement, affinity stubs, TOML parity.

## Related

- [ADR-0009: Batch-oriented runtime](./adr/0009-batch-oriented-runtime.md)
- [ADR-0010: Runtime independent of backends](./adr/0010-runtime-independent-of-backends.md)
- [Pipeline](./concepts/pipeline.md) · [Placement](./concepts/placement.md)
- [Simulator](./simulator.md) — deterministic product-sim scheduling (separate subsystem)
