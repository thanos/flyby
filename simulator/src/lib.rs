//! Simulator source and replay engine for FlyBy.
//!
//! This is the library target of the simulator package. The concrete
//! synthetic source, replay format, and clock model arrive with Part VI
//! of the specification. For now the crate exposes a minimal marker so
//! the workspace compiles and the binary has a lib to link against.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Placeholder for the future simulator source.
#[derive(Debug, Default)]
pub struct SimulatorSource;
