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

use pyo3::prelude::*;

pub mod executor;
pub mod manifest;
pub mod nodes;
pub mod transport;
pub mod wasm;
pub mod python;
pub mod cache;
pub mod registry;

mod error;
pub use error::{Error, Result};

/// Initialize the RemoteMedia runtime
///
/// This should be called once at startup to initialize logging and runtime state.
pub fn init() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    tracing::info!("RemoteMedia Runtime initialized");
    Ok(())
}

/// Python FFI entry point
///
/// This module exposes the Rust runtime to Python via PyO3.
#[pymodule]
fn remotemedia_runtime(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(execute_manifest_py, m)?)?;
    Ok(())
}

/// Execute a pipeline from a JSON manifest (Python FFI)
///
/// # Arguments
/// * `manifest_json` - JSON string containing the pipeline manifest
///
/// # Returns
/// JSON string containing the execution results
#[pyfunction]
fn execute_manifest_py(manifest_json: String) -> PyResult<String> {
    // This will be implemented in Phase 1.3
    tracing::info!("Executing manifest from Python FFI");

    // Parse manifest
    let manifest = manifest::parse(&manifest_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid manifest: {}", e)))?;

    // Execute pipeline (placeholder for now)
    tracing::info!("Manifest parsed: {} nodes", manifest.nodes.len());

    // Return placeholder result
    Ok(serde_json::json!({
        "status": "success",
        "message": "Rust runtime executing (Phase 1 WIP)"
    }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        // Should not panic
        init().unwrap();
    }
}
