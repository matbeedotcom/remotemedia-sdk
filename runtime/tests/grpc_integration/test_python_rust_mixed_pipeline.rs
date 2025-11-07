// ! Integration tests for mixed Rust + Python node pipelines via gRPC
//!
//! Tests various scenarios of Python streaming nodes (N>=1 yields) combined with Rust nodes.

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    data_buffer, execute_response::Outcome,
    pipeline_execution_service_client::PipelineExecutionServiceClient, Connection, DataBuffer,
    ExecuteRequest, JsonData, NodeManifest, PipelineManifest,
};
use std::collections::HashMap;

use super::test_helpers;

/// Test Python ExpanderNode (yields N items) → Rust Calculator (processes each)
#[tokio::test]
async fn test_python_expander_to_rust_calculator() {
    // Start test server
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    // Create manifest: ExpanderNode (Python, yields 3) → MultiplyNode (Rust)
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![
            NodeManifest {
                id: "expander".to_string(),
                node_type: "ExpanderNode".to_string(),
                params: r#"{"expansion_factor": 3}"#.to_string(),
                is_streaming: true,
                capabilities: None,
                host: String::new(),
                runtime_hint: 1, // Python
                input_types: vec![],
                output_types: vec![],
            },
            NodeManifest {
                id: "multiply".to_string(),
                node_type: "CalculatorNode".to_string(),
                params: r#"{"operation": "multiply", "value": 2.0}"#.to_string(),
                is_streaming: false,
                capabilities: None,
                host: String::new(),
                runtime_hint: 0, // Rust
                input_types: vec![],
                output_types: vec![],
            },
        ],
        connections: vec![Connection {
            from: "expander".to_string(),
            to: "multiply".to_string(),
        }],
    };

    // Create input: single value that will be expanded to 3
    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "expander".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"value": 10}"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    // Execute pipeline
    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    // Verify: ExpanderNode should yield 3 items (10, 11, 12)
    // Each should be multiplied by 2: (20, 22, 24)
    assert!(response.outcome.is_some(), "Expected outcome");
    let outputs = match response.outcome.unwrap() {
        Outcome::Result(result) => result.data_outputs,
        Outcome::Error(err) => panic!("Execution failed: {:?}", err),
    };

    // Should have output from 'multiply' node
    assert!(
        outputs.contains_key("multiply"),
        "Missing multiply node output"
    );

    // The multiply node should have processed 3 items (expanded from 1)
    // Note: This assumes the executor returns Vec<DataBuffer>
    println!("Response outputs: {:?}", outputs);
}

/// Test Python RangeGeneratorNode (yields 5 items) → Rust PassThrough
#[tokio::test]
async fn test_python_range_generator() {
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![NodeManifest {
            id: "generator".to_string(),
            node_type: "RangeGeneratorNode".to_string(),
            params: r#"{}"#.to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 1, // Python
            input_types: vec![],
            output_types: vec![],
        }],
        connections: vec![],
    };

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "generator".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"start": 0, "end": 5, "step": 1}"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    assert!(response.outcome.is_some());
    let outputs = match response.outcome.unwrap() {
        Outcome::Result(result) => result.data_outputs,
        Outcome::Error(err) => panic!("Execution failed: {:?}", err),
    };

    // Generator should output 5 items (0, 1, 2, 3, 4)
    assert!(outputs.contains_key("generator"));
    println!("Range generator outputs: {:?}", outputs);
}

/// Test Rust node → Python streaming node → Rust node (full chain)
#[tokio::test]
async fn test_rust_python_rust_chain() {
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    // Chain: Calculator (Rust) → ExpanderNode (Python, 1→3) → Calculator (Rust)
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![
            NodeManifest {
                id: "add10".to_string(),
                node_type: "CalculatorNode".to_string(),
                params: r#"{"operation": "add", "value": 10.0}"#.to_string(),
                is_streaming: false,
                capabilities: None,
                host: String::new(),
                runtime_hint: 0, // Rust
                input_types: vec![],
                output_types: vec![],
            },
            NodeManifest {
                id: "expander".to_string(),
                node_type: "ExpanderNode".to_string(),
                params: r#"{"expansion_factor": 3}"#.to_string(),
                is_streaming: true,
                capabilities: None,
                host: String::new(),
                runtime_hint: 1, // Python
                input_types: vec![],
                output_types: vec![],
            },
            NodeManifest {
                id: "multiply2".to_string(),
                node_type: "CalculatorNode".to_string(),
                params: r#"{"operation": "multiply", "value": 2.0}"#.to_string(),
                is_streaming: false,
                capabilities: None,
                host: String::new(),
                runtime_hint: 0, // Rust
                input_types: vec![],
                output_types: vec![],
            },
        ],
        connections: vec![
            Connection {
                from: "add10".to_string(),
                to: "expander".to_string(),
            },
            Connection {
                from: "expander".to_string(),
                to: "multiply2".to_string(),
            },
        ],
    };

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "add10".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"5"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    assert!(response.outcome.is_some());

    // Flow: 5 → +10 = 15 → expand to (15, 16, 17) → ×2 = (30, 32, 34)
    match &response.outcome {
        Some(Outcome::Result(result)) => println!("Rust→Python→Rust chain result: {:?}", result),
        Some(Outcome::Error(err)) => panic!("Execution failed: {:?}", err),
        None => panic!("No outcome in response"),
    }
}

