# Pcap fixtures

Classic libpcap captures for simulator tests and CLI demos.

Regenerate after changing frame layouts:

```bash
cargo run -p flyby-simulator --example gen_pcap_fixtures
```

| File | Packets | Timing | Notes |
|---|---|---|---|
| `empty.pcap` | 0 | — | Valid global header only |
| `tiny_3pkt.pcap` | 3 | 0 / 1 ms / 5 ms | OriginalTiming smoke |
| `burst_100.pcap` | 100 | 10 µs apart | Short burst |
| `udp_quotes.pcap` | 20 | 100 µs apart | UDP + binary AAPL quotes |
| `ns_timestamps.pcap` | 3 | 0 / 250 ns / 1.5 µs | Nanosecond magic |
| `quotes_1s_1kpps.pcap` | 1000 | 1 ms apart | 1 s CLI demo (MSFT quotes) |

## Try

```bash
cargo run -p flyby-simulator --bin flyby-sim -- pcap simulator/fixtures/tiny_3pkt.pcap --full-speed
cargo run -p flyby-simulator --bin flyby-sim -- pcap simulator/fixtures/udp_quotes.pcap --full-speed
cargo run -p flyby-simulator --bin flyby-sim -- pcap simulator/fixtures/quotes_1s_1kpps.pcap
```

Omit `--full-speed` to honour capture timestamps via virtual-clock OriginalTiming.
