# flyby-simulator

First-class FlyBy subsystem for developing, testing, and benchmarking the
ingestion pipeline without privileged networking, AF_XDP, DPDK, SPDK, or
NVMe hardware. See ADR-007 and ADR-008.

## Features

- **VirtualNic** / **VirtualStorageSource** / **PcapSource** — same source
  traits as production backends
- **TrafficPacer** — fixed-rate, burst, Gaussian, and full-speed patterns
- **PayloadSpec** — fixed-seq, random, Gaussian size, protocol-aware
  (market quote / FIX-like), and custom callbacks
- **SimClock** — real time or deterministic virtual time
- **FaultInjector** — deterministic drop / corrupt / latency spikes
- **SimScheduler** — tick loop with optional shared-memory ring + consumers
- **SimReplay** — virtual-clock adapter over `flyby_storage::ReplayMode`
- **EduControls** — pause, single-step, breakpoints, batch inspection
- **Scenarios** — version-controlled presets

## Quick start

```bash
cargo run -p flyby-simulator --bin flyby-sim -- constant_rate
cargo run -p flyby-simulator --bin flyby-sim -- gaussian_rate
cargo run -p flyby-simulator --bin flyby-sim -- protocol_quotes
cargo run -p flyby-simulator --bin flyby-sim -- pcap capture.pcap --full-speed
```

Results are **simulated** and must not be quoted as hardware performance.

## Documentation

See [docs/src/simulator.md](../docs/src/simulator.md) and Part VI of the
master specification.
