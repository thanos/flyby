//! Explicit, typed, contextual errors.
//!
//! FlyBy rejects "stringly typed" errors. Every failure carries a
//! machine-readable [`ErrorKind`] and, where useful, structured context.
//! Errors are recoverable where possible: a stage that fails to decode a
//! single record should be able to surface a [`ErrorKind::Decode`] without
//! tearing down the whole pipeline.

use std::fmt;
use std::io;

/// The kind of failure, independent of any surrounding context.
///
/// Variants are expected to grow as backends are introduced. New variants
/// require a note in the changelog and, for breaking changes, an ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A source could not be opened or read from.
    Source,
    /// A sink could not be opened or written to (hard failure).
    Sink,
    /// The sink (or stage) is temporarily full; the caller should slow down
    /// and retry. Distinct from [`ErrorKind::Sink`] hard failures.
    BackPressure,
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
    /// A backend capability was requested but not compiled in
    /// (feature flag off). Prefer [`ErrorKind::NotImplemented`] when the
    /// feature is on but the binding is still a stub.
    FeatureNotEnabled,
    /// The backend is compiled in but not yet implemented (stub).
    NotImplemented,
    /// The pipeline was asked to operate outside of a valid lifecycle state.
    Lifecycle,
    /// An underlying I/O failure.
    Io,
    /// A failure that does not fit any other category.
    Other,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            ErrorKind::Source => "source",
            ErrorKind::Sink => "sink",
            ErrorKind::BackPressure => "back-pressure",
            ErrorKind::Decode => "decode",
            ErrorKind::Encode => "encode",
            ErrorKind::Placement => "placement",
            ErrorKind::PreProcess => "preprocess",
            ErrorKind::Config => "config",
            ErrorKind::FeatureNotEnabled => "feature-not-enabled",
            ErrorKind::NotImplemented => "not-implemented",
            ErrorKind::Lifecycle => "lifecycle",
            ErrorKind::Io => "io",
            ErrorKind::Other => "other",
        };
        f.write_str(label)
    }
}

/// The unified error type for all FlyBy stages.
///
/// Carries an [`ErrorKind`], a human-readable message, and an optional
/// chained source error (for I/O and wrapped failures).
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    /// Create a new error from a kind and a message.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    /// Create an error that wraps another error as its source.
    pub fn with_source(
        kind: ErrorKind,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            source: Some(Box::new(source)),
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

    /// Create a [`ErrorKind::BackPressure`] error.
    pub fn back_pressure(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::BackPressure, message)
    }

    /// Create a [`ErrorKind::Decode`] error.
    pub fn decode(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Decode, message)
    }

    /// Create a [`ErrorKind::Encode`] error.
    pub fn encode(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Encode, message)
    }

    /// Create a [`ErrorKind::Placement`] error.
    pub fn placement(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Placement, message)
    }

    /// Create a [`ErrorKind::PreProcess`] error.
    pub fn preprocess(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::PreProcess, message)
    }

    /// Create a [`ErrorKind::Config`] error.
    pub fn config(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Config, message)
    }

    /// Create a [`ErrorKind::FeatureNotEnabled`] error.
    pub fn feature_not_enabled(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::FeatureNotEnabled, message)
    }

    /// Create a [`ErrorKind::NotImplemented`] error.
    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotImplemented, message)
    }

    /// Create a [`ErrorKind::Lifecycle`] error.
    pub fn lifecycle(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Lifecycle, message)
    }

    /// Create a [`ErrorKind::Io`] error.
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Io, message)
    }

    /// Create a [`ErrorKind::Other`] error.
    pub fn other(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Other, message)
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

impl Clone for Error {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            message: self.message.clone(),
            // Source chain is not cloneable; drop it on clone.
            source: None,
        }
    }
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.message == other.message
    }
}

impl Eq for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        let kind = match value.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => ErrorKind::BackPressure,
            _ => ErrorKind::Io,
        };
        let message = value.to_string();
        Self::with_source(kind, message, value)
    }
}

/// Convenience alias used throughout the framework.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_uses_stable_kind_label() {
        let err = Error::source("boom");
        assert_eq!(err.to_string(), "source: boom");
    }

    #[test]
    fn from_io_preserves_source_chain() {
        let io = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err: Error = io.into();
        assert_eq!(err.kind(), ErrorKind::Io);
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn would_block_maps_to_back_pressure() {
        let io = io::Error::new(io::ErrorKind::WouldBlock, "busy");
        let err: Error = io.into();
        assert_eq!(err.kind(), ErrorKind::BackPressure);
    }

    #[test]
    fn not_implemented_helper() {
        let err = Error::not_implemented("stub");
        assert_eq!(err.kind(), ErrorKind::NotImplemented);
    }

    #[test]
    fn all_constructors_and_labels() {
        let cases = [
            (Error::sink("s"), ErrorKind::Sink, "sink"),
            (
                Error::back_pressure("b"),
                ErrorKind::BackPressure,
                "back-pressure",
            ),
            (Error::decode("d"), ErrorKind::Decode, "decode"),
            (Error::encode("e"), ErrorKind::Encode, "encode"),
            (Error::placement("p"), ErrorKind::Placement, "placement"),
            (Error::preprocess("pp"), ErrorKind::PreProcess, "preprocess"),
            (Error::config("c"), ErrorKind::Config, "config"),
            (
                Error::feature_not_enabled("f"),
                ErrorKind::FeatureNotEnabled,
                "feature-not-enabled",
            ),
            (Error::lifecycle("l"), ErrorKind::Lifecycle, "lifecycle"),
            (Error::io("i"), ErrorKind::Io, "io"),
            (Error::other("o"), ErrorKind::Other, "other"),
        ];
        for (err, kind, label) in cases {
            assert_eq!(err.kind(), kind);
            assert!(err.message().chars().next().is_some());
            assert!(err.to_string().starts_with(label));
            let cloned = err.clone();
            assert_eq!(cloned, err);
        }

        let interrupted: Error = io::Error::new(io::ErrorKind::Interrupted, "intr").into();
        assert_eq!(interrupted.kind(), ErrorKind::BackPressure);
    }

    #[test]
    fn with_source_preserves_chain() {
        let err = Error::with_source(ErrorKind::Other, "wrap", io::Error::other("inner"));
        assert!(std::error::Error::source(&err).is_some());
        assert!(err.clone().message() == "wrap");
    }
}
