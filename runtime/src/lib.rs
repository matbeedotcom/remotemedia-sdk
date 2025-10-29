//! RemoteMedia Runtime - Language-neutral execution engine for distributed AI pipelines
//!
//! This crate provides the core runtime that executes RemoteMedia pipelines.
//! It supports:
//! - Manifest-based pipeline execution
//! - RustPython VM for backward compatibility with Python nodes
//! - WASM sandbox for portable, secure execution
//! - WebRTC and gRPC transports
//! - Automatic capability-based scheduling

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod audio;
pub mod cache;
pub mod capabilities;
pub mod data;
pub mod executor;
pub mod manifest;
pub mod nodes;
pub mod python;
pub mod registry;
pub mod transport;
pub mod wasm;

// gRPC service (only available with grpc-transport feature)
#[cfg(feature = "grpc-transport")]
pub mod grpc_service;

mod error;
pub use error::{Error, Result};

/// Initialize the RemoteMedia runtime
///
/// This should be called once at startup to initialize logging and runtime state.
pub fn init() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("RemoteMedia Runtime initialized");
    Ok(())
}

// Python FFI entry point is now in src/python/ffi.rs
// The _remotemedia_runtime module is defined there

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        // Should not panic
        init().unwrap();
    }
}
