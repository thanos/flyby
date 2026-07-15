//! Integration tests: SimulatedNetSource end-to-end.
//!
//! Drives the simulated network source through its full lifecycle and verifies
//! batch contents, packet structure, and error behaviour.
//!
//! Covers:
//! - Packet count per batch matches config
//! - Ethernet/IP/UDP frame structure (header bytes)
//! - Sequence number monotonicity across polls
//! - Drop rate produces non-zero drop count
//! - Init/shutdown lifecycle
//! - Uninitialized source rejects polls
//! - Varying payload sizes

use flyby_core::Lifecycle;
use flyby_net::{NetworkSource, RawBatch, SimNetConfig, SimulatedNetSource};

const ETH_HEADER: usize = 14;
const IP_HEADER: usize = 20;
const UDP_HEADER: usize = 8;
const NET_HEADER: usize = ETH_HEADER + IP_HEADER + UDP_HEADER;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_src(cfg: SimNetConfig) -> SimulatedNetSource {
    SimulatedNetSource::new(cfg)
}

fn default_batch(n: usize) -> RawBatch {
    RawBatch::new(n, 2048)
}

// ---------------------------------------------------------------------------
// Packet count
// ---------------------------------------------------------------------------

#[test]
fn batch_contains_configured_packet_count() {
    let cfg = SimNetConfig {
        batch_size: 16,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(32);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 16);
}

#[test]
fn batch_size_one_yields_single_packet() {
    let cfg = SimNetConfig {
        batch_size: 1,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(8);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 1);
}

// ---------------------------------------------------------------------------
// Frame structure
// ---------------------------------------------------------------------------

#[test]
fn packet_total_length_equals_header_plus_payload() {
    let payload = 32usize;
    let cfg = SimNetConfig {
        payload_size: payload,
        batch_size: 4,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(8);
    src.poll_batch(&mut batch).unwrap();

    for (data, _meta) in batch.packets() {
        assert_eq!(
            data.len(),
            NET_HEADER + payload,
            "frame = 42 header bytes + {payload} payload bytes"
        );
    }
}

#[test]
fn ethernet_dst_is_broadcast() {
    let cfg = SimNetConfig {
        batch_size: 1,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(4);
    src.poll_batch(&mut batch).unwrap();

    let (data, _) = batch.packets().next().unwrap();
    assert_eq!(
        &data[0..6],
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        "dst must be broadcast"
    );
}

#[test]
fn ethernet_src_is_fixed() {
    let cfg = SimNetConfig {
        batch_size: 1,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(4);
    src.poll_batch(&mut batch).unwrap();

    let (data, _) = batch.packets().next().unwrap();
    assert_eq!(
        &data[6..12],
        &[0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        "fixed src MAC"
    );
}

#[test]
fn ethertype_is_ipv4() {
    let cfg = SimNetConfig {
        batch_size: 1,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(4);
    src.poll_batch(&mut batch).unwrap();

    let (data, _) = batch.packets().next().unwrap();
    assert_eq!(&data[12..14], &[0x08, 0x00], "EtherType 0x0800 = IPv4");
}

// ---------------------------------------------------------------------------
// Sequence numbers
// ---------------------------------------------------------------------------

#[test]
fn sequence_numbers_increase_across_polls() {
    let cfg = SimNetConfig {
        batch_size: 4,
        payload_size: 8,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();

    let mut all_seqs: Vec<u64> = Vec::new();
    for _ in 0..3 {
        let mut batch = default_batch(8);
        src.poll_batch(&mut batch).unwrap();
        for (data, _) in batch.packets() {
            let seq = u64::from_be_bytes(data[NET_HEADER..NET_HEADER + 8].try_into().unwrap());
            all_seqs.push(seq);
        }
    }

    // Sequences must be strictly increasing
    for window in all_seqs.windows(2) {
        assert!(
            window[1] > window[0],
            "sequence numbers must be strictly increasing"
        );
    }
}

// ---------------------------------------------------------------------------
// Drop rate
// ---------------------------------------------------------------------------

#[test]
fn nonzero_drop_rate_produces_drops_over_many_polls() {
    let cfg = SimNetConfig {
        batch_size: 64,
        drop_rate: 0.9, // 90% drop
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(64);

    let n = src.poll_batch(&mut batch).unwrap();
    // With 90% drop, fewer than 64 packets should be present
    assert!(
        n < 64,
        "90% drop rate should produce fewer than 64 packets, got {n}"
    );
    assert!(batch.dropped > 0, "dropped counter should be non-zero");
}

#[test]
fn zero_drop_rate_yields_full_batch() {
    let cfg = SimNetConfig {
        batch_size: 8,
        drop_rate: 0.0,
        idle_rate: 0.0,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(16);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 8, "no drops, no idle — full batch expected");
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn uninitialized_source_poll_returns_error() {
    let mut src = make_src(SimNetConfig::default());
    let mut batch = default_batch(8);
    assert!(src.poll_batch(&mut batch).is_err());
}

#[test]
fn shutdown_prevents_further_polls() {
    let cfg = SimNetConfig {
        batch_size: 4,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();

    let mut batch = default_batch(8);
    src.poll_batch(&mut batch).unwrap();

    src.shutdown().unwrap();
    batch.reset(2048);
    assert!(
        src.poll_batch(&mut batch).is_err(),
        "post-shutdown poll must fail"
    );
}

#[test]
fn reinit_after_shutdown_works() {
    let cfg = SimNetConfig {
        batch_size: 2,
        ..SimNetConfig::default()
    };
    let mut src = make_src(cfg);
    src.init().unwrap();
    let mut batch = default_batch(4);
    src.poll_batch(&mut batch).unwrap();

    src.shutdown().unwrap();
    src.init().unwrap();

    batch.reset(2048);
    let n = src.poll_batch(&mut batch).unwrap();
    assert_eq!(n, 2);
}

// ---------------------------------------------------------------------------
// Backend name
// ---------------------------------------------------------------------------

#[test]
fn backend_name_is_sim() {
    let cfg = SimNetConfig {
        batch_size: 1,
        ..SimNetConfig::default()
    };
    let mut src = SimulatedNetSource::new(cfg);
    src.init().unwrap();
    assert_eq!(src.backend_name(), "simulator");
}
