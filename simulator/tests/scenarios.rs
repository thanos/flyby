//! Integration tests: run full scenarios through the scheduler.
//!
//! Each test exercises a named [`Scenario`] end-to-end — scenario construction,
//! NIC creation, scheduler execution, and stat verification.

use flyby_core::{CountingCollector, MetricsCollector};
use flyby_simulator::{
    FaultSpec, Scenario, SimScheduler, VirtualConsumer, VirtualNic, VirtualNicConfig,
    VirtualSharedMemory,
    events::{NullEventSink, SimEventKind, VecEventSink},
};
use std::sync::atomic::Ordering;
use std::time::Duration;

fn nic_for(scenario: &Scenario) -> VirtualNic<NullEventSink> {
    VirtualNic::new(
        VirtualNicConfig {
            traffic: scenario.traffic.clone(),
            fault: scenario.fault.clone(),
            ..VirtualNicConfig::default()
        },
        NullEventSink,
    )
}

// ---------------------------------------------------------------------------
// Constant-rate scenario
// ---------------------------------------------------------------------------

#[test]
fn constant_rate_generates_nonzero_packets() {
    let scenario = Scenario::constant_rate();
    let mut sched = SimScheduler::new(scenario.clone());
    sched.add_nic(nic_for(&scenario));

    let stats = sched.run().unwrap();
    assert!(stats.packets_generated > 0, "should generate packets");
    assert_eq!(stats.packets_dropped, 0, "no faults in constant_rate");
    assert_eq!(stats.packets_corrupted, 0, "no faults in constant_rate");
}

#[test]
fn constant_rate_tick_count_matches_duration() {
    let scenario = Scenario::constant_rate();
    let expected = (scenario.duration.as_nanos() as u64) / scenario.tick_ns;
    let mut sched = SimScheduler::new(scenario);
    let stats = sched.run().unwrap();
    assert_eq!(stats.ticks, expected);
}

// ---------------------------------------------------------------------------
// Packet-loss scenario
// ---------------------------------------------------------------------------

#[test]
fn packet_loss_scenario_records_drops() {
    let scenario = Scenario::packet_loss();
    let mut sched = SimScheduler::new(scenario.clone());
    sched.add_nic(nic_for(&scenario));

    let stats = sched.run().unwrap();
    assert!(stats.packets_generated > 0);
    assert!(
        stats.packets_dropped > 0,
        "packet_loss scenario should drop some packets, got 0 out of {}",
        stats.packets_generated
    );
}

// ---------------------------------------------------------------------------
// Corrupt-packets scenario
// ---------------------------------------------------------------------------

#[test]
fn corrupt_packets_scenario_records_corruption() {
    let scenario = Scenario::corrupt_packets();
    let mut sched = SimScheduler::new(scenario.clone());
    sched.add_nic(nic_for(&scenario));

    let stats = sched.run().unwrap();
    assert!(stats.packets_generated > 0);
    assert!(
        stats.packets_corrupted > 0,
        "corrupt_packets scenario should corrupt some, got 0 out of {}",
        stats.packets_generated
    );
}

// ---------------------------------------------------------------------------
// Full-drop edge case
// ---------------------------------------------------------------------------

#[test]
fn full_drop_rate_drops_every_packet() {
    let scenario = Scenario {
        duration: Duration::from_millis(5),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };
    let nic = VirtualNic::new(
        VirtualNicConfig {
            traffic: scenario.traffic.clone(),
            fault: FaultSpec {
                drop_rate: 1.0,
                ..FaultSpec::default()
            },
            ..VirtualNicConfig::default()
        },
        NullEventSink,
    );
    let mut sched = SimScheduler::new(scenario);
    sched.add_nic(nic);

    let stats = sched.run().unwrap();
    assert_eq!(
        stats.packets_dropped, stats.packets_generated,
        "100% drop rate: dropped={} generated={}",
        stats.packets_dropped, stats.packets_generated
    );
}

// ---------------------------------------------------------------------------
// Event tracing
// ---------------------------------------------------------------------------

