//! Round-trip marshaling tests for Phase 1.7
//!
//! This test suite validates data marshaling between Rust and Python:
//! - Rust → JSON → Python → JSON → Rust
//! - Tests primitives, collections, nested structures
//! - Identifies gaps in current implementation
//! - Establishes baseline for numpy and CloudPickle support

use remotemedia_runtime::python::{marshal, vm::PythonVm};
use serde_json::{json, Value};

/// Test round-trip for primitive types
#[test]
fn test_roundtrip_primitives() {
    let test_cases = vec![
        ("null", Value::Null),
        ("bool_true", Value::Bool(true)),
        ("bool_false", Value::Bool(false)),
        ("int_positive", json!(42)),
        ("int_negative", json!(-17)),
        ("int_zero", json!(0)),
        ("float_positive", json!(3.14)),
        ("float_negative", json!(-2.71)),
        ("float_zero", json!(0.0)),
        ("string_simple", json!("hello")),
        ("string_empty", json!("")),
        ("string_with_quotes", json!("He said \"hello\"")),
        ("string_with_backslash", json!("path\\to\\file")),
        ("string_unicode", json!("こんにちは")),
    ];

    pyo3::Python::with_gil(|py| {
        for (name, original) in test_cases {
            // Rust → Python
            let py_obj = marshal::json_to_python(py, &original)
                .unwrap_or_else(|e| panic!("{}: Failed to convert to Python: {:?}", name, e));

            // Python → Rust
            let result = marshal::python_to_json(py, &py_obj)
                .unwrap_or_else(|e| panic!("{}: Failed to convert from Python: {:?}", name, e));

            assert_eq!(original, result, "{}: Round-trip failed", name);
        }
    });
}

/// Test round-trip for collection types
#[test]
fn test_roundtrip_collections() {
    let test_cases = vec![
        ("list_empty", json!([])),
        ("list_ints", json!([1, 2, 3, 4, 5])),
        ("list_strings", json!(["a", "b", "c"])),
        ("list_mixed", json!([1, "two", 3.0, true, null])),
        ("dict_empty", json!({})),
        ("dict_simple", json!({"name": "Alice", "age": 30})),
        (
            "dict_nested",
            json!({"user": {"name": "Bob", "roles": ["admin", "user"]}}),
        ),
        ("list_of_dicts", json!([{"id": 1}, {"id": 2}, {"id": 3}])),
        (
            "dict_of_lists",
            json!({"nums": [1, 2, 3], "strs": ["a", "b"]}),
        ),
    ];

    pyo3::Python::with_gil(|py| {
        for (name, original) in test_cases {
            let py_obj = marshal::json_to_python(py, &original)
                .unwrap_or_else(|e| panic!("{}: Failed to convert to Python: {:?}", name, e));

            let result = marshal::python_to_json(py, &py_obj)
                .unwrap_or_else(|e| panic!("{}: Failed to convert from Python: {:?}", name, e));

            assert_eq!(original, result, "{}: Round-trip failed", name);
        }
    });
}

/// Test round-trip for deeply nested structures
#[test]
fn test_roundtrip_nested() {
    let deep_structure = json!({
        "pipeline": {
            "nodes": [
                {
                    "id": "node1",
                    "type": "transform",
                    "params": {
                        "operation": "scale",
                        "factor": 2.5,
                        "enabled": true
                    },
                    "outputs": ["node2", "node3"]
                },
                {
                    "id": "node2",
                    "type": "filter",
                    "params": {
                        "threshold": 0.8,
                        "mode": "absolute"
                    },
                    "outputs": []
                }
            ],
            "metadata": {
                "created": "2025-10-23",
                "version": 1,
                "tags": ["test", "example"]
            }
        }
    });

    pyo3::Python::with_gil(|py| {
        let py_obj = marshal::json_to_python(py, &deep_structure).unwrap();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        assert_eq!(deep_structure, result);
    });
}

/// Test round-trip through Python VM execution
#[test]
fn test_roundtrip_via_vm() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    // Test simple data storage and retrieval
    let test_data = json!([1, 2, 3, 4, 5]);

    // Use vm.json_to_python_value to create Python representation
    let code = format!(
        r#"
test_data = {}
test_data
"#,
        vm.json_to_python_value(&test_data)
    );

    let response = vm.execute(&code).unwrap();
    assert_eq!(response["status"], "success");

    // Verify we can retrieve the data
    let retrieve_code = "test_data";
    let response = vm.execute(retrieve_code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "[1, 2, 3, 4, 5]");
}

/// Test edge cases and special values
#[test]
fn test_roundtrip_edge_cases() {
    pyo3::Python::with_gil(|py| {
        // Empty string
        let empty_str = json!("");
        let py_obj = marshal::json_to_python(py, &empty_str).unwrap();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        assert_eq!(empty_str, result);

        // Large integer
        let large_int = json!(9223372036854775807i64); // i64::MAX
        let py_obj = marshal::json_to_python(py, &large_int).unwrap();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        assert_eq!(large_int, result);

        // Deeply nested array
        let nested_array = json!([[[[1, 2], [3, 4]], [[5, 6], [7, 8]]]]);
        let py_obj = marshal::json_to_python(py, &nested_array).unwrap();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        assert_eq!(nested_array, result);
    });
}

