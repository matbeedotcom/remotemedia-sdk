//! Python bindings for RuntimeData
//!
//! This module provides PyO3 bindings that allow Python nodes to work
//! directly with RuntimeData instead of going through JSON serialization.

use crate::data::RuntimeData;
use crate::grpc_service::generated::{AudioBuffer, VideoFrame, TensorBuffer};
use prost::bytes::Bytes;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::HashMap;

/// Python wrapper for RuntimeData
///
/// This allows Python nodes to receive and return RuntimeData directly,
/// avoiding JSON serialization overhead and maintaining type safety.
#[pyclass(name = "RuntimeData")]
#[derive(Clone)]
pub struct PyRuntimeData {
    pub inner: RuntimeData,
    #[pyo3(get, set)]
    pub session_id: Option<String>,
}

#[pymethods]
impl PyRuntimeData {
    /// Create a Text RuntimeData
    #[staticmethod]
    fn text(text: String) -> Self {
        PyRuntimeData {
            inner: RuntimeData::Text(text),
            session_id: None,
        }
    }

    /// Create a JSON RuntimeData from a Python dict/list
    #[staticmethod]
    fn json(py: Python, value: Bound<'_, PyAny>) -> PyResult<Self> {
        // Convert Python object to JSON string then parse
        let json_module = py.import("json")?;
        let json_str: String = json_module
            .call_method1("dumps", (value,))?
            .extract()?;
        let json_value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid JSON: {}", e)))?;

        Ok(PyRuntimeData {
            inner: RuntimeData::Json(json_value),
            session_id: None,
        })
    }

    /// Create a Binary RuntimeData
    #[staticmethod]
    fn binary(data: Bound<'_, PyBytes>) -> Self {
        PyRuntimeData {
            inner: RuntimeData::Binary(Bytes::copy_from_slice(data.as_bytes())),
            session_id: None,
        }
    }

    /// Create an Audio RuntimeData
    #[staticmethod]
    fn audio(
        samples: Vec<u8>,
        sample_rate: u32,
        channels: u32,
        format: String,
        num_samples: u64,
    ) -> Self {
        let audio_format = match format.as_str() {
            "f32" => 0, // AudioFormat::F32
            "i16" => 1, // AudioFormat::I16
            "i32" => 2, // AudioFormat::I32
            _ => 0,     // Default to F32
        };

        PyRuntimeData {
            inner: RuntimeData::Audio(AudioBuffer {
                samples,
                sample_rate,
                channels,
                format: audio_format,
                num_samples,
            }),
            session_id: None,
        }
    }

    /// Get the data type as a string
    fn data_type(&self) -> String {
        match &self.inner {
            RuntimeData::Audio(_) => "audio".to_string(),
            RuntimeData::Video(_) => "video".to_string(),
            RuntimeData::Tensor(_) => "tensor".to_string(),
            RuntimeData::Json(_) => "json".to_string(),
            RuntimeData::Text(_) => "text".to_string(),
            RuntimeData::Binary(_) => "binary".to_string(),
        }
    }

    /// Get item count
    fn item_count(&self) -> usize {
        self.inner.item_count()
    }

    /// Check if this is text data
    fn is_text(&self) -> bool {
        matches!(self.inner, RuntimeData::Text(_))
    }

    /// Check if this is audio data
    fn is_audio(&self) -> bool {
        matches!(self.inner, RuntimeData::Audio(_))
    }

    /// Check if this is JSON data
    fn is_json(&self) -> bool {
        matches!(self.inner, RuntimeData::Json(_))
    }

