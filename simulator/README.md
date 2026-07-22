# flyby-simulator

First-class FlyBy subsystem for developing, testing, and benchmarking the
ingestion pipeline without privileged networking, AF_XDP, DPDK, SPDK, or
NVMe hardware. See ADR-007 and ADR-008.

## Features

- **VirtualNic** / **VirtualStorageSource** — same `NetworkSource` /
  `StorageSource` traits as production backends
- **SimClock** — real time or deterministic virtual time
- **TrafficPacer** — fixed-rate, burst, and full-speed patterns
- **FaultInjector** — deterministic drop / corrupt / latency spikes
- **SimScheduler** — tick loop with optional shared-memory ring + consumers
- **SimReplay** — virtual-clock adapter over `flyby_storage::ReplayMode`
- **EduControls** — pause, single-step, breakpoints, batch inspection
- **Scenarios** — version-controlled presets (`constant_rate`,
  `market_open_burst`, `packet_loss`, …)

## Quick start

```bash
cargo run -p flyby-simulator --bin flyby-sim -- constant_rate
```

Results are **simulated** and must not be quoted as hardware performance.

## Library usage

```rust,ignore
use flyby_simulator::{Scenario, SimScheduler, VirtualNic, VirtualNicConfig, NullEventSink};

let scenario = Scenario::constant_rate();
let mut sched = SimScheduler::new(scenario.clone());
sched.add_nic(VirtualNic::new(
    VirtualNicConfig { traffic: scenario.traffic.clone(), ..Default::default() },
    NullEventSink,
));
let stats = sched.run()?;
```

## Documentation

See [docs/src/simulator.md](../docs/src/simulator.md) and Part VI of the
master specification.
