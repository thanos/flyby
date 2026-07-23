//! Render documentation screenshots for the Ratatui TUI dashboard.
//!
//! ```bash
//! cargo run -p flyby-simulator --example render_tui_docs
//! ```
//!
//! Writes SVG captures under `docs/src/images/tui/`.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use flyby_simulator::tui::{render_text_frame, text_frame_to_svg};
use flyby_simulator::Scenario;

fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/src/images/tui")
}

fn write_shot(name: &str, title: &str, scenario: Scenario, steps: usize) {
    let text = render_text_frame(scenario, steps, 112, 30).unwrap_or_else(|e| {
        panic!("render {name}: {e}");
    });
    let dir = docs_dir();
    fs::create_dir_all(&dir).expect("create images dir");

    let txt_path = dir.join(format!("{name}.txt"));
    fs::write(&txt_path, &text).expect("write txt");

    let svg = text_frame_to_svg(&text, title);
    let svg_path = dir.join(format!("{name}.svg"));
    fs::write(&svg_path, svg).expect("write svg");

    println!("wrote {} and {}", txt_path.display(), svg_path.display());
}

fn main() {
    // 1) Fresh paused dashboard.
    write_shot(
        "01-paused-constant-rate",
        "flyby-sim tui constant_rate — paused",
        Scenario {
            duration: Duration::from_millis(50),
            tick_ns: 1_000_000,
            ..Scenario::constant_rate()
        },
        0,
    );

    // 2) After several steps of packet_loss (drops visible in counters).
    write_shot(
        "02-packet-loss-stepped",
        "flyby-sim tui packet_loss — after steps",
        Scenario {
            duration: Duration::from_millis(80),
            tick_ns: 1_000_000,
            ..Scenario::packet_loss()
        },
        25,
    );

    // 3) Queue overflow with tiny ring.
    write_shot(
        "03-queue-overflow",
        "flyby-sim tui queue_overflow — ring pressure",
        Scenario {
            duration: Duration::from_millis(30),
            ..Scenario::queue_overflow()
        },
        40,
    );

    println!("\nEmbed in docs/src/simulator.md as:");
    println!("  ![paused](./images/tui/01-paused-constant-rate.svg)");
}
