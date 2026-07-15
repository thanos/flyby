//! The builder API for composing a FlyBy pipeline.
//!
//! ## Status
//!
//! The builder can validate feature selection and, when a decoder is
//! provided with a concrete message type that implements
//! [`Encode`](crate::api::Encode), can drive a minimal **demo pipeline**
//! (simulated source → decode → shared memory sink) via
//! [`FlyByBuilder::run_demo`]. The fluent [`.run`][FlyByBuilder::run]
//! method remains a configuration skeleton for the full multi-stage
//! wiring (placement, multiple sinks, real AF_XDP, …).
//!
//! Target style (from the specification):
//!
//! ```rust,no_run
//! use flyby::prelude::*;
//!
//! fn doc() -> Result<()> {
//!     FlyBy::builder()
//!         .source()
//!         .memory()
//!         .placement()
//!         .run::<()>()?;
//!     Ok(())
//! }
//! ```

use crate::api::{Decoder, Error, ErrorKind, Result};

/// The entry point for the FlyBy builder API.
#[derive(Debug, Default)]
pub struct FlyBy;

impl FlyBy {
    /// Begin building a new pipeline.
    pub fn builder() -> FlyByBuilder {
        FlyByBuilder::default()
    }
}

/// A fluent builder for a FlyBy pipeline.
#[derive(Debug, Default)]
pub struct FlyByBuilder {
    has_source: bool,
    has_decoder: bool,
    has_placement: bool,
    use_memory: bool,
    use_af_xdp: bool,
    use_dpdk: bool,
    use_io_uring: bool,
    use_spdk: bool,
    use_simulator: bool,
}

impl FlyByBuilder {
    /// Mark that a source will be attached (placeholder for concrete sources).
    pub fn source(mut self) -> Self {
        self.has_source = true;
        self
    }

    /// Pair a decoder with the source.
    ///
    /// Presence is recorded for validation. Concrete decoder wiring for
    /// [`.run`][Self::run] lands with full pipeline composition; use
    /// [`run_demo`][Self::run_demo] for an end-to-end smoke path today.
    pub fn decoder<D: Decoder>(mut self, _decoder: D) -> Self {
        self.has_decoder = true;
        self
    }

    /// Mark that placement will be attached (placeholder).
    pub fn placement(mut self) -> Self {
        self.has_placement = true;
        self
    }

    /// Select the shared-memory sink.
    #[cfg(feature = "memory")]
    pub fn memory(mut self) -> Self {
        self.use_memory = true;
        self
    }

    /// Select the AF_XDP source (stub until implemented).
    #[cfg(feature = "af_xdp")]
    pub fn af_xdp(mut self) -> Self {
        self.use_af_xdp = true;
        self
    }

    /// Select the DPDK source (stub until implemented).
    #[cfg(feature = "dpdk")]
    pub fn dpdk(mut self) -> Self {
        self.use_dpdk = true;
        self
    }

    /// Select the io_uring storage backend (stub until implemented).
    #[cfg(feature = "io_uring")]
    pub fn io_uring(mut self) -> Self {
        self.use_io_uring = true;
        self
    }

    /// Select the SPDK storage backend (stub until implemented).
    #[cfg(feature = "spdk")]
    pub fn spdk(mut self) -> Self {
        self.use_spdk = true;
        self
    }

    /// Select the in-process simulator source.
    #[cfg(feature = "simulator")]
    pub fn simulator(mut self) -> Self {
        self.use_simulator = true;
        self
    }

    fn has_any_backend(&self) -> bool {
        self.use_memory
            || self.use_af_xdp
            || self.use_dpdk
            || self.use_io_uring
            || self.use_spdk
            || self.use_simulator
    }

    /// Validate configuration without running stages.
    ///
    /// Requires at least one backend selector. Does not construct a live
    /// pipeline; see [`run_demo`][Self::run_demo] for an executable path.
    pub fn run<M>(self) -> Result<()> {
        let _ = core::marker::PhantomData::<M>;
        if !self.has_any_backend() {
            return Err(Error::new(
                ErrorKind::Config,
                "no sink or source selected; call at least one selector on the builder",
            ));
        }
        // Prefer coherent combinations: if only sources are selected without
        // a sink, still accept for skeleton validation but warn via message
        // when memory is available and unused — kept silent for now.
        Ok(())
    }

    /// Run a minimal end-to-end demo: simulated net source → decoder →
    /// shared-memory sink for `steps` pipeline steps.
    ///
    /// Requires the `memory` feature. Uses [`flyby_net::SimulatedNetSource`]
    /// with a small batch size. The decoder must produce `M` and `M` must
    /// implement [`crate::api::Encode`].
    #[cfg(feature = "memory")]
    pub fn run_demo<M, D>(self, mut decoder: D, steps: usize) -> Result<u64>
    where
        M: crate::api::Message + crate::api::Encode,
        D: Decoder<Output = M>,
    {
        use crate::api::{Lifecycle, Sink};
        use flyby_memory::SharedMemorySink;
        use flyby_net::{NetworkSource, RawBatch, SimNetConfig, SimulatedNetSource};

        if !self.use_memory && !self.use_simulator && !self.has_source {
            // Allow demo if memory is compiled; still require explicit intent.
            return Err(Error::config(
                "run_demo requires .memory() and a source (.source() or .simulator())",
            ));
        }
        if !self.use_memory {
            return Err(Error::config("run_demo requires .memory()"));
        }

        let mut src = SimulatedNetSource::try_new(SimNetConfig {
            batch_size: 4,
            payload_size: 32,
            ..SimNetConfig::default()
        })?;
        src.init()?;

        let mut sink: SharedMemorySink<M> = SharedMemorySink::new(256, 256)?;
        sink.init()?;

        let mut batch = RawBatch::new(8, 2048);
        let mut written = 0u64;

        for _ in 0..steps {
            batch.reset(2048);
            let n = src.poll_batch(&mut batch)?;
            if n == 0 {
                continue;
            }
            for (data, _meta) in batch.packets() {
                // Demo path: if the decoder expects a custom wire format,
                // callers supply it; for raw sim frames, many decoders will
                // return Ok(None). We still exercise the source and sink.
                if let Some(msg) = decoder.decode(data)? {
                    match sink.write(&msg) {
                        Ok(()) => written += 1,
                        Err(e) if e.kind() == ErrorKind::BackPressure => break,
                        Err(e) => return Err(e),
                    }
                }
            }
        }

        sink.shutdown()?;
        src.shutdown()?;
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_without_backend_errors() {
        let err = FlyBy::builder().run::<()>().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
    }

    #[cfg(feature = "memory")]
    #[test]
    fn run_with_memory_ok() {
        FlyBy::builder().memory().run::<()>().unwrap();
    }
}
