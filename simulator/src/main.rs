//! `flyby-sim` CLI: run named scenarios, replay pcap, or open the TUI dashboard.
//!
//! Usage:
//!
//! ```text
//! flyby-sim [scenario]
//! flyby-sim tui [scenario]
//! flyby-sim pcap <path> [--full-speed]
//! ```
//!
//! Results are **simulated** and must not be quoted as hardware benchmarks.
//!
//! Medium article demos live under `articles/` —
//! `./scripts/reproduce-article.sh <slug>`.

use flyby_simulator::{
    FaultSpec, PcapConfig, PcapSource, Scenario, SimScheduler, TrafficConfig, VirtualConsumer,
    VirtualNic, VirtualNicConfig, VirtualSharedMemory, events::NullEventSink,
};
use flyby_storage::ReplayMode;

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();

    if args.first().map(String::as_str) == Some("pcap") {
        args.remove(0);
        run_pcap(&args);
        return;
    }

    if args.first().map(String::as_str) == Some("tui") {
        args.remove(0);
        run_tui(&args);
        return;
    }

    let scenario_name = args
        .first()
        .cloned()
        .unwrap_or_else(|| "constant_rate".to_string());
    let scenario = resolve_scenario(&scenario_name);
    run_headless(scenario);
}

fn resolve_scenario(name: &str) -> Scenario {
    Scenario::by_name(name).unwrap_or_else(|| {
        eprintln!(
            "Unknown scenario '{}'. Available: {}",
            name,
            Scenario::builtin_names().join(", ")
        );
        eprintln!("Or: flyby-sim tui [scenario]");
        eprintln!("Or: flyby-sim pcap <path> [--full-speed]");
        eprintln!("Medium articles: ./scripts/reproduce-article.sh --list");
        std::process::exit(1);
    })
}

fn run_tui(args: &[String]) {
    #[cfg(feature = "tui")]
    {
        let scenario_name = args
            .first()
            .cloned()
            .unwrap_or_else(|| "constant_rate".to_string());
        let scenario = resolve_scenario(&scenario_name);
        if let Err(e) = flyby_simulator::tui::run_dashboard(scenario) {
            eprintln!("TUI error: {e}");
            std::process::exit(1);
        }
    }
    #[cfg(not(feature = "tui"))]
    {
        let _ = args;
        eprintln!("TUI support not enabled. Build with `--features tui` (default).");
        std::process::exit(1);
    }
}

fn run_headless(scenario: Scenario) {
    println!(
        "Running scenario '{}': {}",
        scenario.name, scenario.description
    );
    println!("  Duration : {:?}", scenario.duration);
    println!("  Tick     : {} µs", scenario.tick_ns / 1_000);
    println!("  Traffic  : {:?}", scenario.traffic.pattern);
    println!("  Payload  : {:?}", scenario.traffic.payload);
    println!(
        "  Faults   : drop={:.1}% corrupt={:.1}% spike={:.1}%@{}µs",
        scenario.fault.drop_rate * 100.0,
        scenario.fault.corrupt_rate * 100.0,
        scenario.fault.latency_spike_rate * 100.0,
        scenario.fault.latency_spike_ns / 1_000,
    );
    println!("  Note     : results are SIMULATED (not hardware)");

    let nic = VirtualNic::new(
        VirtualNicConfig {
            name: "nic0",
            traffic: TrafficConfig {
                pattern: scenario.traffic.pattern.clone(),
                payload_size: scenario.traffic.payload_size,
                batch_size: scenario.traffic.batch_size,
                payload: scenario.traffic.payload.clone(),
            },
            fault: FaultSpec {
                drop_rate: scenario.fault.drop_rate,
                corrupt_rate: scenario.fault.corrupt_rate,
                latency_spike_rate: scenario.fault.latency_spike_rate,
                latency_spike_ns: scenario.fault.latency_spike_ns,
            },
            fault_seed: 0,
            udp_dst_port: 9000,
        },
        NullEventSink,
    );

    let scenario_label = scenario.name;
    let is_slow = scenario_label == "slow_consumer";
    let is_overflow = scenario_label == "queue_overflow";
    let ring_slots = if is_overflow { 16 } else { 4096 };
    let frame = 42 + scenario.traffic.payload_size.max(1);

    let mut sched =
        SimScheduler::new(scenario).with_ring(VirtualSharedMemory::new("ring0", ring_slots, frame));
    sched.add_nic(nic);
    if is_slow {
        sched.add_consumer(VirtualConsumer::slow("c0", 1));
    } else {
        sched.add_consumer(VirtualConsumer::new("c0"));
    }

    print_stats(sched.run());
}

