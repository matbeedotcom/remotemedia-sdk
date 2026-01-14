//! Python FFI functions for calling Rust runtime from Python
//!
//! This module provides the bridge between Python and Rust, allowing
//! Python code to execute pipelines using the Rust runtime.
//!
//! Uses PipelineExecutor from core for transport-agnostic execution.
//! (Migrated from PipelineRunner per spec 026)

use super::marshal::{python_to_runtime_data, runtime_data_to_python};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use remotemedia_core::{
    data::RuntimeData,
    manifest::Manifest,
    transport::{PipelineExecutor, TransportData},
};
use std::sync::Arc;

/// Map runtime errors to appropriate Python exceptions
///
/// Provides consistent error handling across FFI, with special handling for:
/// - Validation errors -> ValueError with structured error details
/// - Manifest errors -> ValueError
/// - Execution errors -> RuntimeError
fn map_runtime_error(e: remotemedia_core::Error) -> PyErr {
    match e {
        remotemedia_core::Error::Validation(ref validation_errors) => {
            // Format validation errors as structured JSON for Python consumers
            let errors_json = serde_json::to_string_pretty(validation_errors)
                .unwrap_or_else(|_| e.to_string());
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Parameter validation failed ({} error(s)):\n{}",
                validation_errors.len(),
                errors_json
            ))
        }
        remotemedia_core::Error::Manifest(msg)
        | remotemedia_core::Error::InvalidManifest(msg) => {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid manifest: {}", msg))
        }
        remotemedia_core::Error::InvalidData(msg) => {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid data: {}", msg))
        }
        remotemedia_core::Error::InvalidInput { message, node_id, .. } => {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid input for node '{}': {}",
                node_id, message
            ))
        }
        _ => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Execution failed: {}", e)),
    }
}

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
        let manifest: Manifest = serde_json::from_str(&manifest_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse manifest: {}",
                e
            ))
        })?;
        let manifest = Arc::new(manifest);

        // Create PipelineExecutor (spec 026 migration)
        let executor = PipelineExecutor::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create executor: {}",
                e
            ))
        })?;

        // Execute using PipelineExecutor (no input data for basic execution)
        let input = TransportData::new(RuntimeData::Text(String::new()));
        let output = executor
            .execute_unary(manifest, input)
            .await
            .map_err(map_runtime_error)?;

        // Convert output to Python
        Python::attach(|py| {
            // Use runtime_data_to_python for direct conversion (zero-copy for numpy!)
            // This avoids JSON serialization and converts RuntimeData::Numpy directly to numpy arrays
            let outputs_py = runtime_data_to_python(py, &output.data)?;

            // Include metrics if requested - now includes scheduler metrics from PipelineExecutor
            if enable_metrics.unwrap_or(false) {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", &outputs_py)?;
                dict.set_item("metrics", "{}")?; // Placeholder - metrics exposed via executor.prometheus_metrics()
                Ok(dict.into_any().unbind())
            } else {
                Ok(outputs_py)  // runtime_data_to_python already returns PyObject (unbound)
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
    // Convert input_data to RuntimeData directly (zero-copy for numpy!)
    let rust_input: Vec<RuntimeData> = input_data
        .iter()
        .map(|obj| python_to_runtime_data(py, obj))
        .collect::<PyResult<Vec<_>>>()?;

    future_into_py(py, async move {
        // Parse manifest
        let manifest: Manifest = serde_json::from_str(&manifest_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse manifest: {}",
                e
            ))
        })?;
        let manifest = Arc::new(manifest);

        // Create PipelineExecutor (spec 026 migration)
        let executor = PipelineExecutor::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create executor: {}",
                e
            ))
        })?;

        // Use first input item or empty text
        let input_data = if let Some(first) = rust_input.first() {
            first.clone()
        } else {
            RuntimeData::Text(String::new())
        };

        // Execute using PipelineExecutor - uses map_runtime_error for proper validation error handling
        let input = TransportData::new(input_data);
        let output = executor
            .execute_unary(manifest, input)
            .await
            .map_err(map_runtime_error)?;

        // Convert output to Python
        Python::attach(|py| {
            // Use runtime_data_to_python for direct conversion (zero-copy for numpy!)
            // This avoids JSON serialization and converts RuntimeData::Numpy directly to numpy arrays
            let outputs_py = runtime_data_to_python(py, &output.data)?;

            // Include metrics if requested - now includes scheduler metrics from PipelineExecutor
            if enable_metrics.unwrap_or(false) {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", &outputs_py)?;
                dict.set_item("metrics", "{}")?; // Placeholder - metrics exposed via executor.prometheus_metrics()
                Ok(dict.into_any().unbind())
            } else {
                Ok(outputs_py)  // runtime_data_to_python already returns PyObject (unbound)
            }
        })
    })
}

/// Execute pipeline directly with Python Node instances (Feature 011)
///
/// This function bypasses the node registry and executes Node instances
/// directly using InstanceExecutor. This enables custom Python nodes to
/// run without registration.
///
/// # Arguments
/// * `node_instances` - List of Python Node instances to execute in sequence
/// * `input_data` - Optional input data for the first node
///
/// # Returns
/// Python coroutine that resolves to execution results
#[pyfunction]
pub fn execute_pipeline_with_instances<'py>(
    py: Python<'py>,
    node_instances: Vec<Bound<'py, PyAny>>,
    input_data: Option<Bound<'py, PyAny>>,
    enable_metrics: Option<bool>,
) -> PyResult<Bound<'py, PyAny>> {
    use super::instance_handler::InstanceExecutor;
    use super::marshal::{python_to_runtime_data, runtime_data_to_python};

    // Convert to Py<PyAny> before async block (Bound is not Send)
    let node_refs: Vec<Py<PyAny>> = node_instances
        .into_iter()
        .map(|node| node.unbind())
        .collect();

    let input_ref: Option<RuntimeData> = if let Some(input) = input_data {
        Some(python_to_runtime_data(py, &input)?)
    } else {
        None
    };

    future_into_py(py, async move {
        // Convert node instances to InstanceExecutor wrappers
        let executors: Vec<InstanceExecutor> = node_refs
            .into_iter()
            .enumerate()
            .map(|(i, node)| {
                let node_id = format!("instance_{}", i);
                InstanceExecutor::new(node, node_id)
            })
            .collect::<PyResult<Vec<_>>>()?;

        if executors.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Cannot execute empty pipeline",
            ));
        }

        // Initialize all nodes
        for executor in &executors {
            executor.initialize()?;
        }

        // Get initial input data
        let mut current_data = input_ref.unwrap_or(RuntimeData::Text(String::new()));

        // Execute nodes in sequence
        for executor in &executors {
            let outputs = executor.process(current_data)?;

            // Take first output as input for next node
            current_data = outputs.into_iter().next().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Node '{}' produced no output",
                    executor.node_id()
                ))
            })?;
        }

        // Cleanup all nodes
        for executor in &executors {
            executor.cleanup()?;
        }

        // Convert final output to Python
        Python::attach(|py| {
            let py_output = runtime_data_to_python(py, &current_data)?;

            if enable_metrics.unwrap_or(false) {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", py_output)?;
                dict.set_item("metrics", "{}")?;
                Ok(dict.into())
            } else {
                Ok(py_output)
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

/// Python module initialization
///
/// This is now defined in lib.rs to properly set up the PyO3 module structure

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
