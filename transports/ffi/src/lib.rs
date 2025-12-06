//! FFI bindings for RemoteMedia pipelines (Python and Node.js)
//!
//! This crate provides language bindings for the RemoteMedia runtime-core,
//! enabling both Python and Node.js applications to execute media processing
//! pipelines with Rust acceleration.
//!
//! # Features
//!
//! - `python` (default): Enable Python bindings via PyO3
//! - `napi`: Enable Node.js bindings via napi-rs
//!
//! # Architecture
//!
//! ## Shared modules
//! - **marshal.rs**: Data serialization (requires `python` feature)
//!
//! ## Python-specific (`python` feature)
//! - **api.rs**: Python FFI functions
//! - **numpy_bridge.rs**: Zero-copy numpy array integration
//! - **instance_handler.rs**: Python Node instance execution
//!
//! ## Node.js-specific (`napi` feature)
//! - **napi/mod.rs**: Node.js module entry point
//! - **napi/subscriber.rs**: Zero-copy IPC subscriber
//! - **napi/publisher.rs**: Zero-copy IPC publisher
//! - **napi/sample.rs**: Sample lifecycle management
//!
//! # Usage (Python)
//!
//! ```python
//! import asyncio
//! from remotemedia.runtime import execute_pipeline
//!
//! async def main():
//!     manifest = '{"version": "v1", ...}'
//!     results = await execute_pipeline(manifest)
//!     print(results)
//!
//! asyncio.run(main())
//! ```
//!
//! # Usage (Node.js)
//!
//! ```javascript
//! const { createSession } = require('@remotemedia/native');
//!
//! const session = createSession({ id: 'my-session' });
//! const channel = session.channel('audio_input');
//! const subscriber = channel.createSubscriber();
//!
//! subscriber.onData((sample) => {
//!     const data = sample.toRuntimeData();
//!     console.log('Received:', data.type);
//!     sample.release();
//! });
//! ```

#![warn(clippy::all)]

// Python-specific modules (only compiled with `python` feature)
#[cfg(feature = "python")]
mod api;
#[cfg(feature = "python")]
pub mod instance_handler;
#[cfg(feature = "python")]
pub mod marshal;
#[cfg(feature = "python")]
mod numpy_bridge;

// Node.js-specific modules (only compiled with `napi` feature)
#[cfg(feature = "napi")]
pub mod napi;

// Python module entry point
#[cfg(feature = "python")]
use pyo3::prelude::*;

/// Python module for RemoteMedia Rust Runtime
///
/// Provides async pipeline execution with Rust acceleration
/// Installed as remotemedia.runtime
#[cfg(feature = "python")]
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
    m.add_function(wrap_pyfunction!(api::execute_pipeline_with_instances, m)?)?;
    m.add_function(wrap_pyfunction!(api::get_runtime_version, m)?)?;
    m.add_function(wrap_pyfunction!(api::is_available, m)?)?;

    // Add version as module constant
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
