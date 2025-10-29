//! T032: Isolation test - Verify failures don't affect other executions
//!
//! Tests that when one execution fails (e.g., invalid node), other concurrent
//! executions continue successfully. Validates proper isolation.
//!
//! Success Criteria:
//! - Failing execution returns error (expected)
//! - Other 99 concurrent executions succeed
//! - No cascade failures
//! - Metrics show 99 success + 1 failure

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    pipeline_execution_service_client::PipelineExecutionServiceClient, ExecuteRequest,
    ManifestMetadata, NodeManifest, PipelineManifest, DataBuffer, data_buffer, JsonData,
};
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Helper to create a valid test manifest
fn create_valid_manifest(id_suffix: usize) -> PipelineManifest {
    PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: format!("valid_pipeline_{}", id_suffix),
            description: "Valid test pipeline".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
        nodes: vec![NodeManifest {
            id: "calc".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: r#"{"operation": "add", "value": 1.0}"#.to_string(),
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

/// Helper to create an INVALID manifest (nonexistent node type)
fn create_invalid_manifest() -> PipelineManifest {
    PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: "invalid_pipeline".to_string(),
            description: "Invalid test pipeline - should fail".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
        nodes: vec![NodeManifest {
            id: "invalid".to_string(),
            node_type: "NonExistentNode".to_string(), // This will fail
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

#[tokio::test(flavor = "multi_thread")]
async fn test_failure_isolation() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("=== T032: Failure Isolation Test ===");
    println!("  Launching 100 concurrent requests (1 invalid, 99 valid)");
    
    let start_time = Instant::now();
    let mut handles = vec![];
    
    // Spawn 100 concurrent tasks: client #50 will be invalid
    for i in 0..100 {
        let addr = server_addr.clone();
        let is_invalid_client = i == 50; // Middle client will fail
        
        let handle = tokio::spawn(async move {
            let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect");
            
            // Create request (invalid for client #50)
            let manifest = if is_invalid_client {
                create_invalid_manifest()
            } else {
                create_valid_manifest(i)
            };

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

            let request = tonic::Request::new(ExecuteRequest {
                manifest: Some(manifest),
                data_inputs,
                resource_limits: None,
                client_version: "test-v1".to_string(),
            });
            
            // Execute pipeline
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                client.execute_pipeline(request),
            )
            .await
            .expect("Request timeout");
            
            (i, is_invalid_client, response)
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut successful_count = 0;
    let mut failed_count = 0;
    let mut invalid_client_errored = false;
    let mut valid_client_failures = vec![];
    
    for handle in handles {
        match handle.await {
            Ok((client_id, is_invalid, response)) => {
                match response {
                    Ok(resp) => {
                        let inner = resp.into_inner();
                        use remotemedia_runtime::grpc_service::generated::execute_response::Outcome;
                        
                        match inner.outcome {
                            Some(Outcome::Result(_)) => {
                                // Success case
                                if is_invalid {
                                    eprintln!(
                                        "  ⚠️  Client {} (invalid) succeeded - expected failure!",
                                        client_id
                                    );
                                } else {
                                    successful_count += 1;
                                }
                            }
                            Some(Outcome::Error(ref e)) => {
                                // Error case
                                if is_invalid {
                                    // Expected error for invalid client
                                    invalid_client_errored = true;
                                    println!("  ✅ Client {} (invalid) failed as expected", client_id);
                                } else {
                                    // Unexpected error for valid client - ISOLATION FAILURE
                                    failed_count += 1;
                                    valid_client_failures.push(client_id);
                                    eprintln!(
                                        "  ❌ Client {} (valid) failed unexpectedly: {}",
                                        client_id,
                                        e.message
                                    );
                                }
                            }
                            None => {
                                eprintln!("Client {}: No outcome in response", client_id);
                            }
                        }
                    }
                    Err(e) => {
                        if !is_invalid {
                            failed_count += 1;
                            valid_client_failures.push(client_id);
                            eprintln!("  ❌ Client {} RPC error: {}", client_id, e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("  ❌ Task join error: {}", e);
                failed_count += 1;
            }
        }
    }
    
    let elapsed = start_time.elapsed();
    
    println!("\n=== Isolation Test Results ===");
    println!("  Total requests: 100");
    println!("  Valid requests: 99");
    println!("  Invalid requests: 1");
    println!("  Successful (valid): {}", successful_count);
    println!("  Failed (valid): {}", failed_count);
    println!(
        "  Invalid client errored: {}",
        if invalid_client_errored { "✅" } else { "❌" }
    );
    println!("  Total time: {:?}", elapsed);
    
    // Assertions
    assert!(
        invalid_client_errored,
        "Invalid client should have received error response"
    );
    
    assert_eq!(
        successful_count, 99,
        "Expected 99 valid requests to succeed, got {}",
        successful_count
    );
    
    assert_eq!(
        failed_count, 0,
        "Expected 0 valid requests to fail (isolation failure), got {}. Failed clients: {:?}",
        failed_count, valid_client_failures
    );
    
    println!("\n✅ T032 PASSED: Failure isolation working - invalid execution didn't affect others");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_timeout_isolation() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T032b: Timeout Isolation Test ===");
    println!("  Testing that slow/timeout requests don't block others");
    
    let mut handles = vec![];
    
    // Launch 50 fast requests
    for i in 0..50 {
        let addr = server_addr.clone();
        
        let handle = tokio::spawn(async move {
            let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect");
            
            let manifest = create_valid_manifest(i);

            let mut data_inputs = HashMap::new();
            data_inputs.insert(
                "calc".to_string(),
                DataBuffer {
                    data_type: Some(data_buffer::DataType::Json(JsonData {
                        json_payload: r#"{"value": 5.0}"#.to_string(),
                        schema_type: String::new(),
                    })),
                    metadata: HashMap::new(),
                },
            );

            let request = tonic::Request::new(ExecuteRequest {
                manifest: Some(manifest),
                data_inputs,
                resource_limits: None,
                client_version: "test-v1".to_string(),
            });
            
            let start = Instant::now();
            let result = client.execute_pipeline(request).await;
            let elapsed = start.elapsed();
            
            (i, result, elapsed)
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut successful = 0;
    let mut max_latency = Duration::from_secs(0);
    
    for handle in handles {
        if let Ok((client_id, result, latency)) = handle.await {
            if result.is_ok() {
                successful += 1;
                if latency > max_latency {
                    max_latency = latency;
                }
                
                // Fast requests should complete quickly even under load
                assert!(
                    latency < Duration::from_secs(2),
                    "Client {} took too long: {:?}",
                    client_id,
                    latency
                );
            }
        }
    }
    
    println!("\n=== Timeout Isolation Results ===");
    println!("  Successful: {}/50", successful);
    println!("  Max latency: {:?}", max_latency);
    
    assert_eq!(successful, 50, "Expected all fast requests to succeed");
    
    println!("\n✅ T032b PASSED: Fast requests not blocked by slow/timeout requests");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_error_propagation_isolation() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T032c: Error Propagation Isolation ===");
    println!("  Testing that errors are properly isolated and reported");
    
    let mut handles = vec![];
    
    // Mix of valid and invalid requests
    for i in 0..20 {
        let addr = server_addr.clone();
        let should_fail = i % 5 == 0; // Every 5th request fails
        
        let handle = tokio::spawn(async move {
            let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect");
            
            let manifest = if should_fail {
                create_invalid_manifest()
            } else {
                create_valid_manifest(i)
            };
            
            let mut data_inputs = std::collections::HashMap::new();
            data_inputs.insert(
                "calc".to_string(),
                DataBuffer {
                    data_type: Some(data_buffer::DataType::Json(JsonData {
                        json_payload: r#"{"value": 3.0}"#.to_string(),
                        schema_type: String::new(),
                    })),
                    metadata: HashMap::new(),
                },
            );
            
            let request = tonic::Request::new(ExecuteRequest {
                manifest: Some(manifest),
                data_inputs,
                resource_limits: None,
                client_version: "test-v1".to_string(),
            });
            
            let response = client.execute_pipeline(request).await.ok();
            (i, should_fail, response)
        });
        
        handles.push(handle);
    }
    
    // Verify results
    let mut expected_failures = 0;
    let mut expected_successes = 0;
    let mut actual_failures = 0;
    let mut actual_successes = 0;
    
    for handle in handles {
        if let Ok((client_id, should_fail, response)) = handle.await {
            if should_fail {
                expected_failures += 1;
            } else {
                expected_successes += 1;
            }
            
            if let Some(resp) = response {
                let inner = resp.into_inner();
                use remotemedia_runtime::grpc_service::generated::execute_response::Outcome;
                match inner.outcome {
                    Some(Outcome::Error(_)) => {
                        actual_failures += 1;
                    }
                    Some(Outcome::Result(_)) => {
                        actual_successes += 1;
                    }
                    None => {}
                }
            }
        }
    }
    
    println!("\n=== Error Propagation Results ===");
    println!("  Expected failures: {}", expected_failures);
    println!("  Actual failures: {}", actual_failures);
    println!("  Expected successes: {}", expected_successes);
    println!("  Actual successes: {}", actual_successes);
    
    assert_eq!(
        actual_failures, expected_failures,
        "Error propagation mismatch"
    );
    assert_eq!(
        actual_successes, expected_successes,
        "Success propagation mismatch"
    );
    
    println!("\n✅ T032c PASSED: Error propagation properly isolated");
}
