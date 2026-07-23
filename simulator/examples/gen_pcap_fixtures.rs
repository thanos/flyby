//! Generate classic pcap fixtures under `simulator/fixtures/`.
//!
//! ```bash
//! cargo run -p flyby-simulator --example gen_pcap_fixtures
//! ```

use std::fs;
use std::path::PathBuf;

use flyby_simulator::generator::{PayloadGenerator, PayloadSpec, ProtocolMessage, build_udp_frame};
use flyby_simulator::{write_pcap_bytes, write_pcap_bytes_ex};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn write(name: &str, bytes: &[u8]) {
    let path = fixtures_dir().join(name);
    fs::write(&path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
}

fn eth_pad(payload: &[u8]) -> Vec<u8> {
    // Minimal Ethernet-like frame: 14-byte header + payload, padded to 60.
    let mut frame = vec![0u8; 14];
    frame[0..6].copy_from_slice(&[0xff; 6]);
    frame[6..12].copy_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
    frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    frame.extend_from_slice(payload);
    if frame.len() < 60 {
        frame.resize(60, 0);
    }
    frame
}

fn main() {
    let dir = fixtures_dir();
    fs::create_dir_all(&dir).expect("create fixtures dir");

    // 1) Valid header, zero packets.
    write("empty.pcap", &write_pcap_bytes(&[]));

    // 2) Three spaced packets for OriginalTiming tests.
    let f0 = eth_pad(b"pkt0");
    let f1 = eth_pad(b"pkt1");
    let f2 = eth_pad(b"pkt2");
    write(
        "tiny_3pkt.pcap",
        &write_pcap_bytes(&[(0, &f0), (1_000_000, &f1), (5_000_000, &f2)]),
    );

    // 3) Tight burst: 100 frames, 10 µs apart (1 ms total).
    let burst_frames: Vec<(u64, Vec<u8>)> = (0..100u64)
        .map(|i| {
            let mut payload = [0u8; 8];
            payload.copy_from_slice(&i.to_be_bytes());
            (i * 10_000, eth_pad(&payload))
        })
        .collect();
    let burst_refs: Vec<(u64, &[u8])> = burst_frames
        .iter()
        .map(|(t, d)| (*t, d.as_slice()))
        .collect();
    write("burst_100.pcap", &write_pcap_bytes(&burst_refs));

    // 4) UDP market-quote frames (matches ProtocolMessage::MarketQuote).
    let mut quote_gen =
        PayloadGenerator::new(PayloadSpec::Protocol(ProtocolMessage::market_quote("AAPL")));
    let mut quote_frames = Vec::new();
    for seq in 1..=20u64 {
        let mut payload = [0u8; 34];
        let n = quote_gen.fill(seq, &mut payload);
        let frame = build_udp_frame(&payload[..n], 9000);
        // 100 µs between quotes
        quote_frames.push((seq.saturating_sub(1) * 100_000, frame));
    }
    let quote_refs: Vec<(u64, &[u8])> = quote_frames
        .iter()
        .map(|(t, d)| (*t, d.as_slice()))
        .collect();
    write("udp_quotes.pcap", &write_pcap_bytes(&quote_refs));

    // 5) Nanosecond-resolution timestamps (sub-µs spacing).
    let n0 = eth_pad(b"ns0");
    let n1 = eth_pad(b"ns1");
    let n2 = eth_pad(b"ns2");
    write(
        "ns_timestamps.pcap",
        &write_pcap_bytes_ex(
            &[
                (0, &n0),
                (250, &n1),   // 250 ns
                (1_500, &n2), // 1.5 µs
            ],
            true,
        ),
    );

    // 6) Longer capture for CLI demos: 1 second of 1 kpps UDP quotes.
    let mut long_gen =
        PayloadGenerator::new(PayloadSpec::Protocol(ProtocolMessage::market_quote("MSFT")));
    let mut long_frames = Vec::new();
    for seq in 1..=1_000u64 {
        let mut payload = [0u8; 34];
        let n = long_gen.fill(seq, &mut payload);
        let frame = build_udp_frame(&payload[..n], 9000);
        long_frames.push(((seq - 1) * 1_000_000, frame)); // 1 ms spacing
    }
    let long_refs: Vec<(u64, &[u8])> = long_frames
        .iter()
        .map(|(t, d)| (*t, d.as_slice()))
        .collect();
    write("quotes_1s_1kpps.pcap", &write_pcap_bytes(&long_refs));

    println!("\nFixtures ready in {}", dir.display());
    println!("Try:");
    println!(
        "  cargo run -p flyby-simulator --bin flyby-sim -- pcap {}/tiny_3pkt.pcap --full-speed",
        dir.display()
    );
    println!(
        "  cargo run -p flyby-simulator --bin flyby-sim -- pcap {}/udp_quotes.pcap",
        dir.display()
    );
}
