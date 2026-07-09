#![forbid(unsafe_code)]
#![doc = include_str!("../docs/README.md")]
//!
//! # flyby-memory
//!
//! The shared-memory sink for FlyBy.
//!
//! This is the first production sink and the default backend (enabled by
//! the `memory` feature on the `flyby` facade). The concrete ring-buffer
//! and slot-layout implementation arrives with Part III of the
//! specification; this module currently exposes a stub sink so that the
//! workspace compiles and the builder API is exercised end to end.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

use flyby_core::{Lifecycle, Result, Sink};

/// A stub shared-memory sink.
///
/// Real implementation will back this with a fixed-size ring buffer in a
/// memory-mapped region, with explicit memory ordering and a documented
/// safety boundary. For now it simply accepts and drops messages so the
/// pipeline can be wired up and measured.
#[derive(Debug, Default)]
pub struct SharedMemorySink {
    written: u64,
}

impl SharedMemorySink {
    /// Create a new, empty shared-memory sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of messages handed to [`Sink::write`] so far.
    pub fn written(&self) -> u64 {
        self.written
    }
}

impl Lifecycle for SharedMemorySink {}

impl Sink for SharedMemorySink {
    type Message = crate::stub::StubMessage;

    fn write(&mut self, _message: &Self::Message) -> Result<()> {
        self.written += 1;
        Ok(())
    }
}

/// Placeholder re-exports to keep the crate self-contained until Part III.
mod stub {
    use flyby_core::{DefaultSchemaId, Message, Metadata, Timestamp};

    /// A trivial message type used by the stub sink.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct StubMessage {
        seq: u64,
    }

    impl Message for StubMessage {
        type Schema = DefaultSchemaId;

        fn schema_id(&self) -> Self::Schema {
            DefaultSchemaId(0)
        }

        fn timestamp(&self) -> Timestamp {
            Timestamp(0)
        }

        fn metadata(&self) -> Metadata {
            Metadata {
                sequence: self.seq,
                suspect: false,
            }
        }
    }
}
