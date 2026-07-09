//! The [`Message`] model: typed records flowing downstream.
//!
//! A message is the unit of work inside a FlyBy pipeline. It carries a
//! schema identifier, a timestamp, optional metadata, and the typed
//! payload itself.
//!
//! The framework targets fixed-width messages first; variable-length
//! payloads will be introduced once the fixed-width path is measured and
//! stable.

use core::fmt;

/// An opaque identifier for a message schema.
///
/// Backends and decoders use this to dispatch to the correct parser /
/// encoder pair without re-inspecting the payload.
pub trait SchemaId: Copy + Eq + fmt::Debug + Send + Sync + 'static {
    /// A stable, human-readable name for the schema.
    fn name(&self) -> &'static str;
}

/// Default schema identifier backed by a u16 numeric id.
///
/// Sufficient for early-stage work; production deployments are expected
/// to plug in their own schema-registry-backed identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefaultSchemaId(pub u16);

impl SchemaId for DefaultSchemaId {
    fn name(&self) -> &'static str {
        "default"
    }
}

/// A monotonically meaningful timestamp.
///
/// Stored as nanoseconds since the UNIX epoch to match the resolution of
/// modern hardware timestamps. Sources that only have coarser clocks
/// should zero-fill the low bits rather than lie about precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Construct a timestamp from nanoseconds since the epoch.
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// The raw nanosecond value.
    pub const fn as_nanos(&self) -> u64 {
        self.0
    }
}

/// Per-message metadata: provenance, sequence numbers, flags.
///
/// Deliberately small and `Copy` so it can travel alongside a message
/// without heap traffic. Richer context lives in user-defined extensions
/// on the message payload itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Monotonic sequence number assigned by the source.
    pub sequence: u64,
    /// Non-zero when the source marks this message as suspect.
    pub suspect: bool,
}

/// A typed record flowing through the pipeline.
///
/// Each message has:
///
/// - a schema identifier ([`SchemaId`]),
/// - a parser and encoder (provided by the concrete type, not the trait),
/// - metadata ([`Metadata`]),
/// - a timestamp ([`Timestamp`]),
/// - optional user extensions (on the concrete type).
///
/// Implementors should be cheap to move and `Send` so that messages can
/// cross thread boundaries during placement.
pub trait Message: Send + Sync + 'static {
    /// The schema identifier type used by this message family.
    type Schema: SchemaId;

    /// Returns this message's schema identifier.
    fn schema_id(&self) -> Self::Schema;

    /// Returns the timestamp assigned to this message.
    fn timestamp(&self) -> Timestamp;

    /// Returns the metadata associated with this message.
    fn metadata(&self) -> Metadata;
}
