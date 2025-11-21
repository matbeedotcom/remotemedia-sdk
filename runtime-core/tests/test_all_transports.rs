//! Integration test for all transport plugins via plugin registry

mod fixtures;

use fixtures::mock_transport_plugin::MockTransportPlugin;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::{Manifest, ManifestMetadata, NodeManifest};
use remotemedia_runtime_core::transport::runner::PipelineRunner;
use remotemedia_runtime_core::transport::{
    ClientConfig, ServerConfig, TransportData, TransportPluginRegistry,
};
use std::sync::Arc;

#[tokio::test]
async fn test_all_transports_via_registry() {
    let registry = TransportPluginRegistry::new();

    // Register mock transport (always available)
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register mock plugin");

    // Register gRPC transport if available
    #[cfg(feature = "grpc-transport")]
    {
        use remotemedia_runtime_grpc_transport::GrpcTransportPlugin;
        registry
            .register(Arc::new(GrpcTransportPlugin::new()))
            .expect("Failed to register gRPC plugin");
    }

    // Register WebRTC transport if available
    #[cfg(feature = "webrtc-transport")]
    {
        use remotemedia_runtime_webrtc_transport::WebRtcTransportPlugin;
        registry
            .register(Arc::new(WebRtcTransportPlugin::new()))
            .expect("Failed to register WebRTC plugin");
    }

    // Register HTTP transport if available
    #[cfg(feature = "http-transport")]
    {
        use remotemedia_runtime_http_transport::HttpTransportPlugin;
        registry
            .register(Arc::new(HttpTransportPlugin::new()))
            .expect("Failed to register HTTP plugin");
    }

    // Test mock transport
    test_transport(&registry, "mock", "mock://test").await;

    // Test gRPC transport if available
    #[cfg(feature = "grpc-transport")]
    test_transport(&registry, "grpc", "http://localhost:50051").await;

    // Test WebRTC transport if available
    #[cfg(feature = "webrtc-transport")]
    test_transport(&registry, "webrtc", "webrtc://test").await;

    // Test HTTP transport if available
    #[cfg(feature = "http-transport")]
    test_transport(&registry, "http", "http://localhost:8080").await;
}

async fn test_transport(registry: &TransportPluginRegistry, transport_name: &str, address: &str) {
    println!("Testing transport: {}", transport_name);

    let client_config = ClientConfig {
        address: address.to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({})),
    };

    // Get plugin and create client
    let plugin = registry
        .get(transport_name)
        .expect(&format!("Failed to get {} plugin", transport_name));

    let client = plugin
        .create_client(&client_config)
        .await
        .expect(&format!("Failed to create {} client", transport_name));

    // For mock transport, test basic functionality
    if transport_name == "mock" {
        let manifest = Arc::new(Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "test-transport-pipeline".to_string(),
                description: None,
                created_at: None,
            },
            nodes: vec![NodeManifest {
                id: "test".to_string(),
                node_type: "PassthroughNode".to_string(),
                params: serde_json::json!({}),
                ..Default::default()
            }],
            connections: vec![],
        });

        let input = TransportData {
            data: RuntimeData::Text(format!("Test {}", transport_name)),
            sequence: None,
            metadata: std::collections::HashMap::new(),
        };

        let output = client
            .execute_unary(manifest, input.clone())
            .await
            .expect("Failed to execute");

        assert_eq!(output.data, input.data);
    }

    // For other transports, just verify client creation succeeded
    // (actual functionality would require running servers)
    println!("✓ {} transport client created successfully", transport_name);
}

