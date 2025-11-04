//! Python FFI functions for calling Rust runtime from Python
//!
//! This module provides the bridge between Python and Rust, allowing
//! Python code to execute pipelines using the Rust runtime.

use super::marshal::{json_to_python, python_to_json};
use crate::executor::Executor;
use crate::manifest::parse;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;

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
pub fn execute_pipeline(
    py: Python<'_>,
    manifest_json: String,
    enable_metrics: Option<bool>,
) -> PyResult<Bound<'_, PyAny>> {
    future_into_py(py, async move {
        // Parse manifest
        let manifest = parse(&manifest_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse manifest: {}",
                e
            ))
        })?;

        // Execute
        let executor = Executor::new();
        let result = executor.execute(&manifest).await.map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Execution failed: {}", e))
        })?;

        // Convert outputs to Python
        // With auto-initialize, Python::attach is available in async contexts
        Python::attach(|py| {
            let outputs_py = json_to_python(py, &result.outputs)?;

            // Include metrics if requested
            if enable_metrics.unwrap_or(false) {
                let metrics_json = result.metrics.to_json();
                let metrics_str = serde_json::to_string(&metrics_json).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to serialize metrics: {}",
                        e
                    ))
                })?;

                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", &outputs_py)?;
                dict.set_item("metrics", metrics_str)?;
                Ok(dict.into_any().unbind())
            } else {
                Ok(outputs_py.unbind())
            }
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
    input_data: Vec<Bound<'py, PyAny>>,
    enable_metrics: Option<bool>,
) -> PyResult<Bound<'py, PyAny>> {
    // Convert input_data to JSON immediately before the async block
    let rust_input: Vec<serde_json::Value> = input_data
        .iter()
        .map(|obj| python_to_json(py, obj))
        .collect::<PyResult<Vec<_>>>()?;

    future_into_py(py, async move {
        // Parse manifest
        let manifest = parse(&manifest_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse manifest: {}",
                e
            ))
        })?;

        // Execute
        let executor = Executor::new();
        let result = executor
            .execute_with_input(&manifest, rust_input)
            .await
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Execution failed: {}",
                    e
                ))
            })?;

        // Convert outputs back to Python
        // With auto-initialize, Python::attach is available in async contexts
        Python::attach(|py| {
            let outputs_py = json_to_python(py, &result.outputs)?;

            // Include metrics if requested
            if enable_metrics.unwrap_or(false) {
                let metrics_json = result.metrics.to_json();
                let metrics_str = serde_json::to_string(&metrics_json).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to serialize metrics: {}",
                        e
                    ))
                })?;

                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", &outputs_py)?;
                dict.set_item("metrics", metrics_str)?;
                Ok(dict.into_any().unbind())
            } else {
                Ok(outputs_py.unbind())
            }
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

/// Get execution metrics as JSON string
///
/// # Arguments
/// * `metrics_json` - JSON string containing serialized PipelineMetrics
///
/// # Returns
/// JSON string with detailed metrics including per-node breakdown
///
/// # Example (Python)
/// ```python
/// metrics_str = get_metrics(metrics_json)
/// metrics = json.loads(metrics_str)
/// print(f"Total duration: {metrics['total_duration_us']}μs")
/// ```
#[pyfunction]
pub fn get_metrics(metrics_json: String) -> PyResult<String> {
    use crate::executor::PipelineMetrics;

    // Deserialize metrics
    let metrics: PipelineMetrics = serde_json::from_str(&metrics_json).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to parse metrics: {}", e))
    })?;

    // Convert to JSON with enhanced formatting
    let json = metrics.to_json();

    // Serialize back to string
    serde_json::to_string(&json).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
            "Failed to serialize metrics: {}",
            e
        ))
    })
}

/// Python module initialization
#[pymodule]
fn remotemedia_runtime(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize tracing on module load
    // Use try_init to avoid panic if already initialized
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    m.add_function(wrap_pyfunction!(execute_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(execute_pipeline_with_input, m)?)?;
    m.add_function(wrap_pyfunction!(get_runtime_version, m)?)?;
    m.add_function(wrap_pyfunction!(is_available, m)?)?;
    m.add_function(wrap_pyfunction!(get_metrics, m)?)?;

    // Register the runtime_data submodule (PyRuntimeData, numpy_to_audio, etc.)
    use crate::python::runtime_data_py::register_runtime_data_module;
    register_runtime_data_module(m.py(), m)?;

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
