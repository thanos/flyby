//! The builder API for composing a FlyBy pipeline.
//!
//! ## Status
//!
//! - [`.run`][FlyByBuilder::run] validates backend selection (skeleton).
//! - [`.run_demo`][FlyByBuilder::run_demo] builds a real
//!   [`SimplePipeline`](crate::pipeline::SimplePipeline):
//!   simulated net source → decoder → fixed placement → shared-memory sink.
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
    /// Presence is recorded for validation. Use [`run_demo`][Self::run_demo]
    /// to pass a concrete decoder into a live pipeline.
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
        Ok(())
    }

    /// Run a minimal end-to-end pipeline via [`SimplePipeline`](crate::pipeline::SimplePipeline):
    /// simulated net source → decoder → fixed placement → shared-memory sink.
    ///
    /// Drives `steps` calls to [`Pipeline::step`](crate::api::Pipeline::step).
    /// Returns the number of messages written to the sink.
    ///
    /// Requires the `memory` feature. The decoder must produce `M` and `M`
    /// must implement [`crate::api::Encode`].
    #[cfg(feature = "memory")]
    pub fn run_demo<M, D>(self, decoder: D, steps: usize) -> Result<u64>
    where
        M: crate::api::Message + crate::api::Encode + 'static,
        D: Decoder<Output = M> + 'static,
    {
        use crate::api::{Lifecycle, Pipeline, SinkId, StepOutcome};
        use crate::pipeline::{
            FixedPlacement, IdentityPreProcessor, NetworkBatchSource, SimplePipeline,
        };
        use flyby_memory::SharedMemorySink;
        use flyby_net::{SimNetConfig, SimulatedNetSource};

        if !self.use_memory {
            return Err(Error::config("run_demo requires .memory()"));
        }
        if !self.has_source && !self.use_simulator {
            return Err(Error::config("run_demo requires .source() or .simulator()"));
        }

        let src = SimulatedNetSource::try_new(SimNetConfig {
            batch_size: 4,
            payload_size: 32,
            ..SimNetConfig::default()
        })?;
        let adapted = NetworkBatchSource::new(src, 8, 2048);
        let sink_id = SinkId::new(1);
        let mut pipe = SimplePipeline::new(
            adapted,
            decoder,
            IdentityPreProcessor::default(),
            FixedPlacement::new(sink_id)?,
        );
        let mem: SharedMemorySink<M> = SharedMemorySink::new(256, 256)?;
        pipe.register_sink(sink_id, Box::new(mem))?;
        pipe.init()?;

        for _ in 0..steps {
            match pipe.step_outcome()? {
                StepOutcome::Progress | StepOutcome::Idle | StepOutcome::BackPressured => {}
                StepOutcome::Exhausted => break,
            }
        }

        let written = pipe.messages_out();
        pipe.shutdown()?;
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

    #[cfg(feature = "memory")]
    #[test]
    fn run_demo_requires_source_flag() {
        // Decoder that drops everything — still exercises pipeline wiring.
        struct DropDecoder;
        impl Decoder for DropDecoder {
            type Output = flyby_memory::StubMessage;
            fn decode(&mut self, _raw: &[u8]) -> Result<Option<flyby_memory::StubMessage>> {
                Ok(None)
            }
        }
        let err = FlyBy::builder()
            .memory()
            .run_demo(DropDecoder, 1)
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
    }

    #[cfg(feature = "memory")]
    #[test]
    fn run_demo_pipeline_runs() {
        struct DropDecoder;
        impl Decoder for DropDecoder {
            type Output = flyby_memory::StubMessage;
            fn decode(&mut self, _raw: &[u8]) -> Result<Option<flyby_memory::StubMessage>> {
                Ok(None)
            }
        }
        let written = FlyBy::builder()
            .source()
            .memory()
            .run_demo(DropDecoder, 8)
            .unwrap();
        assert_eq!(written, 0);
    }
}
