//! Python FFI transport for RemoteMedia pipelines
//!
//! This crate provides Python bindings for the RemoteMedia runtime-core,
//! enabling Python applications to execute media processing pipelines
//! with Rust acceleration.
//!
//! # Architecture
//!
//! - **api.rs**: Main FFI functions (execute_pipeline, etc.)
//! - **marshal.rs**: Python ↔ JSON conversion
//! - **numpy_bridge.rs**: Zero-copy numpy array integration
//!
//! # Usage (Python)
//!
//! ```python
//! import asyncio
//! from remotemedia_ffi import execute_pipeline
//!
//! async def main():
//!     manifest = '{"version": "v1", ...}'
//!     results = await execute_pipeline(manifest)
//!     print(results)
//!
//! asyncio.run(main())
//! ```

#![warn(clippy::all)]

mod api;
mod marshal;
mod numpy_bridge;

use pyo3::prelude::*;

/// Python module for RemoteMedia Rust Runtime
///
/// Provides async pipeline execution with Rust acceleration
/// Installed as remotemedia.runtime
#[pymodule]
#[pyo3(name = "runtime")]
fn remotemedia_ffi(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize tracing on module load
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    // Add FFI functions from api module
    m.add_function(wrap_pyfunction!(api::execute_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(api::execute_pipeline_with_input, m)?)?;
    m.add_function(wrap_pyfunction!(api::get_runtime_version, m)?)?;
    m.add_function(wrap_pyfunction!(api::is_available, m)?)?;

    // Add version as module constant
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
