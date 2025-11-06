//! RemoteMedia Runtime Core - Transport-agnostic execution engine
//!
//! This crate provides the core runtime functionality for executing RemoteMedia
//! pipelines without any transport-specific dependencies.
//!
//! # Architecture
//!
//! Runtime-core is a pure library that:
//! - Defines transport abstractions (`PipelineTransport`, `StreamSession` traits)
//! - Provides execution engine (`PipelineRunner`)
//! - Manages pipeline graphs, node execution, and session routing
//! - Has ZERO dependencies on transport crates (no tonic, prost, pyo3, etc.)
//!
//! Transport implementations (gRPC, FFI, WebRTC) are separate crates that:
//! - Depend on `remotemedia-runtime-core`
//! - Implement the `PipelineTransport` trait
//! - Handle their own serialization formats
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_runtime_core::transport::PipelineRunner;
//! use remotemedia_runtime_core::transport::TransportData;
//! use remotemedia_runtime_core::data::RuntimeData;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let runner = PipelineRunner::new()?;
//!
//!     let manifest = Arc::new(load_manifest()?);
//!     let input = TransportData::new(RuntimeData::Text("hello".into()));
//!
//!     let output = runner.execute_unary(manifest, input).await?;
//!     println!("Result: {:?}", output.data);
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::arc_with_non_send_sync)] // iceoryx2 types are intentionally !Send

// Transport abstraction layer (NEW in Phase 2)
pub mod transport;

// Re-export core modules from existing runtime
// NOTE: For Phase 2, these are stub re-exports
// In later phases, we'll copy the actual implementations from runtime/

/// Data types module (RuntimeData, AudioBuffer, etc.)
pub mod data {
    //! Core data types
    //!
    //! NOTE: This is a stub for Phase 2. In Phase 4/5, this will contain
    //! the actual RuntimeData types copied from runtime/src/data/

    /// Placeholder RuntimeData for Phase 2
    #[derive(Debug, Clone, PartialEq)]
    pub enum RuntimeData {
        /// Audio data
        Audio {
            /// Audio samples (f32 PCM)
            samples: Vec<f32>,
            /// Sample rate in Hz
            sample_rate: u32,
            /// Number of channels (1=mono, 2=stereo)
            channels: u32,
        },
        /// Text data
        Text(String),
        /// Binary data
        Binary(Vec<u8>),
    }

    impl RuntimeData {
        /// Get the type of this data
        pub fn data_type(&self) -> &str {
            match self {
                RuntimeData::Audio { .. } => "audio",
                RuntimeData::Text(_) => "text",
                RuntimeData::Binary(_) => "binary",
            }
        }
    }
}

/// Manifest parsing module
pub mod manifest {
    //! Pipeline manifest parsing
    //!
    //! NOTE: This is a stub for Phase 2. In Phase 4/5, this will contain
    //! the actual Manifest types copied from runtime/src/manifest/

    use serde::{Deserialize, Serialize};

    /// Placeholder Manifest for Phase 2
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Manifest {
        /// Manifest version
        pub version: String,
        /// Pipeline nodes
        pub nodes: Vec<serde_json::Value>,
        /// Node connections
        pub connections: Vec<serde_json::Value>,
    }

    impl Manifest {
        /// Parse manifest from JSON string
        pub fn from_json(json: &str) -> crate::Result<Self> {
            serde_json::from_str(json)
                .map_err(|e| crate::Error::InvalidManifest(format!("Parse error: {}", e)))
        }
    }
}

// Error types
mod error;
pub use error::{Error, Result};

/// Initialize the RemoteMedia runtime core
///
/// This should be called once at startup to initialize logging.
pub fn init() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("RemoteMedia Runtime Core initialized");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        // Should not panic
        init().ok();
    }
}
