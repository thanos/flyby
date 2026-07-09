//! Minimal FlyBy pipeline example.
//!
//! Demonstrates the target builder style from the specification:
//!
//! ```text
//! FlyBy::builder().source().memory().placement().run::<M>()?;
//! ```
//!
//! Run with:
//!
//! ```sh
//! cargo run -p flyby-examples --bin hello_pipeline
//! ```

use flyby::prelude::*;

fn main() -> Result<()> {
    FlyBy::builder().source().memory().placement().run::<()>()?;
    println!("flyby hello_pipeline: builder accepted the configuration");
    Ok(())
}
