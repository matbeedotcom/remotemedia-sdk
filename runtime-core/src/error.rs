//! Error types for runtime-core

use thiserror::Error;

/// Result type alias for runtime-core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for runtime-core
#[derive(Debug, Error)]
pub enum Error {
    /// Manifest parsing or validation error
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    /// Node execution error
    #[error("Node execution failed: {0}")]
    NodeExecutionFailed(String),

    /// Data validation error
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// General execution error
    #[error("Execution error: {0}")]
    Execution(String),

    /// Session not found
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// Resource limit exceeded
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
