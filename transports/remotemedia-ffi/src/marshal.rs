//! Data marshaling between Python and Rust types.
//!
//! This module provides conversion functions for:
//! - Python objects → Rust serde_json::Value
//! - Rust serde_json::Value → Python objects

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::IntoPyObjectExt;
use serde_json::Value;

/// Convert a Python object to a JSON Value
///
/// Supports:
/// - None → Null
/// - bool → Bool
/// - int → Number
/// - float → Number
/// - str → String
/// - list → Array
/// - tuple → Array (JSON doesn't distinguish tuples from lists)
/// - dict → Object
///
/// If a cache is provided, complex objects that can't be serialized to JSON
/// will be stored in the cache and a reference will be returned instead of pickling.
pub fn python_to_json(py: Python, obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    python_to_json_impl(py, obj, None)
}

/// Convert with optional cache support
pub fn python_to_json_with_cache(
    py: Python,
    obj: &Bound<'_, PyAny>,
    cache: Option<&PyObjectCache>,
) -> PyResult<Value> {
    python_to_json_impl(py, obj, cache)
}

fn python_to_json_impl(
    py: Python,
    obj: &Bound<'_, PyAny>,
    cache: Option<&PyObjectCache>,
) -> PyResult<Value> {
    // None
    if obj.is_none() {
        return Ok(Value::Null);
    }

    // Boolean (check before int, as bool is subclass of int in Python)
    if let Ok(val) = obj.extract::<bool>() {
        return Ok(Value::Bool(val));
    }

    // Integer
    if let Ok(val) = obj.extract::<i64>() {
        return Ok(Value::Number(val.into()));
    }

    // Float
    if let Ok(val) = obj.extract::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(val) {
            return Ok(Value::Number(num));
        }
        // If float is NaN or Inf, convert to null
        return Ok(Value::Null);
    }

    // String
    if let Ok(val) = obj.extract::<String>() {
        return Ok(Value::String(val));
    }

    // List
    if let Ok(list) = obj.downcast::<PyList>() {
        let mut vec = Vec::new();
        for item in list.iter() {
            vec.push(python_to_json_impl(py, &item, cache)?);
        }
        return Ok(Value::Array(vec));
    }

    // Tuple (convert to array, as JSON doesn't have tuples)
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let mut vec = Vec::new();
        for item in tuple.iter() {
            vec.push(python_to_json_impl(py, &item, cache)?);
        }
        return Ok(Value::Array(vec));
    }

    // Dict
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str = key.extract::<String>()?;
            map.insert(key_str, python_to_json_impl(py, &value, cache)?);
        }
        return Ok(Value::Object(map));
    }

    // Numpy array - serialize with metadata for cross-language support (only with native-numpy feature)
    #[cfg(feature = "native-numpy")]
    {
        if is_numpy_array(py, obj) {
            tracing::info!("Converting numpy array to JSON with metadata");
            return numpy_to_json(py, obj);
        }
    }

    // Unsupported type - try to cache it if cache is available, otherwise pickle
    tracing::info!(
        "Complex Python object encountered: {}",
        obj.get_type().name()?
    );

    if let Some(cache) = cache {
        // Cache is available - store object and return reference instead of pickling
        tracing::info!(
            "Caching {} object instead of pickling",
            obj.get_type().name()?
        );
        let py_copy = obj.clone().unbind();
        let cache_id = cache.store(py_copy);

        let mut ref_obj = serde_json::Map::new();
        ref_obj.insert("__pyobj__".to_string(), Value::Bool(true));
        ref_obj.insert("id".to_string(), Value::String(cache_id));
        ref_obj.insert(
            "type".to_string(),
            Value::String(obj.get_type().name()?.to_string()),
        );

        return Ok(Value::Object(ref_obj));
    }

    // No cache - must pickle
    tracing::info!(
        "No cache available, attempting to pickle {}",
        obj.get_type().name()?
    );

    // Try cloudpickle first (handles more types like Cython objects), fall back to pickle
    let pickled_bytes = match py.import("cloudpickle") {
        Ok(cloudpickle) => {
            tracing::info!(
                "Using cloudpickle for serialization of {}",
                obj.get_type().name()?
            );
            match cloudpickle.call_method1("dumps", (obj,)) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::error!("CloudPickle failed for {}: {}", obj.get_type().name()?, e);
                    return Err(e);
                }
            }
        }
        Err(_) => {
            tracing::info!("cloudpickle not available, using standard pickle");
            let pickle = py.import("pickle")?;
            pickle.call_method1("dumps", (obj,))?
        }
    };

    let bytes: &[u8] = pickled_bytes.extract()?;

    // Base64 encode the pickled bytes
    let base64_encoded = base64::encode(bytes);

    // Store as a special JSON object that signals this is pickled data
    let mut map = serde_json::Map::new();
    map.insert("__pickled__".to_string(), Value::Bool(true));
    map.insert("data".to_string(), Value::String(base64_encoded));
    map.insert(
        "type".to_string(),
        Value::String(obj.get_type().name()?.to_string()),
    );

    tracing::info!(
        "Successfully pickled object of type: {}",
        obj.get_type().name()?
    );
    Ok(Value::Object(map))
}

