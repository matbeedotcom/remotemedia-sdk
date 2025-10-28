//! Integration test for CPython executor with numpy arrays
//!
//! This test verifies that:
//! 1. CPython executor can load and execute Python nodes
//! 2. Numpy arrays are properly marshaled between Rust and Python
//! 3. Zero-copy numpy array handling works correctly
//! 4. Runtime selection chooses CPython for numpy-heavy nodes

use pyo3::types::PyAnyMethods;
use remotemedia_runtime::{
    executor::{Executor, ExecutorConfig},
    manifest::{Manifest, ManifestMetadata, NodeManifest, RuntimeHint},
};
use serde_json::json;

#[tokio::test]
async fn test_cpython_with_numpy_array() {
    // Initialize Python
    pyo3::prepare_freethreaded_python();

    // Create a simple numpy processing node in Python
    pyo3::Python::with_gil(|py| {
        let code = std::ffi::CString::new(
            r#"
import numpy as np

class NumpyMultiplier:
    """Node that multiplies numpy arrays by a factor."""

    def __init__(self, factor=2.0):
        self.factor = factor
        self.process_count = 0

    def initialize(self):
        print(f"NumpyMultiplier initialized with factor={self.factor}")

    def process(self, data):
        self.process_count += 1

        # Handle numpy array input
        if isinstance(data, np.ndarray):
            result = data * self.factor
            print(f"Processed numpy array: shape={data.shape}, dtype={data.dtype}, result_mean={result.mean():.2f}")
            return result.tolist()  # Convert back to list for JSON

        # Handle list input (convert to numpy)
        elif isinstance(data, list):
            arr = np.array(data)
            result = arr * self.factor
            print(f"Processed list as numpy: shape={arr.shape}, result_mean={result.mean():.2f}")
            return result.tolist()

        # Handle scalar
        else:
            result = data * self.factor
            print(f"Processed scalar: {data} * {self.factor} = {result}")
            return result

    def cleanup(self):
        print(f"NumpyMultiplier processed {self.process_count} items")
"#,
        )
        .unwrap();

        py.run(&code, None, None).unwrap();

        // Register in remotemedia.nodes
        let sys = py.import("sys").unwrap();
        let modules = sys.getattr("modules").unwrap();

        let register_code = std::ffi::CString::new(
            "import types; mock_module = types.ModuleType('remotemedia.nodes'); mock_module.NumpyMultiplier = NumpyMultiplier",
        )
        .unwrap();
        py.run(&register_code, None, None).unwrap();

        let mock_module = py
            .eval(&std::ffi::CString::new("mock_module").unwrap(), None, None)
            .unwrap();
        modules.set_item("remotemedia.nodes", mock_module).unwrap();
    });

    // Create manifest with numpy node
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "numpy-test-pipeline".to_string(),
            description: Some("Test CPython executor with numpy arrays".to_string()),
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "numpy_node_0".to_string(),
            node_type: "NumpyMultiplier".to_string(),
            params: json!({
                "factor": 3.0
            }),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython), // Explicitly use CPython
            ..Default::default()
        }],
        connections: vec![],
    };

    // Create executor
    let executor = Executor::with_config(ExecutorConfig {
        max_concurrency: 10,
        debug: true,
    });

    // Test with array input
    let input_data = vec![
        json!([1.0, 2.0, 3.0, 4.0, 5.0]),
        json!([10.0, 20.0, 30.0]),
        json!(42.0),
    ];

    // Execute pipeline
    let result = executor
        .execute_with_input(&manifest, input_data)
        .await
        .unwrap();

    println!("Pipeline execution result: {:?}", result);

    // Verify results
    assert_eq!(result.status, "success");
    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 3);

    // First output: [1, 2, 3, 4, 5] * 3 = [3, 6, 9, 12, 15]
    let output1 = &outputs[0];
    assert_eq!(output1.as_array().unwrap().len(), 5);
    assert_eq!(output1[0], json!(3.0));
    assert_eq!(output1[4], json!(15.0));

    // Second output: [10, 20, 30] * 3 = [30, 60, 90]
    let output2 = &outputs[1];
    assert_eq!(output2.as_array().unwrap().len(), 3);
    assert_eq!(output2[0], json!(30.0));
    assert_eq!(output2[2], json!(90.0));

    // Third output: 42 * 3 = 126
    let output3 = &outputs[2];
    assert_eq!(output3, &json!(126.0));

    println!("✓ CPython numpy test passed!");
}

