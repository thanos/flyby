//! The builder API for composing a FlyBy pipeline.
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
//!
//! Each backend selector (`.memory()`, `.af_xdp()`, `.dpdk()`,
//! `.io_uring()`, `.spdk()`, `.simulator()`) is only available when the
//! corresponding feature flag is enabled. Calling a selector that is not
//! compiled in is a compile error with a clear "method not found"
//! message, rather than a silent runtime no-op.
//!
//! The builder is a skeleton at this stage: it records the requested
//! configuration and validates feature flags, but does not yet wire up
//! real stages. Subsequent parts of the specification fill in the
//! concrete source / sink / placement constructors.

use crate::api::{Error, ErrorKind, Result};

/// The entry point for the FlyBy builder API.
///
/// Construct with [`FlyBy::builder`].
#[derive(Debug, Default)]
pub struct FlyBy;

impl FlyBy {
    /// Begin building a new pipeline.
    pub fn builder() -> FlyByBuilder {
        FlyByBuilder::default()
    }
}

/// A fluent builder for a FlyBy pipeline.
///
/// Each method records intent and returns `self` for chaining. The
/// pipeline is materialized by [`FlyByBuilder::run`].
#[derive(Debug, Default)]
pub struct FlyByBuilder {
    use_memory: bool,
    use_af_xdp: bool,
    use_dpdk: bool,
    use_io_uring: bool,
    use_spdk: bool,
    use_simulator: bool,
}

impl FlyByBuilder {
    /// Placeholder for the source selector.
    ///
    /// The concrete source constructors arrive with the networking /
    /// simulator parts of the specification. Kept here so the builder
    /// chain in the documentation compiles today.
    pub fn source(self) -> Self {
        self
    }

    /// Placeholder for the placement selector.
    pub fn placement(self) -> Self {
        self
    }

    /// Select the shared-memory sink.
    ///
    /// Available only when the `memory` feature is enabled (it is on by
    /// default).
    #[cfg(feature = "memory")]
    pub fn memory(mut self) -> Self {
        self.use_memory = true;
        self
    }

    /// Select the AF_XDP source.
    ///
    /// Available only when the `af_xdp` feature is enabled.
    #[cfg(feature = "af_xdp")]
    pub fn af_xdp(mut self) -> Self {
        self.use_af_xdp = true;
        self
    }

    /// Select the DPDK source.
    ///
    /// Available only when the `dpdk` feature is enabled.
    #[cfg(feature = "dpdk")]
    pub fn dpdk(mut self) -> Self {
        self.use_dpdk = true;
        self
    }

    /// Select the io_uring storage backend.
    ///
    /// Available only when the `io_uring` feature is enabled.
    #[cfg(feature = "io_uring")]
    pub fn io_uring(mut self) -> Self {
        self.use_io_uring = true;
        self
    }

    /// Select the SPDK storage backend.
    ///
    /// Available only when the `spdk` feature is enabled.
    #[cfg(feature = "spdk")]
    pub fn spdk(mut self) -> Self {
        self.use_spdk = true;
        self
    }

    /// Select the in-process simulator source.
    ///
    /// Available only when the `simulator` feature is enabled.
    #[cfg(feature = "simulator")]
    pub fn simulator(mut self) -> Self {
        self.use_simulator = true;
        self
    }

    /// Materialize and run the pipeline for the given message type.
    ///
    /// This is a skeleton: it validates that at least one backend was
    /// selected and then returns. Real wiring lands with the memory,
    /// networking, and storage parts of the specification.
    pub fn run<M>(self) -> Result<()> {
        if !self.use_memory
            && !self.use_af_xdp
            && !self.use_dpdk
            && !self.use_io_uring
            && !self.use_spdk
            && !self.use_simulator
        {
            return Err(Error::new(
                ErrorKind::Config,
                "no sink or source selected; call at least one selector on the builder",
            ));
        }
        // Real wiring happens in later parts of the specification.
        // The type parameter is named so the public signature already
        // matches the target style shown in the spec.
        let _ = core::marker::PhantomData::<M>;
        Ok(())
    }
}
