//! Simulator package marker for FlyBy.
//!
//! The real synthetic network source is `flyby_net::SimulatedNetSource`.
//! This package keeps a workspace binary/lib for documentation and future
//! replay tooling. Prefer `flyby::net` / `flyby_net` in application code.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Marker type; use `flyby_net::SimulatedNetSource` for real simulation.
#[derive(Debug, Default)]
pub struct SimulatorSource;

impl SimulatorSource {
    /// Points callers at the real simulator.
    pub fn prefer_net_sim() -> &'static str {
        "use flyby_net::SimulatedNetSource (or flyby::net::SimulatedNetSource)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_smoke() {
        let _ = SimulatorSource;
        assert!(!SimulatorSource::prefer_net_sim().is_empty());
    }
}
