//! Error types for model registry

use thiserror::Error;

/// Model registry errors
#[derive(Debug, Error)]
pub enum ModelRegistryError {
    /// Model not found in registry
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    
    /// Out of memory when loading model
    #[error("Out of memory: needed {needed} bytes, available {available}")]
    OutOfMemory { 
        /// Bytes needed
        needed: usize, 
        /// Bytes available
        available: usize 
    },
    
    /// Model already being loaded
    #[error("Model {0} is already being loaded")]
    AlreadyLoading(String),
    
    /// Model loading failed
    #[error("Failed to load model {0}: {1}")]
    LoadFailed(String, String),
    
    /// Invalid model configuration
    #[error("Invalid model configuration: {0}")]
    InvalidConfig(String),
    
    /// Registry is full
    #[error("Registry has reached maximum capacity")]
    RegistryFull,
    
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Other error
    #[error("Model registry error: {0}")]
    Other(String),
}
