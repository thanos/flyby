//! Explicit, typed, contextual errors.
//!
//! FlyBy rejects "stringly typed" errors. Every failure carries a
//! machine-readable [`ErrorKind`] and, where useful, structured context.
//! Errors are recoverable where possible: a stage that fails to decode a
//! single record should be able to surface a [`ErrorKind::Decode`] without
//! tearing down the whole pipeline.

use std::fmt;

/// The kind of failure, independent of any surrounding context.
///
/// Variants are expected to grow as backends are introduced. New variants
/// require a note in the changelog and, for breaking changes, an ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A source could not be opened or read from.
    Source,
    /// A sink could not be opened or written to.
    Sink,
    /// A byte buffer could not be parsed into a typed message.
    Decode,
    /// A typed message could not be encoded into bytes.
    Encode,
    /// A placement / routing decision could not be made.
    Placement,
    /// A preprocessing step failed.
    PreProcess,
    /// The pipeline was configured incorrectly.
    Config,
    /// A backend capability was requested but not enabled.
    FeatureNotEnabled,
    /// The pipeline was asked to operate outside of a valid lifecycle state.
    Lifecycle,
    /// An underlying I/O failure.
    Io,
    /// A failure that does not fit any other category.
    Other,
}

/// The unified error type for all FlyBy stages.
///
/// Carries an [`ErrorKind`] and a human-readable message. Future revisions
/// will add structured context (source line, record id, backend name)
/// without changing the kind taxonomy.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    /// Create a new error from a kind and a message.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Create a [`ErrorKind::Source`] error.
    pub fn source(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Source, message)
    }

    /// Create a [`ErrorKind::Sink`] error.
    pub fn sink(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Sink, message)
    }

    /// Create a [`ErrorKind::Decode`] error.
    pub fn decode(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Decode, message)
    }

    /// Create a [`ErrorKind::Config`] error.
    pub fn config(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Config, message)
    }

    /// Create a [`ErrorKind::FeatureNotEnabled`] error.
    pub fn feature_not_enabled(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::FeatureNotEnabled, message)
    }

    /// Returns the [`ErrorKind`] for this error.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, value.to_string())
    }
}

/// Convenience alias used throughout the framework.
pub type Result<T> = std::result::Result<T, Error>;