/// Test Python TransformAndExpandNode (yields 3 transformed versions)
#[tokio::test]
async fn test_python_transform_and_expand() {
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![NodeManifest {
            id: "transformer".to_string(),
            node_type: "TransformAndExpandNode".to_string(),
            params: r#"{"transforms": ["upper", "lower", "reverse"]}"#.to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 1, // Python
            input_types: vec![],
            output_types: vec![],
        }],
        connections: vec![],
    };

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "transformer".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#""Hello""#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    assert!(response.outcome.is_some());

    // Should yield 3 transformations: HELLO, hello, olleH
    match &response.outcome {
        Some(Outcome::Result(result)) => println!("Transform and expand result: {:?}", result),
        Some(Outcome::Error(err)) => panic!("Execution failed: {:?}", err),
        None => panic!("No outcome in response"),
    }
}

/// Test Python ChainedTransformNode (yields intermediate results)
#[tokio::test]
async fn test_python_chained_transform() {
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![NodeManifest {
            id: "chained".to_string(),
            node_type: "ChainedTransformNode".to_string(),
            params: r#"{"emit_intermediates": true}"#.to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 1, // Python
            input_types: vec![],
            output_types: vec![],
        }],
        connections: vec![],
    };

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "chained".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"value": 5}"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    assert!(response.outcome.is_some());

    // Flow: 5 → ×2=10 (yield) → +10=20 (yield) → ^2=400 (yield final)
    // Should yield 3 items showing transformation stages
    match &response.outcome {
        Some(Outcome::Result(result)) => println!("Chained transform result: {:?}", result),
        Some(Outcome::Error(err)) => panic!("Execution failed: {:?}", err),
        None => panic!("No outcome in response"),
    }
}

/// Test Python ConditionalExpanderNode (variable N based on input)
#[tokio::test]
async fn test_python_conditional_expander() {
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![NodeManifest {
            id: "conditional".to_string(),
            node_type: "ConditionalExpanderNode".to_string(),
            params: r#"{}"#.to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 1, // Python
            input_types: vec![],
            output_types: vec![],
        }],
        connections: vec![],
    };

    // Test with value=5 (should yield 5 items)
    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "conditional".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"value": 5}"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    assert!(response.outcome.is_some());

    // Should yield 5 items (one for each expansion)
    match &response.outcome {
        Some(Outcome::Result(result)) => {
            println!("Conditional expander (value=5) result: {:?}", result)
        }
        Some(Outcome::Error(err)) => panic!("Execution failed: {:?}", err),
        None => panic!("No outcome in response"),
    }
}

/// Test multiple Python streaming nodes in sequence
#[tokio::test]
async fn test_python_streaming_chain() {
    let addr = test_helpers::start_test_server().await;
    assert!(
        test_helpers::wait_for_server(&addr, 10).await,
        "Server failed to start"
    );

    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to server");

    // Chain: RangeGenerator (1→5) → ExpanderNode (5→15) → FilterNode (15→N)
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: None,
        nodes: vec![
            NodeManifest {
                id: "generator".to_string(),
                node_type: "RangeGeneratorNode".to_string(),
                params: r#"{}"#.to_string(),
                is_streaming: true,
                capabilities: None,
                host: String::new(),
                runtime_hint: 1, // Python
                input_types: vec![],
                output_types: vec![],
            },
            NodeManifest {
                id: "expander".to_string(),
                node_type: "ExpanderNode".to_string(),
                params: r#"{"expansion_factor": 3}"#.to_string(),
                is_streaming: true,
                capabilities: None,
                host: String::new(),
                runtime_hint: 1, // Python
                input_types: vec![],
                output_types: vec![],
            },
            NodeManifest {
                id: "filter".to_string(),
                node_type: "FilterNode".to_string(),
                params: r#"{"min_value": 2.0}"#.to_string(),
                is_streaming: true,
                capabilities: None,
                host: String::new(),
                runtime_hint: 1, // Python
                input_types: vec![],
                output_types: vec![],
            },
        ],
        connections: vec![
            Connection {
                from: "generator".to_string(),
                to: "expander".to_string(),
            },
            Connection {
                from: "expander".to_string(),
                to: "filter".to_string(),
            },
        ],
    };

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "generator".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"5"#.to_string(),
                schema_type: String::new(),
            })), // Range 0..5
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test".to_string(),
    };

    let response = client
        .execute_pipeline(tonic::Request::new(request))
        .await
        .expect("Failed to execute pipeline")
        .into_inner();

    assert!(response.outcome.is_some());

    // Flow: Range 0..5 (5 items) → each expanded to 3 (15 items) → filter >=2
    match &response.outcome {
        Some(Outcome::Result(result)) => println!("Python streaming chain result: {:?}", result),
        Some(Outcome::Error(err)) => panic!("Execution failed: {:?}", err),
        None => panic!("No outcome in response"),
    }
}
