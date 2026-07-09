//! Shared criterion benchmark entry points.
//!
//! This module exists so that `cargo bench -p flyby-benches` has a
//! compilable lib target even before any `[[bench]]` harness is enabled.

#![forbid(unsafe_code)]
