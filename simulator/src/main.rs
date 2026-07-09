//! FlyBy in-process simulator.
//!
//! Simulation is a first-class development workflow (design principle
//! #5): the simulator lets the pipeline run end to end without any
//! hardware or kernel dependencies. The concrete replay / synthetic
//! source arrives with Part VI of the specification; this binary is a
//! stub that confirms the simulator package builds and links against
//! `flyby-core`.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p flyby-simulator
//! ```

use flyby_core::prelude::*;

fn main() -> Result<()> {
    println!("flyby-simulator: skeleton ready (simulator backend lands in Part VI)");
    Ok(())
}
