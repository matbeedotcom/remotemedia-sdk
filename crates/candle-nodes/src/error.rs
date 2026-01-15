//! Error types for Candle ML nodes

use thiserror::Error;

/// Result type for Candle node operations
pub type Result<T> = std::result::Result<T, CandleNodeError>;

/// Errors that can occur in Candle ML nodes
#[derive(Error, Debug)]
pub enum CandleNodeError {
    /// Model loading failed
    #[error("Failed to load model '{model}': {message}")]
    ModelLoad {
        model: String,
        message: String,
    },

    /// Model download failed
    #[error("Failed to download model '{model}' from {download_source}: {message}")]
    ModelDownload {
        model: String,
        download_source: String,
        message: String,
    },

    /// Device initialization failed
    #[error("Failed to initialize device '{device}': {message}")]
    DeviceInit {
        device: String,
        message: String,
    },

    /// Inference failed
    #[error("Inference failed for node '{node_id}': {message}")]
    Inference {
        node_id: String,
        message: String,
    },

    /// Invalid input data
    #[error("Invalid input for node '{node_id}': expected {expected}, got {actual}")]
    InvalidInput {
        node_id: String,
        expected: String,
        actual: String,
    },

    /// Input conversion failed
    #[error("Failed to convert input for node '{node_id}': {message}")]
    InputConversion {
        node_id: String,
        message: String,
    },

    /// Output conversion failed
    #[error("Failed to convert output for node '{node_id}': {message}")]
    OutputConversion {
        node_id: String,
        message: String,
    },

    /// Configuration error
    #[error("Invalid configuration for '{node_type}': {message}")]
    Configuration {
        node_type: String,
        message: String,
    },

    /// Cache error
    #[error("Cache error: {message}")]
    Cache {
        message: String,
    },

    /// Tokenizer error
    #[error("Tokenizer error: {message}")]
    Tokenizer {
        message: String,
    },

    /// Generic Candle error wrapper
    #[error("Candle error: {0}")]
    Candle(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// RemoteMedia core error
    #[error("Core error: {0}")]
    Core(#[from] remotemedia_core::Error),
}

impl CandleNodeError {
    /// Create a model load error with context
    pub fn model_load(model: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ModelLoad {
            model: model.into(),
            message: message.into(),
        }
    }

    /// Create an inference error with context
    pub fn inference(node_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Inference {
            node_id: node_id.into(),
            message: message.into(),
        }
    }

    /// Create an invalid input error
    pub fn invalid_input(
        node_id: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::InvalidInput {
            node_id: node_id.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    /// Create a configuration error
    pub fn configuration(node_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Configuration {
            node_type: node_type.into(),
            message: message.into(),
        }
    }
}

#[cfg(feature = "whisper")]
impl From<candle_core::Error> for CandleNodeError {
    fn from(err: candle_core::Error) -> Self {
        Self::Candle(err.to_string())
    }
}

#[cfg(feature = "yolo")]
impl From<candle_core::Error> for CandleNodeError {
    fn from(err: candle_core::Error) -> Self {
        Self::Candle(err.to_string())
    }
}

#[cfg(feature = "llm")]
impl From<candle_core::Error> for CandleNodeError {
    fn from(err: candle_core::Error) -> Self {
        Self::Candle(err.to_string())
    }
}
