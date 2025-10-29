//! T031: Load test - 100 concurrent clients
//!
//! Tests that the service can handle 100 concurrent ExecutePipeline requests
//! without failures. Validates basic concurrency support.
//!
//! Success Criteria:
//! - All 100 requests complete successfully
//! - No connection errors
//! - No timeout errors
//! - All responses valid

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    pipeline_execution_service_client::PipelineExecutionServiceClient, AudioFormat, ExecuteRequest,
    ManifestMetadata, NodeManifest, PipelineManifest, DataBuffer, data_buffer, AudioBuffer, JsonData,
};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::time::timeout;

/// Helper to create a simple test manifest
fn create_test_manifest(id_suffix: usize) -> PipelineManifest {
    PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: format!("concurrent_test_{}", id_suffix),
            description: "Concurrent load test pipeline".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
        nodes: vec![NodeManifest {
            id: "calc".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: r#"{"operation": "add", "value": 5.0}"#.to_string(),
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

/// Helper to create a simple execute request
fn create_execute_request(id_suffix: usize) -> ExecuteRequest {
    let manifest = create_test_manifest(id_suffix);

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "calc".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"value": 10.0}"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "test-v1".to_string(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_100_concurrent_requests() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("=== T031: Load Test - 100 Concurrent Requests ===");
    
    let start_time = Instant::now();
    
    // Spawn 100 concurrent tasks
    let mut handles = vec![];
    
    for i in 0..100 {
        let addr = server_addr.clone();
        
        let handle = tokio::spawn(async move {
            // Connect client
            let mut client = timeout(
                Duration::from_secs(5),
                PipelineExecutionServiceClient::connect(format!("http://{}", addr)),
            )
            .await
            .expect("Connection timeout")
            .expect("Failed to connect");
            
            // Create request
            let request = tonic::Request::new(create_execute_request(i));
            
            // Execute pipeline
            let response = timeout(Duration::from_secs(5), client.execute_pipeline(request))
                .await
                .expect("Request timeout")
                .expect("RPC failed");
            
            let response = response.into_inner();
            
            // Verify response
            use remotemedia_runtime::grpc_service::generated::execute_response::Outcome;
            match response.outcome {
                Some(Outcome::Result(result)) => {
                    assert_eq!(
                        result.status, 1, // EXECUTION_STATUS_SUCCESS
                        "Client {}: Execution failed",
                        i
                    );
                }
                Some(Outcome::Error(e)) => {
                    panic!("Client {}: Execution failed with error: {}", i, e.message);
                }
                None => {
                    panic!("Client {}: No result or error in response", i);
                }
            }
            
            i // Return client ID for tracking
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    let mut successful = 0;
    let mut failed = 0;
    
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(client_id) => {
                successful += 1;
                if (i + 1) % 10 == 0 {
                    println!("  ✅ {} requests completed", i + 1);
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("  ❌ Client {} failed: {}", i, e);
            }
        }
    }
    
    let elapsed = start_time.elapsed();
    
    println!("\n=== Load Test Results ===");
    println!("  Total requests: 100");
    println!("  Successful: {}", successful);
    println!("  Failed: {}", failed);
    println!("  Total time: {:?}", elapsed);
    println!("  Average latency: {:?}", elapsed / 100);
    
    // Assertions
    assert_eq!(
        successful, 100,
        "Expected 100 successful requests, got {}",
        successful
    );
    assert_eq!(failed, 0, "Expected 0 failures, got {}", failed);
    
    // Performance check: 100 requests should complete in reasonable time
    assert!(
        elapsed < Duration::from_secs(30),
        "100 concurrent requests took too long: {:?}",
        elapsed
    );
    
    println!("\n✅ T031 PASSED: All 100 concurrent requests succeeded");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_with_audio_input() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T031b: Concurrent Requests with Audio Input ===");
    
    // Create test audio buffer (1 second at 16kHz, mono, f32)
    let sample_rate = 16000;
    let duration_sec = 1;
    let num_samples = sample_rate * duration_sec;
    let samples: Vec<f32> = (0..num_samples)
        .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / sample_rate as f32).sin() * 0.5)
        .collect();
    
    let audio_bytes: Vec<u8> = samples
        .iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect();
    
    let start_time = Instant::now();
    let mut handles = vec![];
    
    // Spawn 50 concurrent requests with audio (more expensive operation)
    for i in 0..50 {
        let addr = server_addr.clone();
        let audio_data = audio_bytes.clone();
        
        let handle = tokio::spawn(async move {
            let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect");
            
            // Create manifest with PassThrough node
            let manifest = PipelineManifest {
                version: "v1".to_string(),
                metadata: Some(ManifestMetadata {
                    name: format!("audio_test_{}", i),
                    description: "Audio processing test".to_string(),
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
            };
            
            let audio_buffer = AudioBuffer {
                samples: audio_data,
                sample_rate: 16000,
                channels: 1,
                format: AudioFormat::F32 as i32,
                num_samples: num_samples as u64,
            };

            let mut data_inputs = HashMap::new();
            data_inputs.insert(
                "passthrough".to_string(),
                DataBuffer {
                    data_type: Some(data_buffer::DataType::Audio(audio_buffer)),
                    metadata: HashMap::new(),
                },
            );

            let request = tonic::Request::new(ExecuteRequest {
                manifest: Some(manifest),
                data_inputs,
                resource_limits: None,
                client_version: "test-v1".to_string(),
            });
            
            let response = timeout(Duration::from_secs(10), client.execute_pipeline(request))
                .await
                .expect("Request timeout")
                .expect("RPC failed");
            
            let result = response.into_inner();
            use remotemedia_runtime::grpc_service::generated::execute_response::Outcome;
            assert!(matches!(result.outcome, Some(Outcome::Result(_))), "No successful result");
            
            i
        });
        
        handles.push(handle);
    }
    
    // Wait for completion
    let mut successful = 0;
    for handle in handles {
        if handle.await.is_ok() {
            successful += 1;
        }
    }
    
    let elapsed = start_time.elapsed();
    
    println!("\n=== Audio Load Test Results ===");
    println!("  Total requests: 50");
    println!("  Successful: {}", successful);
    println!("  Total time: {:?}", elapsed);
    
    assert_eq!(successful, 50, "Expected 50 successful audio requests");
    
    println!("\n✅ T031b PASSED: All 50 concurrent audio requests succeeded");
}