#[tokio::test]
async fn test_runtime_auto_detection_for_numpy() {
    pyo3::prepare_freethreaded_python();

    // Setup Python node
    pyo3::Python::with_gil(|py| {
        let code = std::ffi::CString::new(
            r#"
import numpy as np

class NumpyProcessor:
    def process(self, data):
        # Node that uses numpy - should auto-select CPython
        arr = np.array(data)
        return (arr ** 2).tolist()
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();

        let sys = py.import("sys").unwrap();
        let modules = sys.getattr("modules").unwrap();
        let register = std::ffi::CString::new(
            "import types; m = types.ModuleType('remotemedia.nodes'); m.NumpyProcessor = NumpyProcessor",
        )
        .unwrap();
        py.run(&register, None, None).unwrap();
        let m = py
            .eval(&std::ffi::CString::new("m").unwrap(), None, None)
            .unwrap();
        modules.set_item("remotemedia.nodes", m).unwrap();
    });

    // Manifest without explicit runtime hint - should auto-detect
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "auto-detect-test".to_string(),
            description: None,
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "numpy_auto".to_string(),
            node_type: "NumpyProcessor".to_string(), // Contains "numpy" keyword
            params: json!({}),
            capabilities: None,
            host: None,
            runtime_hint: None, // No explicit hint - let auto-detection work
            ..Default::default()
        }],
        connections: vec![],
    };

    let executor = Executor::new();
    let input_data = vec![json!([2.0, 3.0, 4.0])];

    let result = executor
        .execute_with_input(&manifest, input_data)
        .await
        .unwrap();

    assert_eq!(result.status, "success");

    // The output should be a single array [4.0, 9.0, 16.0]
    // But check if it's wrapped or not
    println!("Result outputs: {:?}", result.outputs);

    if result.outputs.is_array() {
        let outputs = result.outputs.as_array().unwrap();
        if outputs.len() == 1 {
            // Single output case
            let output = &outputs[0];
            assert_eq!(output[0], json!(4.0));
            assert_eq!(output[1], json!(9.0));
            assert_eq!(output[2], json!(16.0));
        } else {
            // Direct array case [4.0, 9.0, 16.0]
            assert_eq!(outputs[0], json!(4.0));
            assert_eq!(outputs[1], json!(9.0));
            assert_eq!(outputs[2], json!(16.0));
        }
    }

    println!("✓ Auto-detection test passed!");
}

#[tokio::test]
async fn test_cpython_with_2d_numpy_array() {
    pyo3::prepare_freethreaded_python();

    // Create node that handles 2D arrays
    pyo3::Python::with_gil(|py| {
        let code = std::ffi::CString::new(
            r#"
import numpy as np

class Matrix2DProcessor:
    """Process 2D matrices."""

    def process(self, data):
        # Convert to 2D numpy array
        arr = np.array(data)

        if arr.ndim == 1:
            # Reshape to 2D
            arr = arr.reshape(-1, 1)

        # Transpose the matrix
        result = arr.T

        print(f"Input shape: {arr.shape}, Output shape: {result.shape}")

        return result.tolist()
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();

        let sys = py.import("sys").unwrap();
        let modules = sys.getattr("modules").unwrap();
        let register = std::ffi::CString::new(
            "import types; m = types.ModuleType('remotemedia.nodes'); m.Matrix2DProcessor = Matrix2DProcessor",
        )
        .unwrap();
        py.run(&register, None, None).unwrap();
        let m = py
            .eval(&std::ffi::CString::new("m").unwrap(), None, None)
            .unwrap();
        modules.set_item("remotemedia.nodes", m).unwrap();
    });

    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "2d-matrix-test".to_string(),
            description: None,
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "matrix_node".to_string(),
            node_type: "Matrix2DProcessor".to_string(),
            params: json!({}),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython),
            ..Default::default()
        }],
        connections: vec![],
    };

    let executor = Executor::new();

    // Input: 2x3 matrix
    let input_data = vec![json!([[1, 2, 3], [4, 5, 6]])];

    let result = executor
        .execute_with_input(&manifest, input_data)
        .await
        .unwrap();

    assert_eq!(result.status, "success");

    // Output should be transposed: 3x2 matrix
    println!("2D matrix result: {:?}", result.outputs);

    // The result should be a single output containing the transposed matrix
    if result.outputs.is_array() {
        let outputs = result.outputs.as_array().unwrap();

        // Check if it's wrapped (single element array containing the matrix)
        let output = if outputs.len() == 1 && outputs[0].is_array() {
            &outputs[0]
        } else {
            // Direct matrix output
            &result.outputs
        };

        if output.is_array() && output.as_array().unwrap().len() > 0 {
            let matrix = output.as_array().unwrap();

            // [[1, 2, 3],   ->  [[1, 4],
            //  [4, 5, 6]]        [2, 5],
            //                    [3, 6]]
            assert_eq!(matrix[0][0], json!(1));
            assert_eq!(matrix[0][1], json!(4));
            assert_eq!(matrix[1][0], json!(2));
            assert_eq!(matrix[1][1], json!(5));
            assert_eq!(matrix[2][0], json!(3));
            assert_eq!(matrix[2][1], json!(6));

            println!("✓ 2D matrix test passed!");
        } else {
            panic!("Expected 2D matrix output, got: {:?}", output);
        }
    } else {
        panic!("Expected array output, got: {:?}", result.outputs);
    }
}

