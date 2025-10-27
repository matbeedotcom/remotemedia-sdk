//! Integration tests for Python SDK nodes via CPython executor
//!
//! These tests validate that we can execute actual Python SDK nodes
//! from remotemedia.nodes using the Rust runtime.

use pyo3::types::PyAnyMethods;
use remotemedia_runtime::{
    executor::{Executor, ExecutorConfig},
    manifest::{Manifest, ManifestMetadata, NodeManifest, RuntimeHint},
};
use serde_json::json;
use std::ffi::CString;

/// Helper to add python-client to Python path
fn setup_python_sdk_path() {
    pyo3::prepare_freethreaded_python();

    pyo3::Python::with_gil(|py| {
        // Add the python-client directory to sys.path
        let python_client_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("python-client");

        if python_client_path.exists() {
            let path_str = python_client_path.to_str().unwrap();
            let append_code = CString::new(format!(
                "import sys; sys.path.insert(0, r'{}')",
                path_str
            ))
            .unwrap();
            py.run(&append_code, None, None).unwrap();

            println!("✓ Added Python SDK to path: {}", path_str);
        } else {
            println!("⚠ Python SDK path not found: {:?}", python_client_path);
        }
    });
}

#[tokio::test]
async fn test_passthrough_node_from_sdk() {
    setup_python_sdk_path();

    // Test PassThroughNode from remotemedia.nodes
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "passthrough-sdk-test".to_string(),
            description: Some("Test PassThroughNode from Python SDK".to_string()),
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "passthrough".to_string(),
            node_type: "PassThroughNode".to_string(),
            params: json!({}),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython),
            ..Default::default()
        }],
        connections: vec![],
    };

    let executor = Executor::with_config(ExecutorConfig {
        max_concurrency: 10,
        debug: true,
    });

    // Test with various data types
    let test_inputs = vec![
        json!(42),
        json!("hello world"),
        json!([1, 2, 3, 4, 5]),
        json!({"key": "value", "number": 123}),
    ];

    let result = executor
        .execute_with_input(&manifest, test_inputs.clone())
        .await
        .unwrap();

    println!("PassThroughNode result: {:?}", result);
    assert_eq!(result.status, "success");

    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 4);

    // Verify outputs match inputs (passthrough)
    assert_eq!(outputs[0], json!(42));
    assert_eq!(outputs[1], json!("hello world"));
    assert_eq!(outputs[2], json!([1, 2, 3, 4, 5]));
    assert_eq!(outputs[3], json!({"key": "value", "number": 123}));

    println!("✓ PassThroughNode test passed!");
}

#[tokio::test]
async fn test_calculator_node_from_sdk() {
    setup_python_sdk_path();

    // Test CalculatorNode from remotemedia.nodes.calculator
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "calculator-sdk-test".to_string(),
            description: Some("Test CalculatorNode from Python SDK".to_string()),
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "calc".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: json!({
                "operation": "multiply",
                "operand": 3
            }),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython),
            ..Default::default()
        }],
        connections: vec![],
    };

    let executor = Executor::new();

    // Test multiplication: [2, 5, 10] * 3 = [6, 15, 30]
    let test_inputs = vec![json!(2), json!(5), json!(10)];

    let result = executor
        .execute_with_input(&manifest, test_inputs)
        .await
        .unwrap();

    println!("CalculatorNode result: {:?}", result);
    assert_eq!(result.status, "success");

    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 3);

    assert_eq!(outputs[0], json!(6));
    assert_eq!(outputs[1], json!(15));
    assert_eq!(outputs[2], json!(30));

    println!("✓ CalculatorNode test passed!");
}