/// Test special float values (NaN, Inf)
#[test]
fn test_special_floats() {
    pyo3::Python::with_gil(|py| {
        use pyo3::types::PyFloat;

        // NaN converts to null
        let py_nan = PyFloat::new(py, f64::NAN);
        let result = marshal::python_to_json(py, &py_nan.into_any()).unwrap();
        assert_eq!(result, Value::Null);

        // Infinity converts to null
        let py_inf = PyFloat::new(py, f64::INFINITY);
        let result = marshal::python_to_json(py, &py_inf.into_any()).unwrap();
        assert_eq!(result, Value::Null);
    });
}

/// Test type preservation through round-trip
#[test]
fn test_type_preservation() {
    pyo3::Python::with_gil(|py| {
        // Ensure int stays int, not converted to float
        let int_val = json!(42);
        let py_obj = marshal::json_to_python(py, &int_val).unwrap();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        assert!(result.is_i64());

        // Ensure float stays float
        let float_val = json!(42.0);
        let py_obj = marshal::json_to_python(py, &float_val).unwrap();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        assert!(result.is_f64());
    });
}

/// Test error handling for unsupported types
#[test]
fn test_unsupported_types() {
    pyo3::Python::with_gil(|py| {
        use pyo3::types::PyBytes;

        // Bytes should fail with unsupported type error
        let py_bytes = PyBytes::new(py, b"binary data");
        let result = marshal::python_to_json(py, &py_bytes.into_any());
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("Cannot convert"));
    });
}

/// Test marshaling with Python node processing
#[test]
fn test_node_data_roundtrip() {
    let mut vm = PythonVm::new().unwrap();

    let node_code = r#"
class DataProcessorNode:
    def __init__(self):
        self.count = 0

    def process(self, data):
        # Just double the input number
        self.count += 1
        return data * 2
"#;

    vm.load_class(node_code, "DataProcessorNode").unwrap();
    let instance = vm
        .create_instance("DataProcessorNode", &Value::Null)
        .unwrap();

    // Test with simple number input
    let input_num = json!(21);
    let result = vm.call_method(&instance, "process", &input_num);

    if let Err(e) = &result {
        eprintln!("Error calling process: {:?}", e);
        panic!("Failed to call process: {:?}", e);
    }

    let result = result.unwrap();
    assert_eq!(result["status"], "success");
    let result_str = result["result"].as_str().unwrap();
    eprintln!("Result from processing: {}", result_str);
    assert_eq!(result_str, "42"); // 21 * 2 = 42
}

/// Test marshaling performance with large datasets
#[test]
fn test_large_data_roundtrip() {
    // Create a large dataset
    let large_array: Vec<i64> = (0..1000).collect();
    let large_data = json!({
        "data": large_array,
        "metadata": {
            "count": 1000,
            "type": "sequence"
        }
    });

    pyo3::Python::with_gil(|py| {
        let start = std::time::Instant::now();

        // Rust → Python
        let py_obj = marshal::json_to_python(py, &large_data).unwrap();
        let to_python_time = start.elapsed();

        // Python → Rust
        let start = std::time::Instant::now();
        let result = marshal::python_to_json(py, &py_obj).unwrap();
        let from_python_time = start.elapsed();

        assert_eq!(large_data, result);

        println!("Large data round-trip:");
        println!("  Rust → Python: {:?}", to_python_time);
        println!("  Python → Rust: {:?}", from_python_time);
        println!("  Total: {:?}", to_python_time + from_python_time);
    });
}

/// Test for tuple support (converts to array)
#[test]
fn test_tuple_support() {
    use std::ffi::CString;
    pyo3::Python::with_gil(|py| {
        use pyo3::types::PyTuple;

        // Create a Python tuple
        let py_tuple = PyTuple::new(py, &[1, 2, 3]).unwrap();

        // Tuples should convert to JSON arrays
        let result = marshal::python_to_json(py, &py_tuple.into_any()).unwrap();

        assert_eq!(result, json!([1, 2, 3]));

        // Nested tuples
        let code = CString::new("((1, 2), (3, 4))").unwrap();
        let nested = py.eval(&code, None, None).unwrap();
        let result = marshal::python_to_json(py, &nested).unwrap();
        assert_eq!(result, json!([[1, 2], [3, 4]]));

        // Mixed tuple
        let code = CString::new("(1, 'two', 3.0, True, None)").unwrap();
        let mixed = py.eval(&code, None, None).unwrap();
        let result = marshal::python_to_json(py, &mixed).unwrap();
        assert_eq!(result, json!([1, "two", 3.0, true, null]));
    });
}

/// Test for numpy array support (currently not supported - will be added in 1.7.3)
#[test]
#[ignore] // Ignore until numpy support is added
fn test_numpy_not_supported_yet() {
    let mut vm = PythonVm::new().unwrap();

    let code = r#"
import numpy as np
arr = np.array([1, 2, 3, 4, 5])
arr
"#;

    // This will fail or return a string representation
    let result = vm.execute(code);

    // For now, we just verify it doesn't crash
    // In Phase 1.7.3, this should properly marshal numpy arrays
    assert!(result.is_ok() || result.is_err());
}

/// Test for complex object support (currently not supported - will be added in 1.7.4)
#[test]
#[ignore] // Ignore until CloudPickle support is added
fn test_complex_objects_not_supported_yet() {
    let mut vm = PythonVm::new().unwrap();

    let code = r#"
class CustomObject:
    def __init__(self):
        self.data = [1, 2, 3]
        self.callback = lambda x: x * 2

obj = CustomObject()
obj
"#;

    // This will fail or return a string representation
    let result = vm.execute(code);

    // For now, we just verify it doesn't crash
    // In Phase 1.7.4, this should use CloudPickle for serialization
    assert!(result.is_ok() || result.is_err());
}