#[tokio::test]
async fn test_server_creation_for_all_transports() {
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register mock plugin");

    #[cfg(feature = "grpc-transport")]
    {
        use remotemedia_runtime_grpc_transport::GrpcTransportPlugin;
        registry
            .register(Arc::new(GrpcTransportPlugin::new()))
            .expect("Failed to register gRPC plugin");
    }

    #[cfg(feature = "webrtc-transport")]
    {
        use remotemedia_runtime_webrtc_transport::WebRtcTransportPlugin;
        registry
            .register(Arc::new(WebRtcTransportPlugin::new()))
            .expect("Failed to register WebRTC plugin");
    }

    #[cfg(feature = "http-transport")]
    {
        use remotemedia_runtime_http_transport::HttpTransportPlugin;
        registry
            .register(Arc::new(HttpTransportPlugin::new()))
            .expect("Failed to register HTTP plugin");
    }

    // Create a dummy pipeline runner
    let runner = Arc::new(PipelineRunner::new().expect("Failed to create pipeline runner"));

    // Test mock server creation
    test_server_creation(&registry, "mock", "mock://test", runner.clone()).await;

    // Test gRPC server creation if available
    #[cfg(feature = "grpc-transport")]
    test_server_creation(&registry, "grpc", "[::1]:50051", runner.clone()).await;

    // Test WebRTC server creation if available
    #[cfg(feature = "webrtc-transport")]
    test_server_creation(&registry, "webrtc", "webrtc://test", runner.clone()).await;

    // Test HTTP server creation if available
    #[cfg(feature = "http-transport")]
    test_server_creation(&registry, "http", "0.0.0.0:8080", runner.clone()).await;
}

async fn test_server_creation(
    registry: &TransportPluginRegistry,
    transport_name: &str,
    bind_addr: &str,
    runner: Arc<PipelineRunner>,
) {
    println!("Testing server creation for: {}", transport_name);

    let server_config = ServerConfig {
        address: bind_addr.to_string(),
        tls_config: None,
    };

    let plugin = registry
        .get(transport_name)
        .expect(&format!("Failed to get {} plugin", transport_name));

    let server = plugin
        .create_server(&server_config, runner)
        .await
        .expect(&format!("Failed to create {} server", transport_name));

    println!("✓ {} server created successfully", transport_name);

    // Note: We don't actually start the server in tests to avoid port conflicts
    drop(server);
}

#[tokio::test]
async fn test_registry_list_plugins() {
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register mock plugin");

    #[cfg(feature = "grpc-transport")]
    {
        use remotemedia_runtime_grpc_transport::GrpcTransportPlugin;
        registry
            .register(Arc::new(GrpcTransportPlugin::new()))
            .expect("Failed to register gRPC plugin");
    }

    let plugins = registry.list();

    // Should at least have mock
    assert!(plugins.contains(&"mock".to_string()));

    println!("Registered plugins: {:?}", plugins);

    #[cfg(feature = "grpc-transport")]
    assert!(plugins.contains(&"grpc".to_string()));
}

#[tokio::test]
async fn test_registry_error_cases() {
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register mock plugin");

    // Test missing plugin
    let result = registry.get("nonexistent");
    assert!(result.is_none());

    // Test invalid config (plugin-specific validation)
    // Mock accepts everything, so test with a hypothetical strict plugin
    // For now, just verify the mock plugin accepts any config
    let weird_config = ClientConfig {
        address: "mock://test".to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({
            "totally": "random",
            "config": 12345
        })),
    };

    let plugin = registry.get("mock").expect("Failed to get mock plugin");
    let client = plugin
        .create_client(&weird_config)
        .await
        .expect("Mock should accept any config");

    assert!(client.health_check().await.unwrap());
}

#[tokio::test]
async fn test_multiple_clients_same_transport() {
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register mock plugin");

    // Create multiple clients with same transport
    let config1 = ClientConfig {
        address: "mock://test1".to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({})),
    };

    let config2 = ClientConfig {
        address: "mock://test2".to_string(),
        auth_token: None,
        timeout_ms: None,
        extra_config: Some(serde_json::json!({})),
    };

    let plugin = registry.get("mock").expect("Failed to get mock plugin");

    let client1 = plugin
        .create_client(&config1)
        .await
        .expect("Failed to create client 1");

    let client2 = plugin
        .create_client(&config2)
        .await
        .expect("Failed to create client 2");

    // Both should work independently
    assert!(client1.health_check().await.unwrap());
    assert!(client2.health_check().await.unwrap());
}

#[tokio::test]
async fn test_transport_plugin_reregistration() {
    let registry = TransportPluginRegistry::new();
    registry
        .register(Arc::new(MockTransportPlugin))
        .expect("Failed to register mock plugin");

    // Try to register again - should fail (not replace)
    let result = registry.register(Arc::new(MockTransportPlugin));
    assert!(result.is_err());

    let plugins = registry.list();
    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains(&"mock".to_string()));
}
