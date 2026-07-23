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
| TUI dashboard | Ratatui view of clock, queues, events, sparklines |

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

## CLI (headless)

```bash
cargo run -p flyby-simulator --bin flyby-sim -- constant_rate
cargo run -p flyby-simulator --bin flyby-sim -- gaussian_rate
cargo run -p flyby-simulator --bin flyby-sim -- protocol_quotes
```

Available scenarios: `constant_rate`, `market_open_burst`, `queue_overflow`,
`packet_loss`, `slow_consumer`, `corrupt_packets`, `gaussian_rate`,
`protocol_quotes`.

Throughput numbers from the CLI are **simulated**.

## TUI dashboard

The Ratatui dashboard is the interactive way to watch a scenario: clock,
ring occupancy, drop counters, event flow, and sparklines. Feature `tui`
is enabled by default.

### Launch

```bash
# Steady baseline
cargo run -p flyby-simulator --bin flyby-sim -- tui constant_rate

# Fault injection (watch drop %)
cargo run -p flyby-simulator --bin flyby-sim -- tui packet_loss

# Tiny ring — occupancy / overflow pressure
cargo run -p flyby-simulator --bin flyby-sim -- tui queue_overflow

# Protocol-aware payloads
cargo run -p flyby-simulator --bin flyby-sim -- tui protocol_quotes
```

Requires a real terminal (not all CI log scrapers). For headless builds
without Ratatui: `--no-default-features`.

### Keyboard controls

| Key | Action |
|---|---|
| `Space` | Toggle auto-run / pause |
| `s` or `→` | Single-step one scheduler tick |
| `+` / `-` | Increase / decrease ticks per UI frame |
| `r` | Restart the scenario from tick 0 |
| `q` or `Esc` | Quit (also `Ctrl-C`) |

Suggested first session: start paused → press `s` a few times → `Space` to
auto-run → `+` to speed up → `q` to exit.

### What each pane shows

1. **Header** — scenario name, PAUSED/AUTO/DONE badge, **\[SIMULATED\]** label  
2. **Simulator clock** — virtual-time progress through the scenario duration  
3. **Pipeline / queues** — packets generated/dropped/corrupted, SHM writes,
   consumer reads, ring fill, last batch size  
4. **Ring occupancy** — gauge for shared-memory back-pressure  
5. **Event flow** — recent faults, ticks, lifecycle (quieter during fast auto-run)  
6. **Sparklines** — packets per tick and tick wall-latency (ns)  
7. **Footer** — status line + key hints  

### Screenshots

Captured from the live dashboard via the TestBackend (regenerate with
`cargo run -p flyby-simulator --example render_tui_docs`).

**Paused at start** (`constant_rate`):

![TUI paused on constant_rate](./images/tui/01-paused-constant-rate.svg)

**After stepping** (`packet_loss` — note the drop counter):

![TUI packet_loss after steps](./images/tui/02-packet-loss-stepped.svg)

**Ring pressure** (`queue_overflow`):

![TUI queue_overflow](./images/tui/03-queue-overflow.svg)

Plain-text copies of the same frames live beside the SVGs in
[`docs/src/images/tui/`](./images/tui/) for diff-friendly reviews.

### Regenerating screenshots

```bash
cargo run -p flyby-simulator --example render_tui_docs
```

## Medium articles

Publishing hooks live under `articles/` (catalog + screenshots + expected
output). Reproduce a post with:

```bash
./scripts/reproduce-article.sh part-vi-simulator-intro
```

See [Medium articles](./articles.md).

## When not to use it

Do not quote simulator throughput or latency as production figures. Use it
for correctness, relative comparisons, tutorials, and CI.

## Related ADRs

- [ADR-0007: Simulator before hardware](./adr/0007-simulator-before-hardware.md)
- [ADR-0008: Simulator is a product feature](./adr/0008-simulator-is-a-product-feature.md)