    /// Extract text (returns None if not text)
    fn as_text(&self) -> Option<String> {
        match &self.inner {
            RuntimeData::Text(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Extract JSON as Python object (returns None if not JSON)
    fn as_json(&self, py: Python) -> PyResult<Option<PyObject>> {
        match &self.inner {
            RuntimeData::Json(value) => {
                // Convert JSON to string then parse in Python
                let json_str = serde_json::to_string(value)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON serialization error: {}", e)))?;
                let json_module = py.import("json")?;
                let py_obj = json_module.call_method1("loads", (json_str,))?;
                Ok(Some(py_obj.into()))
            }
            _ => Ok(None),
        }
    }

    /// Extract audio data as tuple (samples_bytes, sample_rate, channels, format, num_samples)
    fn as_audio(&self, py: Python) -> Option<(PyObject, u32, u32, String, u64)> {
        match &self.inner {
            RuntimeData::Audio(buf) => {
                let samples_bytes = PyBytes::new(py, &buf.samples);
                let format_str = match buf.format {
                    0 => "f32",
                    1 => "i16",
                    2 => "i32",
                    _ => "f32",
                };
                Some((
                    samples_bytes.into(),
                    buf.sample_rate,
                    buf.channels,
                    format_str.to_string(),
                    buf.num_samples,
                ))
            }
            _ => None,
        }
    }

    /// Extract binary data
    fn as_binary(&self, py: Python) -> Option<PyObject> {
        match &self.inner {
            RuntimeData::Binary(bytes) => {
                let py_bytes = PyBytes::new(py, bytes);
                Some(py_bytes.into())
            }
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!("RuntimeData({})", self.data_type())
    }
}

/// Convert Python numpy array to audio RuntimeData
///
/// This is a convenience function for Python nodes that work with numpy arrays
#[pyfunction]
fn numpy_to_audio(
    py: Python,
    array: Bound<'_, PyAny>,
    sample_rate: u32,
    channels: u32,
) -> PyResult<PyRuntimeData> {
    // Import numpy
    let numpy = py.import("numpy")?;

    // Ensure array is float32
    let array_f32 = array.call_method1("astype", (numpy.getattr("float32")?,))?;

    // Get the raw bytes
    let tobytes = array_f32.call_method0("tobytes")?;
    let bytes: Bound<PyBytes> = tobytes.extract()?;
    let samples_vec = bytes.as_bytes().to_vec();

    // Calculate number of samples
    let num_samples = (samples_vec.len() / 4) as u64; // 4 bytes per f32

    Ok(PyRuntimeData {
        inner: RuntimeData::Audio(AudioBuffer {
            samples: samples_vec,
            sample_rate,
            channels,
            format: 1, // F32 (codebase convention)
            num_samples,
        }),
        session_id: None,
    })
}

/// Convert audio RuntimeData to numpy array
#[pyfunction]
fn audio_to_numpy(py: Python, data: &PyRuntimeData) -> PyResult<PyObject> {
    match &data.inner {
        RuntimeData::Audio(buf) => {
            let numpy = py.import("numpy")?;

            // Create bytes object
            let py_bytes = PyBytes::new(py, &buf.samples);

            // Determine dtype based on format
            let dtype = match buf.format {
                0 => "float32",
                1 => "int16",
                2 => "int32",
                _ => "float32",
            };

            // Create numpy array from bytes
            let array = numpy.call_method1(
                "frombuffer",
                (py_bytes, numpy.getattr(dtype)?),
            )?;

            // Reshape if multi-channel
            let reshaped = if buf.channels > 1 {
                let shape = (buf.num_samples as usize / buf.channels as usize, buf.channels as usize);
                array.call_method1("reshape", (shape,))?
            } else {
                array
            };

            Ok(reshaped.into())
        }
        _ => Err(pyo3::exceptions::PyTypeError::new_err(
            "RuntimeData is not Audio type",
        )),
    }
}

/// Register the RuntimeData Python module
pub fn register_runtime_data_module(py: Python, parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let runtime_data_module = PyModule::new(py, "runtime_data")?;

    runtime_data_module.add_class::<PyRuntimeData>()?;
    runtime_data_module.add_function(wrap_pyfunction!(numpy_to_audio, &runtime_data_module)?)?;
    runtime_data_module.add_function(wrap_pyfunction!(audio_to_numpy, &runtime_data_module)?)?;

    parent_module.add_submodule(&runtime_data_module)?;

    // Also register in sys.modules so it can be imported as "from remotemedia_runtime.runtime_data import ..."
    let sys = py.import("sys")?;
    let sys_modules = sys.getattr("modules")?;
    sys_modules.set_item("remotemedia_runtime.runtime_data", runtime_data_module)?;

    Ok(())
}

/// Helper function to convert Rust RuntimeData to Python PyRuntimeData
pub fn runtime_data_to_py(data: RuntimeData) -> PyRuntimeData {
    PyRuntimeData {
        inner: data,
        session_id: None,
    }
}

/// Helper function to convert Rust RuntimeData to Python PyRuntimeData with session_id
pub fn runtime_data_to_py_with_session(data: RuntimeData, session_id: Option<String>) -> PyRuntimeData {
    PyRuntimeData {
        inner: data,
        session_id,
    }
}

/// Helper function to extract Rust RuntimeData from Python PyRuntimeData
pub fn py_to_runtime_data(py_data: &PyRuntimeData) -> RuntimeData {
    py_data.inner.clone()
}
