//! The [`Sink`] trait: terminal destinations.
//!
//! A sink sits at the tail of a FlyBy pipeline. Shared memory is the
//! first production sink, but the abstraction is deliberately generic so
//! that Arrow Flight, Kafka, files, or future transports can be added
//! without changing the pipeline shape.

use crate::{Lifecycle, Message, Result};

/// A terminal destination for decoded messages.
///
/// Implementations are responsible for any serialization required before
/// the message leaves the pipeline. Sinks must respect back-pressure:
/// [`Sink::write`] may return [`crate::Error`] with
/// [`crate::ErrorKind::Sink`] to signal that the pipeline should slow
/// down.
pub trait Sink: Lifecycle {
    /// The message type accepted by this sink.
    type Message: Message;

    /// Write a single message to the sink.
    fn write(&mut self, message: &Self::Message) -> Result<()>;

    /// Flush any buffered messages downstream.
    ///
    /// Default is a no-op; buffering sinks override this.
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
