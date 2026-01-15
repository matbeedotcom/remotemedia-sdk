//! Error types for RemoteMedia Runtime Core

use thiserror::Error;

use crate::validation::ValidationError;

/// Result type alias for RemoteMedia Runtime Core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types that can occur in the RemoteMedia Runtime Core
#[derive(Debug, Error)]
pub enum Error {
    /// Manifest parsing or validation error
    #[error("Manifest error: {0}")]
    Manifest(String),

    /// Manifest parsing or validation error (alias)
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    /// Pipeline execution error
    #[error("Execution error: {0}")]
    Execution(String),

    /// Invalid input data (type mismatch, validation failure)
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// IPC communication error
    #[error("IPC error: {0}")]
    IpcError(String),

    /// Transport error (for compatibility)
    #[error("Transport error: {0}")]
    Transport(String),

    /// WASM error (for compatibility, not used in core)
    #[error("WASM error: {0}")]
    Wasm(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Invalid input (node-specific)
    #[error("Invalid input: {message}")]
    InvalidInput {
        /// Error message
        message: String,
        /// Node that rejected the input
        node_id: String,
        /// Additional context
        context: String,
    },

    /// Remote pipeline execution error
    #[error("Remote execution failed: {0}")]
    RemoteExecutionFailed(String),

    /// Remote execution timeout
    #[error("Remote execution timeout after {timeout_ms}ms: {context}")]
    RemoteTimeout {
        /// Timeout duration in milliseconds
        timeout_ms: u64,
        /// Additional context
        context: String,
    },

    /// Circuit breaker is open (too many failures)
    #[error("Circuit breaker open for endpoint {endpoint}: {reason}")]
    CircuitBreakerOpen {
        /// Endpoint URL
        endpoint: String,
        /// Reason for circuit breaker activation
        reason: String,
    },

    /// All configured endpoints failed
    #[error("All {count} endpoints failed: {details}")]
    AllEndpointsFailed {
        /// Number of endpoints that failed
        count: usize,
        /// Failure details
        details: String,
    },

    /// Failed to fetch remote manifest
    #[error("Manifest fetch failed from {url}: {reason}")]
    ManifestFetchFailed {
        /// Manifest URL
        url: String,
        /// Failure reason
        reason: String,
    },

    /// Circular dependency detected in remote pipeline references
    #[error("Circular dependency detected: {reason}\nDependency chain: {}", chain.join(" -> "))]
    CircularDependency {
        /// Chain of manifest names/identifiers showing the cycle
        chain: Vec<String>,
        /// Description of the circular dependency
        reason: String,
    },

    /// Generic error
    #[error("{0}")]
    Other(String),

    /// Node parameter validation failed
    #[error("Parameter validation failed: {} error(s)", .0.len())]
    Validation(Vec<ValidationError>),

    // =========================================================================
    // Ingestion errors (spec 028)
    // =========================================================================

    /// File not found for ingestion
    #[error("Ingest file not found: {path}")]
    IngestFileNotFound {
        /// Path that was not found
        path: String,
    },

    /// Invalid URI scheme for ingestion
    #[error("Invalid ingest scheme: {scheme}. Expected one of: {expected:?}")]
    IngestInvalidScheme {
        /// The invalid scheme
        scheme: String,
        /// Expected schemes
        expected: Vec<String>,
    },

    /// Unsupported URI scheme for ingestion (no plugin registered)
    #[error("Unsupported ingest scheme: {scheme}. Available: {available:?}")]
    IngestUnsupportedScheme {
        /// The unsupported scheme
        scheme: String,
        /// Available schemes
        available: Vec<String>,
    },

    /// Media decode error during ingestion
    #[error("Ingest decode error: {message}")]
    IngestDecodeError {
        /// Error message
        message: String,
        /// Optional codec name
        codec: Option<String>,
    },

    /// Connection error during ingestion
    #[error("Ingest connection error: {message}")]
    IngestConnectionError {
        /// Error message
        message: String,
        /// URL that failed to connect
        url: String,
    },

    /// Plugin already registered in ingest registry
    #[error("Ingest plugin already registered: {name}")]
    IngestPluginAlreadyRegistered {
        /// Plugin name
        name: String,
    },

    /// Plugin not found in ingest registry
    #[error("Ingest plugin not found: {name}")]
    IngestPluginNotFound {
        /// Plugin name
        name: String,
    },

    /// Lock error in ingest registry
    #[error("Ingest registry lock error: {message}")]
    IngestLockError {
        /// Error message
        message: String,
    },
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Other(err.to_string())
    }
}

// Note: ort::Error conversion only needed when using ort directly (e.g., speaker-diarization)
// The silero-vad feature now uses voice_activity_detector which handles ort internally
#[cfg(feature = "speaker-diarization")]
impl From<ort::Error> for Error {
    fn from(err: ort::Error) -> Self {
        Error::Execution(format!("ONNX Runtime error: {}", err))
    }
}
