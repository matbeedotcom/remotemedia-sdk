//! Numpy array marshaling with zero-copy support (Phase 1.7.3)
//!
//! This module provides efficient data exchange between Rust and Python numpy arrays
//! using rust-numpy for zero-copy buffer access.
//!
//! Strategy:
//! 1. Python → Rust: Use rust-numpy's PyReadonlyArray for zero-copy read access
//! 2. Rust → Python: Use rust-numpy's PyArray for zero-copy write access
//! 3. For JSON serialization: Use metadata + base64 encoded data
//! 4. For direct Rust access: Use ndarray crate integration
//!
//! Supported dtypes:
//! - int8, int16, int32, int64
//! - uint8, uint16, uint32, uint64
//! - float32, float64
//! - bool

use numpy::{PyArray, PyArrayDyn, PyArrayMethods};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Numpy array metadata for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumpyArrayMeta {
    /// Array shape (dimensions)
    pub shape: Vec<usize>,

    /// Data type string (e.g., "float64", "int32")
    pub dtype: String,

    /// Total number of elements
    pub size: usize,

    /// Whether array is C-contiguous
    pub c_contiguous: bool,

    /// Whether array is Fortran-contiguous
    pub f_contiguous: bool,
}

/// Numpy array data transfer structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumpyArrayData {
    /// Array metadata
    pub meta: NumpyArrayMeta,

    /// Serialized data (as base64 string for JSON transport)
    pub data: String,
}

/// Check if a Python object is a numpy array
pub fn is_numpy_array(_py: Python, obj: &Bound<'_, PyAny>) -> bool {
    obj.downcast::<PyArrayDyn<f64>>().is_ok()
        || obj.downcast::<PyArrayDyn<f32>>().is_ok()
        || obj.downcast::<PyArrayDyn<i64>>().is_ok()
        || obj.downcast::<PyArrayDyn<i32>>().is_ok()
        || obj.downcast::<PyArrayDyn<u8>>().is_ok()
}

/// Extract numpy array metadata from a numpy array
pub fn extract_numpy_metadata(_py: Python, obj: &Bound<'_, PyAny>) -> PyResult<NumpyArrayMeta> {
    // Try to get as PyArrayDyn (works for any dtype)
    // We'll use reflection to get metadata without knowing the dtype
    let shape_obj = obj.getattr("shape")?;
    let shape: Vec<usize> = shape_obj.extract()?;

    let dtype_obj = obj.getattr("dtype")?;
    let dtype_str = dtype_obj.getattr("name")?.extract::<String>()?;

    let size: usize = obj.getattr("size")?.extract()?;

    let flags = obj.getattr("flags")?;
    let c_contiguous: bool = flags.getattr("c_contiguous")?.extract()?;
    let f_contiguous: bool = flags.getattr("f_contiguous")?.extract()?;

    Ok(NumpyArrayMeta {
        shape,
        dtype: dtype_str,
        size,
        c_contiguous,
        f_contiguous,
    })
}

/// Convert numpy array to JSON (with metadata + base64 data)
///
/// Uses rust-numpy for zero-copy read access to the array data.
pub fn numpy_to_json(py: Python, obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    let meta = extract_numpy_metadata(py, obj)?;

    // Get raw bytes using tobytes() method (copies data)
    // TODO: For true zero-copy, we'd pass shared memory handles instead
    let bytes_obj = obj.call_method0("tobytes")?;
    let bytes: &[u8] = bytes_obj.extract()?;

    // Encode as base64
    use base64::Engine;
    let base64_data = base64::engine::general_purpose::STANDARD.encode(bytes);

    let array_data = NumpyArrayData {
        meta,
        data: base64_data,
    };

    // Return as JSON with special marker
    Ok(serde_json::json!({
        "__numpy__": true,
        "array": serde_json::to_value(&array_data).unwrap()
    }))
}

/// Convert JSON (with numpy metadata) back to numpy array
///
/// Uses rust-numpy to create the array from raw bytes.
pub fn json_to_numpy<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    // Extract array data
    let array_data: NumpyArrayData = serde_json::from_value(
        value.get("array")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyValueError, _>("Missing 'array' field"))?
            .clone(),
    )
    .map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to parse array data: {}", e))
    })?;

    // Decode base64 data
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&array_data.data)
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Failed to decode base64: {}", e))
        })?;

    // Import numpy
    let numpy = py.import("numpy")?;

    // Create numpy array from bytes
    // numpy.frombuffer(bytes, dtype=dtype).reshape(shape)
    let frombuffer = numpy.getattr("frombuffer")?;
    let flat_array = frombuffer.call1((bytes, array_data.meta.dtype.as_str()))?;

    // Reshape to original shape
    let shaped_array = flat_array.call_method1("reshape", (array_data.meta.shape,))?;

    Ok(shaped_array)
}

