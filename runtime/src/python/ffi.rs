//! Python FFI functions for calling Rust runtime from Python
//!
//! This module provides the bridge between Python and Rust, allowing
//! Python code to execute pipelines using the Rust runtime.

use pyo3::prelude::*;
use pyo3_asyncio::tokio::future_into_py;
use crate::executor::Executor;
use crate::manifest::parse;
use super::marshal::{python_to_json, json_to_python};

/// Execute a pipeline from a JSON manifest
///
/// # Arguments
/// * `manifest_json` - JSON string containing the pipeline manifest
///
/// # Returns
/// Python coroutine that resolves to execution results
///
/// # Example (Python)
/// ```python
/// import asyncio
/// from remotemedia._remotemedia_runtime import execute_pipeline
///
/// async def main():
///     manifest = '{"version": "v1", ...}'
///     results = await execute_pipeline(manifest)
///     print(results)
///
/// asyncio.run(main())
/// ```
#[pyfunction]
pub fn execute_pipeline<'py>(
    py: Python<'py>,
    manifest_json: String,
) -> PyResult<&'py PyAny> {
    future_into_py(py, async move {
        // Parse manifest
        let manifest = parse(&manifest_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to parse manifest: {}", e)
            ))?;

        // Execute
        let executor = Executor::new();
        let result = executor.execute(&manifest).await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Execution failed: {}", e)
            ))?;

        // Convert outputs to Python
        Python::with_gil(|py| {
            json_to_python(py, &result.outputs)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Failed to convert result to Python: {}", e)
                ))
        })
    })
}

/// Execute a pipeline with input data
///
/// # Arguments
/// * `manifest_json` - JSON string containing the pipeline manifest
/// * `input_data` - List of input items to process
///
/// # Returns
/// Python coroutine that resolves to list of results
///
/// # Example (Python)
/// ```python
/// manifest = pipeline.serialize()
/// results = await execute_pipeline_with_input(manifest, [1, 2, 3])
/// ```
#[pyfunction]
pub fn execute_pipeline_with_input<'py>(
    py: Python<'py>,
    manifest_json: String,
    input_data: Vec<PyObject>,
) -> PyResult<&'py PyAny> {
    future_into_py(py, async move {
        // Parse manifest
        let manifest = parse(&manifest_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to parse manifest: {}", e)
            ))?;

        // Convert Python input to Rust Values
        let rust_input: Vec<serde_json::Value> = Python::with_gil(|py| {
            input_data.iter()
                .map(|obj| python_to_json(py, obj))
                .collect::<PyResult<Vec<_>>>()
        })?;

        // Execute
        let executor = Executor::new();
        let result = executor.execute_with_input(&manifest, rust_input).await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Execution failed: {}", e)
            ))?;

        // Convert outputs back to Python
        Python::with_gil(|py| {
            json_to_python(py, &result.outputs)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Failed to convert result to Python: {}", e)
                ))
        })
    })
}

/// Get runtime version information
#[pyfunction]
pub fn get_runtime_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Check if Rust runtime is available
#[pyfunction]
pub fn is_available() -> bool {
    true
}

use pyo3::types::PyList;

/// Python module initialization
#[pymodule]
fn remotemedia_runtime(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(execute_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(execute_pipeline_with_input, m)?)?;
    m.add_function(wrap_pyfunction!(get_runtime_version, m)?)?;
    m.add_function(wrap_pyfunction!(is_available, m)?)?;

    // Add version as module constant
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let version = get_runtime_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_availability() {
        assert!(is_available());
    }
}
