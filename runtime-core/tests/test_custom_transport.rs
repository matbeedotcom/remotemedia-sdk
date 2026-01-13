//! Integration test for custom transport plugin with RemotePipelineNode

mod fixtures;

use fixtures::mock_transport_plugin::MockTransportPlugin;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_runtime_core::transport::{ClientConfig, TransportData, TransportPluginRegistry};
use std::sync::Arc;

#[tokio::test]
async fn test_remote_pipeline_node_with_mock_transport() {
    // Register mock transport plugin
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register plugin");

    // Create a simple remote pipeline manifest
    let remote_manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "test-pipeline".to_string(),
                ..Default::default()
            },
        nodes: vec![NodeManifest {
            id: "echo_node".to_string(),
            node_type: "PassthroughNode".to_string(),
            params: serde_json::json!({}),
            ..Default::default()
        }],
        connections: vec![],
    };

    // Create client config
    let client_config = ClientConfig {
        address: "mock://test".to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({})),
    };

    // Create client from registry
    let plugin = registry.get("mock").expect("Failed to get plugin");
    let client = plugin
        .create_client(&client_config)
        .await
        .expect("Failed to create mock client");

    // Test unary execution
    let input = TransportData {
        data: RuntimeData::Text("Hello, Mock Transport!".to_string()),
        sequence: None,
        metadata: std::collections::HashMap::new(),
    };

    let output = client
        .execute_unary(Arc::new(remote_manifest.clone()), input.clone())
        .await
        .expect("Failed to execute unary");

    // Verify echo behavior
    assert_eq!(output.data, input.data);

    // Test health check
    let health = client.health_check().await.expect("Health check failed");
    assert!(health);
}

#[tokio::test]
async fn test_remote_pipeline_node_streaming_with_mock_transport() {
    // Register mock transport plugin
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register plugin");

    // Create a simple remote pipeline manifest
    let remote_manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "test-streaming-pipeline".to_string(),
                ..Default::default()
            },
        nodes: vec![NodeManifest {
            id: "stream_node".to_string(),
            node_type: "PassthroughNode".to_string(),
            params: serde_json::json!({}),
            ..Default::default()
        }],
        connections: vec![],
    };

    // Create client config
    let client_config = ClientConfig {
        address: "mock://test".to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({})),
    };

    // Create client from registry
    let plugin = registry.get("mock").expect("Failed to get plugin");
    let client = plugin
        .create_client(&client_config)
        .await
        .expect("Failed to create mock client");

    // Create stream session
    let mut session = client
        .create_stream_session(Arc::new(remote_manifest))
        .await
        .expect("Failed to create stream session");

    // Verify session is active
    assert!(session.is_active());

    // Send some data
    let input1 = TransportData {
        data: RuntimeData::Text("Message 1".to_string()),
        sequence: Some(1),
        metadata: std::collections::HashMap::new(),
    };

    let input2 = TransportData {
        data: RuntimeData::Text("Message 2".to_string()),
        sequence: Some(2),
        metadata: std::collections::HashMap::new(),
    };

    session.send(input1.clone()).await.expect("Failed to send");
    session.send(input2.clone()).await.expect("Failed to send");

    // Receive data (LIFO due to Vec buffer)
    let output2 = session
        .receive()
        .await
        .expect("Failed to receive")
        .expect("Expected data");
    let output1 = session
        .receive()
        .await
        .expect("Failed to receive")
        .expect("Expected data");

    // Verify echo behavior
    assert_eq!(output1.data, input1.data);
    assert_eq!(output2.data, input2.data);

    // Close session
    session.close().await.expect("Failed to close session");
}

#[tokio::test]
async fn test_mock_transport_validation() {
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register plugin");

    // Test that mock transport validates any config
    let client_config = ClientConfig {
        address: "mock://test".to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({
            "arbitrary": "config",
            "should": "work"
        })),
    };

    let plugin = registry.get("mock").expect("Failed to get plugin");
    let client = plugin
        .create_client(&client_config)
        .await
        .expect("Mock transport should accept any config");

    // Verify client works
    assert!(client.health_check().await.unwrap());
}

#[tokio::test]
async fn test_registry_error_on_missing_plugin() {
    let registry = TransportPluginRegistry::new();

    let result = registry.get("nonexistent");

    assert!(result.is_none());
}
