//! Error types for RemoteMedia Runtime

use thiserror::Error;

/// Result type alias for RemoteMedia Runtime operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types that can occur in the RemoteMedia Runtime
#[derive(Error, Debug)]
pub enum Error {
    /// Manifest parsing or validation error
    #[error("Manifest error: {0}")]
    Manifest(String),

    /// Pipeline execution error
    #[error("Execution error: {0}")]
    Execution(String),

    /// Python VM error
    #[error("Python VM error: {0}")]
    PythonVm(String),

    /// WASM runtime error
    #[error("WASM error: {0}")]
    Wasm(String),

    /// Transport error (gRPC, WebRTC)
    #[error("Transport error: {0}")]
    Transport(String),

    /// Data marshaling error
    #[error("Marshaling error: {0}")]
    Marshaling(String),

    /// IPC communication error
    #[error("IPC error: {0}")]
    IpcError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Invalid input data (type mismatch, validation failure)
    #[error("Invalid input: {message}")]
    InvalidInput {
        message: String,
        node_id: String,
        context: String,
    },

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Other(err.to_string())
    }
}

#[cfg(feature = "silero-vad")]
impl From<ort::Error> for Error {
    fn from(err: ort::Error) -> Self {
        Error::Execution(format!("ONNX Runtime error: {}", err))
    }
}
