//! CLI entry for the flyby-simulator package.
//!
//! Prefer `flyby_net::SimulatedNetSource` for real synthetic traffic.

fn main() {
    println!(
        "flyby-simulator: marker package. {}",
        flyby_simulator::SimulatorSource::prefer_net_sim()
    );
}
