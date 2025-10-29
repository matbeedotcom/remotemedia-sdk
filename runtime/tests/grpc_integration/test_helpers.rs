//! Test helpers for gRPC integration tests
//!
//! Provides utilities for starting test servers, creating test clients,
//! and common test data structures.

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::executor::Executor;
use remotemedia_runtime::grpc_service::{server::GrpcServer, ServiceConfig};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// Start a test gRPC server and return its address
///
/// Server runs in background task with test-friendly configuration:
/// - Random available port
/// - Auth disabled
/// - Permissive resource limits
/// - Structured logging disabled for cleaner test output
pub async fn start_test_server() -> String {
    // Create test configuration
    let mut config = ServiceConfig::default();
    
    // Bind to random port on localhost
    config.bind_address = "[::1]:0".to_string();
    
    // Disable auth for tests
    config.auth.require_auth = false;
    
    // Permissive limits for tests
    config.limits.max_memory_bytes = 500_000_000; // 500MB
    config.limits.max_timeout = Duration::from_secs(30);
    
    // Disable JSON logging for cleaner test output
    config.json_logging = false;
    
    // Create executor
    let executor = Arc::new(Executor::new());
    
    // Create server
    let _server = GrpcServer::new(config, executor).expect("Failed to create test server");
    
    // Get a random available port
    let listener = tokio::net::TcpListener::bind("[::1]:0")
        .await
        .expect("Failed to bind to random port");
    
    let addr = listener.local_addr().expect("Failed to get local addr");
    drop(listener); // Close listener so server can bind to it
    
    // Spawn server in background
    let server_addr_str = addr.to_string();
    let server_addr_clone = server_addr_str.clone();
    
    tokio::spawn(async move {
        // Update config with actual bound address
        let mut config = ServiceConfig::default();
        config.bind_address = server_addr_clone;
        config.auth.require_auth = false;
        config.limits.max_memory_bytes = 500_000_000;
        config.limits.max_timeout = Duration::from_secs(30);
        config.json_logging = false;
        
        let executor = Arc::new(Executor::new());
        let server = GrpcServer::new(config, executor).expect("Failed to create server");
        
        // Run server (this will block until shutdown)
        let _ = server.serve().await;
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    server_addr_str
}

/// Wait for server to be ready by attempting connection
pub async fn wait_for_server(addr: &str, max_attempts: u32) -> bool {
    use remotemedia_runtime::grpc_service::generated::pipeline_execution_service_client::PipelineExecutionServiceClient;
    
    for attempt in 1..=max_attempts {
        match PipelineExecutionServiceClient::connect(format!("http://{}", addr)).await {
            Ok(_) => {
                return true;
            }
            Err(_) => {
                if attempt < max_attempts {
                    sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
    
    false
}

/// Create a simple test audio buffer (sine wave)
pub fn create_test_audio_buffer(
    sample_rate: u32,
    duration_sec: u32,
    frequency_hz: f32,
) -> remotemedia_runtime::grpc_service::generated::AudioBuffer {
    use remotemedia_runtime::grpc_service::generated::AudioFormat;
    
    let num_samples = sample_rate * duration_sec;
    let samples: Vec<f32> = (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (t * frequency_hz * 2.0 * std::f32::consts::PI).sin() * 0.5
        })
        .collect();
    
    let audio_bytes: Vec<u8> = samples
        .iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();
    
    remotemedia_runtime::grpc_service::generated::AudioBuffer {
        samples: audio_bytes,
        sample_rate,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: num_samples as u64,
    }
}

/// Create a simple calculator pipeline manifest
pub fn create_calculator_manifest(
    name: &str,
    operation: &str,
    value: f64,
) -> remotemedia_runtime::grpc_service::generated::PipelineManifest {
    use remotemedia_runtime::grpc_service::generated::{
        ManifestMetadata, NodeManifest, PipelineManifest,
    };
    
    PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: name.to_string(),
            description: format!("Test pipeline: {}", name),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
        nodes: vec![NodeManifest {
            id: "calc".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: format!(r#"{{"operation": "{}", "value": {}}}"#, operation, value),
            is_streaming: false,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,
            input_types: vec![3], // JSON
            output_types: vec![3], // JSON
        }],
        connections: vec![],
    }
}

/// Create a simple passthrough pipeline manifest
pub fn create_passthrough_manifest(
    name: &str,
) -> remotemedia_runtime::grpc_service::generated::PipelineManifest {
    use remotemedia_runtime::grpc_service::generated::{
        ManifestMetadata, NodeManifest, PipelineManifest,
    };

    PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: name.to_string(),
            description: format!("PassThrough pipeline: {}", name),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
        nodes: vec![NodeManifest {
            id: "passthrough".to_string(),
            node_type: "PassThrough".to_string(),
            params: "{}".to_string(),
            is_streaming: false,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,
            input_types: vec![1], // Audio
            output_types: vec![1], // Audio
        }],
        connections: vec![],
    }
}

/// Wrap AudioBuffer in DataBuffer (for Phase 1-2 generic protocol)
pub fn wrap_audio_in_data_buffer(
    audio: remotemedia_runtime::grpc_service::generated::AudioBuffer,
) -> remotemedia_runtime::grpc_service::generated::DataBuffer {
    use remotemedia_runtime::grpc_service::generated::{DataBuffer, data_buffer};

    DataBuffer {
        data_type: Some(data_buffer::DataType::Audio(audio)),
        metadata: std::collections::HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_server_startup() {
        let addr = start_test_server().await;
        assert!(!addr.is_empty());
        assert!(addr.contains("::1") || addr.contains("127.0.0.1"));
        
        // Verify server is accessible
        assert!(wait_for_server(&addr, 10).await);
    }
    
    #[test]
    fn test_create_test_audio_buffer() {
        let buffer = create_test_audio_buffer(16000, 1, 440.0);
        assert_eq!(buffer.sample_rate, 16000);
        assert_eq!(buffer.num_samples, 16000);
        assert_eq!(buffer.channels, 1);
        assert_eq!(buffer.samples.len(), 16000 * 4); // f32 = 4 bytes
    }
    
    #[test]
    fn test_create_calculator_manifest() {
        let manifest = create_calculator_manifest("test", "add", 5.0);
        assert_eq!(manifest.version, "v1");
        assert_eq!(manifest.nodes.len(), 1);
        assert_eq!(manifest.nodes[0].node_type, "CalculatorNode");
    }
    
    #[test]
    fn test_create_passthrough_manifest() {
        let manifest = create_passthrough_manifest("test");
        assert_eq!(manifest.version, "v1");
        assert_eq!(manifest.nodes.len(), 1);
        assert_eq!(manifest.nodes[0].node_type, "PassThrough");
    }
}