/// Convert a JSON Value to a Python object
///
/// Supports:
/// - Null → None
/// - Bool → bool
/// - Number → int or float
/// - String → str
/// - Array → list
/// - Object → dict
pub fn json_to_python<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    json_to_python_with_cache(py, value, None)
}

/// Convert with optional cache support for __pyobj__ references
pub fn json_to_python_with_cache<'py>(
    py: Python<'py>,
    value: &Value,
    cache: Option<&PyObjectCache>,
) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Null => Ok(py.None().into_bound(py)),

        Value::Bool(b) => Ok(b.into_bound_py_any(py)?),

        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_bound_py_any(py)?)
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_bound_py_any(py)?)
            } else {
                // Fallback for u64 or other edge cases
                Ok(py.None().into_bound(py))
            }
        }

        Value::String(s) => Ok(s.into_bound_py_any(py)?),

        Value::Array(arr) => {
            let py_list = PyList::empty(py);
            for item in arr {
                let py_item = json_to_python_with_cache(py, item, cache)?;
                py_list.append(py_item)?;
            }
            Ok(py_list.into_any())
        }

        Value::Object(obj) => {
            // Check if this is a numpy array (Phase 1.7.3) - only with native-numpy feature
            #[cfg(feature = "native-numpy")]
            {
                if is_numpy_json(value) {
                    tracing::info!("Converting JSON back to numpy array");
                    return json_to_numpy(py, value);
                }
            }

            // Check if this is a PyObject cache reference (Phase 1.10)
            if obj
                .get("__pyobj__")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(cache) = cache {
                    if let Some(Value::String(id)) = obj.get("id") {
                        tracing::info!("Retrieving PyObject reference from cache: {}", id);
                        if let Some(cached_obj) = cache.get(id) {
                            return Ok(cached_obj.into_bound(py));
                        } else {
                            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                                "PyObject reference {} not found in cache",
                                id
                            )));
                        }
                    }
                }
                // No cache or invalid reference - return as regular dict
                tracing::warn!("__pyobj__ reference found but cache not available or ID missing");
            }

            // Check if this is a pickled Python object (Phase 1.7.4)
            if obj
                .get("__pickled__")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(Value::String(base64_data)) = obj.get("data") {
                    tracing::info!(
                        "Unpickling Python object: {}",
                        obj.get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                    );

                    // Base64 decode
                    let pickled_bytes = base64::decode(base64_data).map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Failed to decode base64: {}",
                            e
                        ))
                    })?;

                    // Unpickle using Python's pickle module
                    let pickle = py.import("pickle")?;
                    let py_bytes = pyo3::types::PyBytes::new(py, &pickled_bytes);
                    let unpickled = pickle.call_method1("loads", (py_bytes,))?;

                    tracing::info!("Successfully unpickled object");
                    return Ok(unpickled);
                }
            }

            // Regular dict - recursively convert values (may contain __pyobj__ references)
            let py_dict = PyDict::new(py);
            for (key, value) in obj {
                py_dict.set_item(key, json_to_python_with_cache(py, value, cache)?)?;
            }
            Ok(py_dict.into_any())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_to_json_primitives() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            // None
            let py_none = py.None().into_bound(py);
            assert_eq!(python_to_json(py, &py_none).unwrap(), Value::Null);

            // Boolean
            let py_bool = true.into_bound_py_any(py).unwrap();
            assert_eq!(python_to_json(py, &py_bool).unwrap(), Value::Bool(true));

            // Integer
            let py_int = 42.into_bound_py_any(py).unwrap();
            assert_eq!(python_to_json(py, &py_int).unwrap(), Value::from(42));

            // Float
            let py_float = 3.14.into_bound_py_any(py).unwrap();
            let result = python_to_json(py, &py_float).unwrap();
            assert!(result.is_number());

            // String
            let py_str = "hello".into_bound_py_any(py).unwrap();
            assert_eq!(python_to_json(py, &py_str).unwrap(), Value::from("hello"));
        });
    }

    #[test]
    fn test_python_to_json_list() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            let py_list = vec![1, 2, 3].into_bound_py_any(py).unwrap();
            let result = python_to_json(py, &py_list).unwrap();

            assert_eq!(
                result,
                Value::Array(vec![Value::from(1), Value::from(2), Value::from(3)])
            );
        });
    }

    #[test]
    fn test_python_to_json_dict() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            let py_dict = [("name", "Alice"), ("city", "NYC")]
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>()
                .into_bound_py_any(py)
                .unwrap();

            let result = python_to_json(py, &py_dict).unwrap();
            assert!(result.is_object());

            let obj = result.as_object().unwrap();
            assert_eq!(obj.get("name").unwrap(), &Value::from("Alice"));
            assert_eq!(obj.get("city").unwrap(), &Value::from("NYC"));
        });
    }

    #[test]
    fn test_json_to_python_primitives() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            // Null
            let json_null = Value::Null;
            let py_obj = json_to_python(py, &json_null).unwrap();
            assert!(py_obj.is_none());

            // Boolean
            let json_bool = Value::Bool(true);
            let py_obj = json_to_python(py, &json_bool).unwrap();
            assert_eq!(py_obj.extract::<bool>().unwrap(), true);

            // Integer
            let json_int = Value::from(42);
            let py_obj = json_to_python(py, &json_int).unwrap();
            assert_eq!(py_obj.extract::<i64>().unwrap(), 42);

            // String
            let json_str = Value::from("hello");
            let py_obj = json_to_python(py, &json_str).unwrap();
            assert_eq!(py_obj.extract::<String>().unwrap(), "hello");
        });
    }

    #[test]
    fn test_json_to_python_collections() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            // Array
            let json_array = Value::Array(vec![Value::from(1), Value::from(2), Value::from(3)]);
            let py_obj = json_to_python(py, &json_array).unwrap();
            assert_eq!(py_obj.extract::<Vec<i64>>().unwrap(), vec![1, 2, 3]);

            // Object
            let mut map = serde_json::Map::new();
            map.insert("x".to_string(), Value::from(10));
            map.insert("y".to_string(), Value::from(20));
            let json_obj = Value::Object(map);

            let py_obj = json_to_python(py, &json_obj).unwrap();
            let py_dict = py_obj.downcast::<PyDict>().unwrap();

            let x_value = py_dict.get_item("x").unwrap().unwrap();
            let y_value = py_dict.get_item("y").unwrap().unwrap();
            assert_eq!(x_value.extract::<i64>().unwrap(), 10);
            assert_eq!(y_value.extract::<i64>().unwrap(), 20);
        });
    }

    #[test]
    fn test_round_trip() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            // Create complex Python structure
            let py_list = PyList::empty(py);
            py_list.append(1.into_bound_py_any(py).unwrap()).unwrap();
            py_list
                .append("test".into_bound_py_any(py).unwrap())
                .unwrap();
            py_list
                .append(vec![2, 3, 4].into_bound_py_any(py).unwrap())
                .unwrap();

            // Python → JSON
            let json_val = python_to_json(py, py_list.as_any()).unwrap();

            // JSON → Python
            let result = json_to_python(py, &json_val).unwrap();

            // Verify it's a list with correct length
            let result_list = result.downcast::<PyList>().unwrap();
            assert_eq!(result_list.len(), 3);
        });
    }

    #[test]
    fn test_tuple_conversion() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            use pyo3::types::PyTuple;

            // Simple tuple
            let tuple = PyTuple::new(py, &[1, 2, 3]).unwrap();
            let json_val = python_to_json(py, tuple.as_any()).unwrap();
            assert_eq!(
                json_val,
                Value::Array(vec![Value::from(1), Value::from(2), Value::from(3)])
            );

            // Nested tuple
            let inner1 = PyTuple::new(py, &[1, 2]).unwrap();
            let inner2 = PyTuple::new(py, &[3, 4]).unwrap();
            let outer = PyTuple::new(py, &[&inner1, &inner2]).unwrap();
            let json_val = python_to_json(py, outer.as_any()).unwrap();
            assert!(json_val.is_array());
            assert_eq!(json_val.as_array().unwrap().len(), 2);

            // Empty tuple
            let empty_tuple = PyTuple::empty(py);
            let json_val = python_to_json(py, empty_tuple.as_any()).unwrap();
            assert_eq!(json_val, Value::Array(vec![]));
        });
    }
}
