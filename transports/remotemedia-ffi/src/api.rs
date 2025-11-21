//! Python FFI functions for calling Rust runtime from Python
//!
//! This module provides the bridge between Python and Rust, allowing
//! Python code to execute pipelines using the Rust runtime.
//!
//! Uses PipelineRunner from runtime-core for transport-agnostic execution.

use super::marshal::{json_to_python, python_to_json};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use remotemedia_runtime_core::{
    data::RuntimeData,
    manifest::Manifest,
    transport::{PipelineRunner, TransportData},
};
use std::sync::Arc;

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

        // Create PipelineRunner
        let runner = PipelineRunner::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create runner: {}",
                e
            ))
        })?;

        // Execute using PipelineRunner (no input data for basic execution)
        let input = TransportData::new(RuntimeData::Text(String::new()));
        let output = runner.execute_unary(manifest, input).await.map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Execution failed: {}", e))
        })?;

        // Convert output to Python
        Python::attach(|py| {
            // Convert RuntimeData to JSON for Python marshaling
            let output_json = match &output.data {
                RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                } => {
                    serde_json::json!({
                        "type": "audio",
                        "samples": samples,
                        "sample_rate": sample_rate,
                        "channels": channels
                    })
                }
                RuntimeData::Text(s) => {
                    serde_json::json!({ "type": "text", "data": s })
                }
                RuntimeData::Json(v) => v.clone(),
                RuntimeData::Binary(b) => {
                    serde_json::json!({ "type": "binary", "data": b })
                }
                RuntimeData::Video { .. } => {
                    serde_json::json!({ "type": "video", "note": "Video data not fully supported in FFI yet" })
                }
                RuntimeData::Tensor { .. } => {
                    serde_json::json!({ "type": "tensor", "note": "Tensor data not fully supported in FFI yet" })
                }
                RuntimeData::ControlMessage { .. } => {
                    serde_json::json!({ "type": "control_message", "note": "Control message data not fully supported in FFI yet" })
                }
            };

            let outputs_py = json_to_python(py, &output_json)?;

            // Include metrics if requested (TODO: get metrics from PipelineRunner)
            if enable_metrics.unwrap_or(false) {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", &outputs_py)?;
                dict.set_item("metrics", "{}")?; // Placeholder - metrics not yet exposed from PipelineRunner
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
        let manifest: Manifest = serde_json::from_str(&manifest_json).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to parse manifest: {}",
                e
            ))
        })?;
        let manifest = Arc::new(manifest);

        // Create PipelineRunner
        let runner = PipelineRunner::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create runner: {}",
                e
            ))
        })?;

        // Convert JSON input to RuntimeData (simplified - assumes first input item)
        let input_data = if let Some(first) = rust_input.first() {
            if let Some(text) = first.as_str() {
                RuntimeData::Text(text.to_string())
            } else {
                // Try to handle as JSON
                RuntimeData::Json(first.clone())
            }
        } else {
            RuntimeData::Text(String::new())
        };

        // Execute using PipelineRunner
        let input = TransportData::new(input_data);
        let output = runner.execute_unary(manifest, input).await.map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Execution failed: {}", e))
        })?;

        // Convert output to Python
        Python::attach(|py| {
            // Convert RuntimeData to JSON for Python marshaling
            let output_json = match &output.data {
                RuntimeData::Audio {
                    samples,
                    sample_rate,
                    channels,
                } => {
                    serde_json::json!({
                        "type": "audio",
                        "samples": samples,
                        "sample_rate": sample_rate,
                        "channels": channels
                    })
                }
                RuntimeData::Text(s) => {
                    serde_json::json!({ "type": "text", "data": s })
                }
                RuntimeData::Json(v) => v.clone(),
                RuntimeData::Binary(b) => {
                    serde_json::json!({ "type": "binary", "data": b })
                }
                RuntimeData::Video { .. } => {
                    serde_json::json!({ "type": "video", "note": "Video data not fully supported in FFI yet" })
                }
                RuntimeData::Tensor { .. } => {
                    serde_json::json!({ "type": "tensor", "note": "Tensor data not fully supported in FFI yet" })
                }
                RuntimeData::ControlMessage { .. } => {
                    serde_json::json!({ "type": "control_message", "note": "Control message data not fully supported in FFI yet" })
                }
            };

            let outputs_py = json_to_python(py, &output_json)?;

            // Include metrics if requested (TODO: get metrics from PipelineRunner)
            if enable_metrics.unwrap_or(false) {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("outputs", &outputs_py)?;
                dict.set_item("metrics", "{}")?; // Placeholder
                Ok(dict.into_any().unbind())
            } else {
                Ok(outputs_py.unbind())
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
                "Cannot execute empty pipeline"
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
            current_data = outputs.into_iter().next()
                .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("Node '{}' produced no output", executor.node_id())
                ))?;
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