fn run_pcap(args: &[String]) {
    let path = args.first().cloned().unwrap_or_else(|| {
        eprintln!("Usage: flyby-sim pcap <path> [--full-speed]");
        std::process::exit(1);
    });
    let full_speed = args.iter().any(|a| a == "--full-speed");

    let replay = if full_speed {
        ReplayMode::FullSpeed
    } else {
        ReplayMode::OriginalTiming
    };

    let src = match PcapSource::from_path(
        &path,
        PcapConfig {
            name: "pcap0",
            replay,
            ..PcapConfig::default()
        },
        NullEventSink,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load pcap '{path}': {e}");
            std::process::exit(1);
        }
    };

    println!("Replaying pcap '{path}' ({} packets)", src.len());
    println!(
        "  Replay   : {:?}",
        if full_speed {
            "FullSpeed"
        } else {
            "OriginalTiming"
        }
    );
    println!("  Note     : results are SIMULATED (not hardware)");

    let last_ts = src
        .len()
        .checked_sub(1)
        .and_then(|i| {
            flyby_simulator::load_pcap(&path)
                .ok()
                .map(|p| p[i].timestamp_ns)
        })
        .unwrap_or(0);
    let duration =
        std::time::Duration::from_nanos(last_ts.saturating_add(1_000_000).max(1_000_000));

    let scenario = Scenario {
        name: "pcap_replay",
        description: "Classic pcap replay",
        duration,
        tick_ns: 100_000,
        ..Scenario::default()
    };

    let max_frame = flyby_simulator::load_pcap(&path)
        .ok()
        .and_then(|p| p.into_iter().map(|pkt| pkt.data.len()).max())
        .unwrap_or(2048)
        .max(64);

    let mut sched =
        SimScheduler::new(scenario).with_ring(VirtualSharedMemory::new("ring0", 4096, max_frame));
    sched.add_nic(src);
    sched.add_consumer(VirtualConsumer::new("c0"));
    print_stats(sched.run());
}

fn print_stats(result: flyby_core::Result<flyby_simulator::SimStats>) {
    match result {
        Ok(stats) => {
            println!("\nResults (simulated):");
            println!("  Ticks             : {}", stats.ticks);
            println!("  Packets generated : {}", stats.packets_generated);
            println!("  Packets dropped   : {}", stats.packets_dropped);
            println!("  Packets corrupted : {}", stats.packets_corrupted);
            println!("  Slots written     : {}", stats.slots_written);
            println!("  Slots consumed    : {}", stats.slots_consumed);
            println!("  Ring overflows    : {}", stats.ring_overflows);
            println!("  Virtual clock     : {} ns", stats.clock_ns);
            println!("  Wall-clock time   : {:?}", stats.elapsed);
            if stats.elapsed.as_secs_f64() > 0.0 {
                let pps = stats.packets_generated as f64 / stats.elapsed.as_secs_f64();
                println!(
                    "  Throughput        : {:.0} pps (wall-clock, simulated)",
                    pps
                );
            }
        }
        Err(e) => {
            eprintln!("Simulation error: {e}");
            std::process::exit(1);
        }
    }
}
