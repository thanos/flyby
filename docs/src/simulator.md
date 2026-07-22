# Simulator

The FlyBy simulator is a **product feature**, not a test stub (ADR-007,
ADR-008). It lets developers understand, debug, benchmark, and demonstrate
the pipeline without privileged Linux networking, AF_XDP, DPDK, SPDK, or
NVMe hardware.

The crate lives at `simulator/` (`flyby-simulator`). Production backends and
the simulator share the same source traits; only the adapters differ.

## Architecture

```text
Virtual NICs      Virtual Storage
      │                  │
      └──────────┬───────┘
                 ▼
          Source Adapters
                 ▼
           Raw Batch Stream
                 ▼
        Virtual Shared Memory
                 ▼
        Virtual Consumers
```

## Components

| Type | Role |
|---|---|
| `VirtualNic` | `NetworkSource` with traffic pacing + fault injection |
| `VirtualStorageSource` | `StorageSource` wrapping `FileSource` + faults |
| `SimClock` | Real time or deterministic virtual time |
| `TrafficPacer` | Fixed-rate / burst / full-speed emission |
| `FaultInjector` | LCG-seeded drop, corrupt, latency spikes |
| `SimScheduler` | Tick loop, metrics, optional ring + consumers |
| `VirtualSharedMemory` | In-process SPSC byte-slot ring |
| `VirtualConsumer` | Drains the ring (supports slow-consumer mode) |
| `SimReplay` | Virtual-clock adapter over storage `ReplayMode` |
| `Scenario` | Version-controlled run presets |
| `EduControls` | Pause, step, breakpoints, batch inspection |

## CLI

```bash
cargo run -p flyby-simulator --bin flyby-sim -- constant_rate
```

Available scenarios: `constant_rate`, `market_open_burst`, `queue_overflow`,
`packet_loss`, `slow_consumer`, `corrupt_packets`.

Throughput numbers from the CLI are **simulated**.

## Library example

```rust,ignore
use flyby_simulator::{
    Scenario, SimScheduler, VirtualNic, VirtualNicConfig, NullEventSink,
    VirtualSharedMemory, VirtualConsumer,
};

let scenario = Scenario::packet_loss();
let mut sched = SimScheduler::new(scenario.clone())
    .with_ring(VirtualSharedMemory::new("ring0", 1024, 128));
sched.add_nic(VirtualNic::new(
    VirtualNicConfig {
        traffic: scenario.traffic.clone(),
        fault: scenario.fault.clone(),
        ..Default::default()
    },
    NullEventSink,
));
sched.add_consumer(VirtualConsumer::new("c0"));
let stats = sched.run()?;
assert!(stats.packets_dropped > 0);
```

## Educational mode

```rust,ignore
use flyby_simulator::{EduControls, Scenario, SimScheduler};

let mut sched = SimScheduler::new(Scenario::constant_rate())
    .with_edu(EduControls { paused: true, ..Default::default() });
sched.run()?;          // arms the run without ticking
sched.step()?;         // one tick
let batch = sched.last_batch();
sched.finish_run()?;
```

## Replay

Use `SimReplay` with `flyby_storage::ReplayMode` so original / scaled /
single-step / burst timing works against the simulator clock:

```rust,ignore
use flyby_simulator::SimReplay;
use flyby_storage::ReplayMode;

let mut replay = SimReplay::new(ReplayMode::TimeScaled { factor: 0.5 })?;
assert!(replay.ready_at(record_ts_ns, clock_ns));
```

## When not to use it

Do not quote simulator throughput or latency as production figures. Use it
for correctness, relative comparisons, tutorials, and CI.

## Related ADRs

- [ADR-0007: Simulator before hardware](./adr/0007-simulator-before-hardware.md)
- [ADR-0008: Simulator is a product feature](./adr/0008-simulator-is-a-product-feature.md)