#[test]
fn run_emits_started_and_stopped_events() {
    let scenario = Scenario {
        duration: Duration::from_millis(3),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };
    let sink = VecEventSink::new();
    let mut sched = SimScheduler::with_events(scenario.clone(), sink.clone());
    sched.add_nic(nic_for(&scenario));
    sched.run().unwrap();

    let events = sink.events();
    let started = events
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::SimulatorStarted { .. }))
        .count();
    let stopped = events
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::SimulatorStopped { .. }))
        .count();
    assert_eq!(started, 1, "exactly one SimulatorStarted event");
    assert_eq!(stopped, 1, "exactly one SimulatorStopped event");
}

#[test]
fn run_emits_tick_completed_events() {
    let scenario = Scenario {
        duration: Duration::from_millis(3),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };
    let expected_ticks = (scenario.duration.as_nanos() as u64) / scenario.tick_ns;
    let sink = VecEventSink::new();
    let mut sched = SimScheduler::with_events(scenario.clone(), sink.clone());
    sched.add_nic(nic_for(&scenario));
    sched.run().unwrap();

    let ticks = sink
        .events()
        .iter()
        .filter(|e| matches!(e.kind, SimEventKind::TickCompleted { .. }))
        .count();
    assert_eq!(ticks as u64, expected_ticks);
}

// ---------------------------------------------------------------------------
// No-NIC degenerate case
// ---------------------------------------------------------------------------

#[test]
fn scheduler_runs_with_no_nics() {
    let scenario = Scenario {
        duration: Duration::from_millis(2),
        tick_ns: 1_000_000,
        ..Scenario::default()
    };
    let mut sched = SimScheduler::new(scenario);
    let stats = sched.run().unwrap();
    assert_eq!(stats.packets_generated, 0);
    assert!(stats.ticks > 0, "should still tick even with no NICs");
}

// ---------------------------------------------------------------------------
// Multiple NICs
// ---------------------------------------------------------------------------

#[test]
fn two_nics_double_the_traffic() {
    let scenario = Scenario {
        duration: Duration::from_millis(5),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };

    let stats_one = {
        let mut s = SimScheduler::new(scenario.clone());
        s.add_nic(nic_for(&scenario));
        s.run().unwrap()
    };

    let stats_two = {
        let mut s = SimScheduler::new(scenario.clone());
        s.add_nic(nic_for(&scenario));
        s.add_nic(nic_for(&scenario));
        s.run().unwrap()
    };

    assert!(
        stats_two.packets_generated >= stats_one.packets_generated,
        "two NICs should produce at least as many packets as one"
    );
}

// ---------------------------------------------------------------------------
// Ring + consumer + metrics
// ---------------------------------------------------------------------------

#[test]
fn ring_path_delivers_packets_to_consumer() {
    let scenario = Scenario {
        duration: Duration::from_millis(5),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };
    let mut sched =
        SimScheduler::new(scenario.clone()).with_ring(VirtualSharedMemory::new("ring0", 256, 128));
    sched.add_nic(nic_for(&scenario));
    sched.add_consumer(VirtualConsumer::new("c0"));
    let stats = sched.run().unwrap();
    assert!(stats.slots_written > 0);
    assert_eq!(stats.slots_consumed, stats.slots_written);
}

#[test]
fn metrics_collector_receives_samples() {
    let scenario = Scenario {
        duration: Duration::from_millis(3),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };
    let collector = CountingCollector::new().shared();
    let nic = VirtualNic::with_metrics(
        VirtualNicConfig {
            traffic: scenario.traffic.clone(),
            fault: scenario.fault.clone(),
            ..VirtualNicConfig::default()
        },
        NullEventSink,
        collector.clone() as std::sync::Arc<dyn MetricsCollector>,
    );
    let mut sched = SimScheduler::new(scenario)
        .with_metrics(collector.clone() as std::sync::Arc<dyn MetricsCollector>);
    sched.add_nic(nic);
    sched.run().unwrap();
    assert!(
        collector.calls.load(Ordering::Relaxed) > 0,
        "expected metric samples"
    );
}

