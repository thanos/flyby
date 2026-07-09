//! The [`PreProcessor`] trait: enrichment and transformation.
//!
//! A preprocessor runs after decoding and before placement. It is the
//! natural home for normalization, enrichment, filtering, and any
//! CPU-bound transform that does not depend on routing decisions.

use crate::{Message, Result};

/// A transformation applied to a decoded message before placement.
///
/// Preprocessors are synchronous and pure-ish: they receive a message
/// and return either a transformed message, a drop decision, or an
/// error. They must not perform I/O; that belongs in a source or sink.
pub trait PreProcessor: Send + Sync {
    /// The message type this preprocessor operates on.
    type Message: Message;

    /// Apply the preprocessing step.
    ///
    /// Returning `Ok(None)` drops the message from the pipeline.
    fn process(&mut self, message: Self::Message) -> Result<Option<Self::Message>>;
}
