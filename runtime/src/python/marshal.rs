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
pub fn python_to_json(py: Python, obj: &Bound<'_, PyAny>) -> PyResult<Value> {
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
            vec.push(python_to_json(py, &item)?);
        }
        return Ok(Value::Array(vec));
    }

    // Tuple (convert to array, as JSON doesn't have tuples)
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let mut vec = Vec::new();
        for item in tuple.iter() {
            vec.push(python_to_json(py, &item)?);
        }
        return Ok(Value::Array(vec));
    }

    // Dict
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str = key.extract::<String>()?;
            map.insert(key_str, python_to_json(py, &value)?);
        }
        return Ok(Value::Object(map));
    }

    // Unsupported type
    Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
        format!(
            "Cannot convert Python type '{}' to JSON",
            obj.get_type().name()?
        )
    ))
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
                let py_item = json_to_python(py, item)?;
                py_list.append(py_item)?;
            }
            Ok(py_list.into_any())
        }

        Value::Object(obj) => {
            let py_dict = PyDict::new(py);
            for (key, value) in obj {
                py_dict.set_item(key, json_to_python(py, value)?)?;
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            let py_dict = [("name", "Alice"), ("city", "NYC")]
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>()
                .into_bound_py_any(py).unwrap();

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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            // Create complex Python structure
            let py_list = PyList::empty(py);
            py_list.append(1.into_bound_py_any(py).unwrap()).unwrap();
            py_list.append("test".into_bound_py_any(py).unwrap()).unwrap();
            py_list.append(vec![2, 3, 4].into_bound_py_any(py).unwrap()).unwrap();

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
        Python::with_gil(|py| {
            use pyo3::types::PyTuple;

            // Simple tuple
            let tuple = PyTuple::new(py, &[1, 2, 3]).unwrap();
            let json_val = python_to_json(py, tuple.as_any()).unwrap();
            assert_eq!(json_val, Value::Array(vec![Value::from(1), Value::from(2), Value::from(3)]));

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