#[test]
fn educational_single_step_inspects_batch() {
    use flyby_simulator::EduControls;

    let scenario = Scenario {
        duration: Duration::from_millis(5),
        tick_ns: 1_000_000,
        ..Scenario::constant_rate()
    };
    let mut sched = SimScheduler::new(scenario.clone()).with_edu(EduControls {
        paused: true,
        ..EduControls::default()
    });
    sched.add_nic(nic_for(&scenario));
    sched.run().unwrap();
    assert!(sched.step().unwrap());
    assert!(sched.last_batch().is_some());
    let stats = sched.finish_run().unwrap();
    assert_eq!(stats.ticks, 1);
}

#[test]
fn gaussian_rate_scenario_runs() {
    let scenario = Scenario {
        duration: Duration::from_millis(20),
        tick_ns: 1_000_000,
        ..Scenario::gaussian_rate()
    };
    let mut sched = SimScheduler::new(scenario.clone());
    sched.add_nic(nic_for(&scenario));
    let stats = sched.run().unwrap();
    assert!(stats.packets_generated > 0);
    assert!(stats.ticks > 0);
}

#[test]
fn protocol_quotes_scenario_runs() {
    let scenario = Scenario {
        duration: Duration::from_millis(10),
        tick_ns: 1_000_000,
        ..Scenario::protocol_quotes()
    };
    let mut sched = SimScheduler::new(scenario.clone());
    sched.add_nic(nic_for(&scenario));
    let stats = sched.run().unwrap();
    assert!(stats.packets_generated > 0);
}

#[test]
fn pcap_source_replays_through_scheduler() {
    use flyby_simulator::{PcapConfig, PcapSource, write_pcap_bytes};
    use flyby_storage::ReplayMode;

    let bytes = write_pcap_bytes(&[
        (0, &[0xAAu8; 64]),
        (2_000_000, &[0xBBu8; 64]),
        (4_000_000, &[0xCCu8; 32]),
    ]);
    let packets = flyby_simulator::parse_pcap(&bytes).unwrap();
    let src = PcapSource::new(
        packets,
        PcapConfig {
            replay: ReplayMode::FullSpeed,
            ..PcapConfig::default()
        },
        NullEventSink,
    )
    .unwrap();

    let scenario = Scenario {
        name: "pcap_test".into(),
        description: "pcap integration".into(),
        duration: Duration::from_millis(10),
        tick_ns: 1_000_000,
        ..Scenario::default()
    };
    let mut sched = SimScheduler::new(scenario).with_ring(VirtualSharedMemory::new("r0", 64, 128));
    sched.add_nic(src);
    sched.add_consumer(VirtualConsumer::new("c0"));
    let stats = sched.run().unwrap();
    assert_eq!(stats.packets_generated, 3);
    assert_eq!(stats.slots_written, 3);
    assert_eq!(stats.slots_consumed, 3);
}

#[test]
fn fixture_tiny_3pkt_loads() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/tiny_3pkt.pcap");
    let packets = flyby_simulator::load_pcap(path).expect("fixture present; run gen_pcap_fixtures");
    assert_eq!(packets.len(), 3);
    assert_eq!(packets[0].timestamp_ns, 0);
    assert_eq!(packets[1].timestamp_ns, 1_000_000);
    assert_eq!(packets[2].timestamp_ns, 5_000_000);
}

#[test]
fn fixture_udp_quotes_and_ns() {
    let quotes = flyby_simulator::load_pcap(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/udp_quotes.pcap"
    ))
    .unwrap();
    assert_eq!(quotes.len(), 20);
    // Ethernet/IP/UDP + 34-byte quote
    assert_eq!(quotes[0].data.len(), 42 + 34);

    let ns = flyby_simulator::load_pcap(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/ns_timestamps.pcap"
    ))
    .unwrap();
    assert_eq!(ns.len(), 3);
    assert_eq!(ns[1].timestamp_ns, 250);
    assert_eq!(ns[2].timestamp_ns, 1_500);
}

#[test]
fn fixture_empty_pcap() {
    let packets =
        flyby_simulator::load_pcap(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/empty.pcap"))
            .unwrap();
    assert!(packets.is_empty());
}
