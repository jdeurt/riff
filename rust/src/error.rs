//! Errors shared by native bindings and the engine.

/// A schema, generation, or provider failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid invocation arguments, independent of the schema contract.
    #[error("{0}")]
    Argument(String),
    /// Invalid or unsupported input contract.
    #[error("{0}")]
    Schema(String),
    /// Bounded generation could not produce a complete result.
    #[error("{0}")]
    Generation(String),
    /// A host-language provider failed.
    #[error("{0}")]
    Provider(String),
}

/// A fallible engine operation.
pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Schema(format!("Invalid JSON contract: {error}"))
    }
}
