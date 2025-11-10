//! Integration tests for RemotePipelineNode
//!
//! These tests verify that RemotePipelineNode can:
//! - Connect to remote gRPC servers
//! - Execute remote pipelines with retry and timeout
//! - Handle errors gracefully
//!
//! Note: These tests require a running gRPC server or use mock servers.

#![cfg(test)]
#![cfg(feature = "grpc-client")]

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::nodes::{AsyncStreamingNode, remote_pipeline::RemotePipelineNode};
use serde_json::json;

/// Test that RemotePipelineNode can be created with valid configuration
#[tokio::test]
async fn test_create_remote_node() {
    let params = json!({
        "transport": "grpc",
        "endpoint": "localhost:50051",
        "manifest": {
            "version": "v1",
            "nodes": [{
                "id": "echo",
                "node_type": "PassThrough",
                "params": {}
            }],
            "connections": []
        },
        "timeout_ms": 5000,
        "retry": {
            "max_retries": 2,
            "backoff_ms": 500
        }
    });

    let result = RemotePipelineNode::new("test_remote".to_string(), params);
    assert!(result.is_ok());

    let node = result.unwrap();
    assert_eq!(node.node_id, "test_remote");
    assert_eq!(node.node_type(), "RemotePipelineNode");
}

/// Test that RemotePipelineNode rejects invalid configuration
#[tokio::test]
async fn test_create_remote_node_invalid_config() {
    // Missing endpoint
    let params = json!({
        "transport": "grpc",
        "manifest": {
            "version": "v1",
            "nodes": [],
            "connections": []
        }
    });

    let result = RemotePipelineNode::new("test_remote".to_string(), params);
    assert!(result.is_err());
}

/// Test that RemotePipelineNode rejects unsupported transport
#[tokio::test]
async fn test_create_remote_node_unsupported_transport() {
    let params = json!({
        "transport": "unknown",
        "endpoint": "localhost:50051",
        "manifest": {
            "version": "v1",
            "nodes": [],
            "connections": []
        }
    });

    let result = RemotePipelineNode::new("test_remote".to_string(), params);
    assert!(result.is_err());
}

/// Test single remote node execution
///
/// This test requires a running gRPC server or uses a mock server.
/// Currently disabled until mock server is implemented.
#[tokio::test]
#[ignore] // Enable when mock server is available
async fn test_single_remote_node() {
    let params = json!({
        "transport": "grpc",
        "endpoint": "localhost:50051",
        "manifest": {
            "version": "v1",
            "nodes": [{
                "id": "echo",
                "node_type": "PassThrough",
                "params": {}
            }],
            "connections": []
        },
        "timeout_ms": 5000
    });

    let mut node = RemotePipelineNode::new("test_remote".to_string(), params).unwrap();
    node.initialize().await.unwrap();

    let input = RuntimeData::Text("Hello, World!".to_string());
    let result = node.process(input.clone()).await;

    // Should succeed with mock server
    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output, input);
}

/// Test remote execution timeout
///
/// This test verifies that timeouts are enforced correctly.
/// Currently disabled until mock server is implemented.
#[tokio::test]
#[ignore] // Enable when mock server is available
async fn test_remote_timeout() {
    let params = json!({
        "transport": "grpc",
        "endpoint": "localhost:50051",
        "manifest": {
            "version": "v1",
            "nodes": [{
                "id": "slow",
                "node_type": "SlowNode", // Mock node that takes >timeout
                "params": {"delay_ms": 10000}
            }],
            "connections": []
        },
        "timeout_ms": 1000 // 1 second timeout
    });

    let mut node = RemotePipelineNode::new("test_remote".to_string(), params).unwrap();
    node.initialize().await.unwrap();

    let input = RuntimeData::Text("Test".to_string());
    let result = node.process(input).await;

    // Should fail with timeout error
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("timeout") || err_msg.contains("Timeout"));
}

/// Test remote execution retry logic
///
/// This test verifies that retries work correctly.
/// Currently disabled until mock server is implemented.
#[tokio::test]
#[ignore] // Enable when mock server is available
async fn test_remote_retry() {
    let params = json!({
        "transport": "grpc",
        "endpoint": "localhost:50051",
        "manifest": {
            "version": "v1",
            "nodes": [{
                "id": "flaky",
                "node_type": "FlakyNode", // Mock node that fails first 2 attempts
                "params": {"fail_count": 2}
            }],
            "connections": []
        },
        "timeout_ms": 5000,
        "retry": {
            "max_retries": 3,
            "backoff_ms": 100
        }
    });

    let mut node = RemotePipelineNode::new("test_remote".to_string(), params).unwrap();
    node.initialize().await.unwrap();

    let input = RuntimeData::Text("Test".to_string());
    let result = node.process(input.clone()).await;

    // Should succeed on 3rd attempt
    assert!(result.is_ok());
}

/// Test authentication token environment variable substitution
#[tokio::test]
async fn test_auth_token_env_substitution() {
    std::env::set_var("TEST_API_TOKEN", "secret123");

    let params = json!({
        "transport": "grpc",
        "endpoint": "localhost:50051",
        "manifest": {
            "version": "v1",
            "nodes": [],
            "connections": []
        },
        "auth_token": "${TEST_API_TOKEN}"
    });

    let node = RemotePipelineNode::new("test_remote".to_string(), params).unwrap();
    assert_eq!(node.config.auth_token, Some("secret123".to_string()));

    std::env::remove_var("TEST_API_TOKEN");
}
