# Simulator

The FlyBy simulator is a **product feature**, not a test stub (ADR-007,
ADR-008). It lets developers understand, debug, benchmark, and demonstrate
the pipeline without privileged Linux networking, AF_XDP, DPDK, SPDK, or
NVMe hardware.

The crate lives at `simulator/` (`flyby-simulator`). Production backends and
the simulator share the same source traits; only the adapters differ.

## Architecture

```text
Virtual NICs / Pcap      Virtual Storage
      │                         │
      └────────────┬────────────┘
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
| `VirtualNic` | `NetworkSource` with traffic pacing + payload generators + faults |
| `PcapSource` | Classic pcap ingest with `SimReplay` timing |
| `VirtualStorageSource` | `StorageSource` wrapping `FileSource` + faults |
| `SimClock` | Real time or deterministic virtual time |
| `TrafficPacer` | Fixed-rate / burst / Gaussian / full-speed emission |
| `PayloadSpec` | Fixed-seq, random, Gaussian size, protocol-aware, custom callbacks |
| `FaultInjector` | LCG-seeded drop, corrupt, latency spikes |
| `SimScheduler` | Tick loop, metrics, optional ring + consumers |
| `VirtualSharedMemory` | In-process SPSC byte-slot ring |
| `VirtualConsumer` | Drains the ring (supports slow-consumer mode) |
| `SimReplay` | Virtual-clock adapter over storage `ReplayMode` |
| `Scenario` | Version-controlled run presets |
| `EduControls` | Pause, step, breakpoints, batch inspection |

## Traffic generators

```rust,ignore
use flyby_simulator::{PayloadSpec, ProtocolMessage, TrafficConfig, TrafficPattern};

// Gaussian arrivals
let gaussian = TrafficConfig::gaussian_rate();

// Protocol-aware binary quotes
let quotes = TrafficConfig {
    pattern: TrafficPattern::FixedRate { pps: 10_000 },
    payload_size: 34,
    batch_size: 64,
    payload: PayloadSpec::Protocol(ProtocolMessage::market_quote("AAPL")),
};

// Custom callback
use std::sync::Arc;
let custom = TrafficConfig {
    payload: PayloadSpec::Custom(Arc::new(|seq, buf| {
        buf.fill(0);
        buf[0] = (seq & 0xFF) as u8;
    })),
    ..TrafficConfig::default()
};
```

## Pcap ingest

Classic pcap only (not pcap-ng). Convert with `editcap -F pcap` if needed.

```bash
cargo run -p flyby-simulator --bin flyby-sim -- pcap capture.pcap --full-speed
```

```rust,ignore
use flyby_simulator::{PcapConfig, PcapSource, load_pcap, NullEventSink};
use flyby_storage::ReplayMode;

let packets = load_pcap("capture.pcap")?;
let src = PcapSource::new(
    packets,
    PcapConfig { replay: ReplayMode::OriginalTiming, ..Default::default() },
    NullEventSink,
)?;
```

## CLI

```bash
cargo run -p flyby-simulator --bin flyby-sim -- constant_rate
cargo run -p flyby-simulator --bin flyby-sim -- gaussian_rate
cargo run -p flyby-simulator --bin flyby-sim -- protocol_quotes
```

Available scenarios: `constant_rate`, `market_open_burst`, `queue_overflow`,
`packet_loss`, `slow_consumer`, `corrupt_packets`, `gaussian_rate`,
`protocol_quotes`.

Throughput numbers from the CLI are **simulated**.

## When not to use it

Do not quote simulator throughput or latency as production figures. Use it
for correctness, relative comparisons, tutorials, and CI.

## Related ADRs

- [ADR-0007: Simulator before hardware](./adr/0007-simulator-before-hardware.md)
- [ADR-0008: Simulator is a product feature](./adr/0008-simulator-is-a-product-feature.md)