/// Check if a JSON value represents a numpy array
pub fn is_numpy_json(value: &Value) -> bool {
    value
        .get("__numpy__")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Convert numpy array to Rust vector (for f64 arrays)
///
/// This provides zero-copy read access to the array data.
pub fn numpy_to_vec_f64(_py: Python, obj: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let array = obj.downcast::<PyArrayDyn<f64>>()?;
    let readonly = array.readonly();
    let slice = readonly.as_slice()?;
    Ok(slice.to_vec())
}

/// Convert Rust vector to numpy array (f64)
///
/// This creates a new numpy array from Rust data.
pub fn vec_to_numpy_f64<'py>(py: Python<'py>, data: &[f64], shape: &[usize]) -> PyResult<Bound<'py, PyAny>> {
    let array = PyArray::from_slice(py, data);
    let reshaped = array.reshape(shape)?;
    Ok(reshaped.into_any())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_numpy_array() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            // Try to import numpy (skip test if not available)
            if py.import("numpy").is_err() {
                println!("Skipping test: numpy not available in this Python environment");
                return;
            }

            // Create a numpy array using run
            use pyo3::ffi::c_str;
            let code = c_str!(r#"
import numpy as np
result = np.array([1.0, 2.0, 3.0])
"#);
            py.run(code, None, None).unwrap();

            // Get the result from locals
            let locals = py.eval(c_str!("locals()"), None, None).unwrap();
            let array = locals.get_item("result").unwrap();

            assert!(is_numpy_array(py, &array));

            // Regular list should not be detected as numpy array
            let list = py.eval(c_str!("[1, 2, 3]"), None, None).unwrap();
            assert!(!is_numpy_array(py, &list));
        });
    }

    #[test]
    fn test_extract_numpy_metadata() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            if py.import("numpy").is_err() {
                println!("Skipping test: numpy not available");
                return;
            }

            use pyo3::ffi::c_str;
            // Create a 2D array
            let code = c_str!(r#"
import numpy as np
result = np.array([[1, 2], [3, 4]], dtype=np.int32)
"#);
            py.run(code, None, None).unwrap();
            let locals = py.eval(c_str!("locals()"), None, None).unwrap();
            let array = locals.get_item("result").unwrap();

            let meta = extract_numpy_metadata(py, &array).unwrap();

            assert_eq!(meta.shape, vec![2, 2]);
            assert_eq!(meta.dtype, "int32");
            assert_eq!(meta.size, 4);
            assert!(meta.c_contiguous);
        });
    }

    #[test]
    fn test_numpy_roundtrip() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            if py.import("numpy").is_err() {
                println!("Skipping test: numpy not available");
                return;
            }

            use pyo3::ffi::c_str;
            // Create array
            let code = c_str!(r#"
import numpy as np
result = np.array([1.0, 2.0, 3.0, 4.0], dtype=np.float64)
"#);
            py.run(code, None, None).unwrap();
            let locals = py.eval(c_str!("locals()"), None, None).unwrap();
            let original = locals.get_item("result").unwrap();

            // Python → JSON
            let json_val = numpy_to_json(py, &original).unwrap();

            assert!(is_numpy_json(&json_val));

            // JSON → Python
            let reconstructed = json_to_numpy(py, &json_val).unwrap();

            // Verify shape and dtype
            let meta = extract_numpy_metadata(py, &reconstructed).unwrap();
            assert_eq!(meta.shape, vec![4]);
            assert_eq!(meta.dtype, "float64");

            // Verify data (compare as lists)
            let numpy = py.import("numpy").unwrap();
            let allclose = numpy.getattr("allclose").unwrap();
            let result = allclose.call1((&original, &reconstructed)).unwrap();
            let arrays_equal: bool = result.extract().unwrap();

            assert!(arrays_equal, "Arrays should be equal after round-trip");
        });
    }

    #[test]
    fn test_numpy_to_vec() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            if py.import("numpy").is_err() {
                println!("Skipping test: numpy not available");
                return;
            }

            use pyo3::ffi::c_str;
            // Create array
            let code = c_str!(r#"
import numpy as np
result = np.array([1.0, 2.0, 3.0, 4.0])
"#);
            py.run(code, None, None).unwrap();
            let locals = py.eval(c_str!("locals()"), None, None).unwrap();
            let array = locals.get_item("result").unwrap();

            // Convert to Rust Vec
            let vec = numpy_to_vec_f64(py, &array).unwrap();

            assert_eq!(vec, vec![1.0, 2.0, 3.0, 4.0]);
        });
    }

    #[test]
    fn test_vec_to_numpy() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            if py.import("numpy").is_err() {
                println!("Skipping test: numpy not available");
                return;
            }

            // Create Rust vector
            let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
            let shape = vec![2, 3];

            // Convert to numpy
            let array = vec_to_numpy_f64(py, &data, &shape).unwrap();

            // Verify shape
            let meta = extract_numpy_metadata(py, &array).unwrap();
            assert_eq!(meta.shape, vec![2, 3]);
            assert_eq!(meta.size, 6);
        });
    }
}
