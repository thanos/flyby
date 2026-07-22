//! `flyby-sim` CLI: run named scenarios from the command line.
//!
//! Usage:
//!
//! ```text
//! flyby-sim [scenario]
//! ```
//!
//! Where `scenario` is one of:
//!
//! - `constant_rate` (default)
//! - `market_open_burst`
//! - `queue_overflow`
//! - `packet_loss`
//! - `slow_consumer`
//! - `corrupt_packets`
//!
//! Results are **simulated** and must not be quoted as hardware benchmarks.

use flyby_simulator::{
    FaultSpec, Scenario, SimScheduler, TrafficConfig, VirtualConsumer, VirtualNic,
    VirtualNicConfig, VirtualSharedMemory, events::NullEventSink,
};

fn main() {
    let scenario_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "constant_rate".to_string());

    let scenario = match scenario_name.as_str() {
        "constant_rate" => Scenario::constant_rate(),
        "market_open_burst" => Scenario::market_open_burst(),
        "queue_overflow" => Scenario::queue_overflow(),
        "packet_loss" => Scenario::packet_loss(),
        "slow_consumer" => Scenario::slow_consumer(),
        "corrupt_packets" => Scenario::corrupt_packets(),
        other => {
            eprintln!(
                "Unknown scenario '{}'. Available: constant_rate, market_open_burst, queue_overflow, packet_loss, slow_consumer, corrupt_packets",
                other
            );
            std::process::exit(1);
        }
    };

    println!(
        "Running scenario '{}': {}",
        scenario.name, scenario.description
    );
    println!("  Duration : {:?}", scenario.duration);
    println!("  Tick     : {} µs", scenario.tick_ns / 1_000);
    println!("  Traffic  : {:?}", scenario.traffic.pattern);
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
            },
            fault: FaultSpec {
                drop_rate: scenario.fault.drop_rate,
                corrupt_rate: scenario.fault.corrupt_rate,
                latency_spike_rate: scenario.fault.latency_spike_rate,
                latency_spike_ns: scenario.fault.latency_spike_ns,
            },
            fault_seed: 0,
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

    match sched.run() {
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
            eprintln!("Simulation error: {}", e);
            std::process::exit(1);
        }
    }
}
