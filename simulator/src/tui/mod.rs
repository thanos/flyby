//! Ratatui dashboard for the FlyBy simulator.
//!
//! Visualises NIC / ring occupancy, throughput, tick latency, event flow,
//! replay/clock status, and educational controls (pause / single-step).
//!
//! Enable with the `tui` feature (on by default):
//!
//! ```bash
//! cargo run -p flyby-simulator --bin flyby-sim -- tui constant_rate
//! ```
//!
//! ## Keys
//!
//! | Key | Action |
//! |---|---|
//! | `Space` | Pause / resume auto-run |
//! | `s` / `→` | Single-step one tick |
//! | `+` / `-` | Faster / slower auto-run |
//! | `r` | Restart scenario |
//! | `q` / `Esc` | Quit |

#![cfg(feature = "tui")]

mod app;
mod snapshot;
mod ui;

pub use app::{run_dashboard, run_dashboard_compiled};
pub use snapshot::{render_text_frame, text_frame_to_svg};
