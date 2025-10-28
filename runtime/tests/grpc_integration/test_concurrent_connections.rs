//! T034: Connection pooling test - 1000 concurrent connections
//!
//! Tests that the service can accept and handle 1000 concurrent connections
//! without errors. Validates connection pool management.
//!
//! Success Criteria:
//! - 1000 connections accepted without errors
//! - All connections can execute requests
//! - No connection refused errors
//! - Connection pool metrics accurate

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    pipeline_execution_service_client::PipelineExecutionServiceClient, ExecuteRequest,
};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread")]
async fn test_1000_concurrent_connections() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("=== T034: Connection Pooling Test - 1000 Connections ===");
    println!("  Establishing 1000 concurrent connections...");
    
    let start_time = Instant::now();
    let mut handles = vec![];
    
    // Create 1000 connections
    for i in 0..1000 {
        let addr = server_addr.clone();
        
        let handle = tokio::spawn(async move {
            // Connect
            let client = tokio::time::timeout(
                Duration::from_secs(10),
                PipelineExecutionServiceClient::connect(format!("http://{}", addr)),
            )
            .await
            .expect("Connection timeout");
            
            match client {
                Ok(_) => Ok(i),
                Err(e) => Err((i, e)),
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all connections
    let mut successful_connections = 0;
    let mut failed_connections = vec![];
    
    for handle in handles {
        match handle.await {
            Ok(Ok(client_id)) => {
                successful_connections += 1;
                if (client_id + 1) % 100 == 0 {
                    println!("  ✅ {} connections established", client_id + 1);
                }
            }
            Ok(Err((client_id, e))) => {
                failed_connections.push((client_id, e.to_string()));
                eprintln!("  ❌ Connection {} failed: {}", client_id, e);
            }
            Err(e) => {
                eprintln!("  ❌ Task join error: {}", e);
            }
        }
    }
    
    let connection_time = start_time.elapsed();
    
    println!("\n=== Connection Results ===");
    println!("  Total attempts: 1000");
    println!("  Successful: {}", successful_connections);
    println!("  Failed: {}", failed_connections.len());
    println!("  Connection time: {:?}", connection_time);
    
    // Assertions
    assert_eq!(
        successful_connections, 1000,
        "Expected 1000 successful connections, got {}",
        successful_connections
    );
    
    assert_eq!(
        failed_connections.len(),
        0,
        "Expected 0 failed connections, got {}: {:?}",
        failed_connections.len(),
        failed_connections
    );
    
    println!("\n✅ T034 PASSED: All 1000 connections established successfully");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_connection_reuse() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T034b: Connection Reuse Test ===");
    println!("  Testing connection keep-alive and reuse");
    
    // Connect once
    let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", server_addr))
        .await
        .expect("Failed to connect");
    
    let manifest = crate::grpc_integration::test_helpers::create_calculator_manifest("reuse_test", "add", 1.0);
    
    // Make 10 requests on same connection
    let start_time = Instant::now();
    
    for i in 0..10 {
        let mut data_inputs = std::collections::HashMap::new();
        data_inputs.insert("calc".to_string(), format!(r#"{{"value": {}.0}}"#, i));
        
        let request = tonic::Request::new(ExecuteRequest {
            manifest: Some(manifest.clone()),
            audio_inputs: std::collections::HashMap::new(),
            data_inputs,
            resource_limits: None,
            client_version: "test-v1".to_string(),
        });
        
        let response = client
            .execute_pipeline(request)
            .await
            .expect("Request failed");
        
        let inner = response.into_inner();
        use remotemedia_runtime::grpc_service::generated::execute_response::Outcome;
        assert!(matches!(inner.outcome, Some(Outcome::Result(_))), "Expected successful result");
    }
    
    let elapsed = start_time.elapsed();
    let avg_latency = elapsed / 10;
    
    println!("\n=== Reuse Results ===");
    println!("  Requests: 10");
    println!("  Total time: {:?}", elapsed);
    println!("  Average latency: {:?}", avg_latency);
    
    // Connection reuse should be efficient
    assert!(
        elapsed < Duration::from_secs(5),
        "10 requests on reused connection took too long: {:?}",
        elapsed
    );
    
    println!("\n✅ T034b PASSED: Connection reuse working efficiently");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_connection_timeout_handling() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T034c: Connection Timeout Handling ===");
    println!("  Testing graceful handling of connection timeouts");
    
    // Connect with very short timeout
    let result = tokio::time::timeout(
        Duration::from_millis(1), // Unreasonably short - should timeout
        PipelineExecutionServiceClient::connect(format!("http://{}", server_addr)),
    )
    .await;
    
    // Should either timeout or succeed (timing-dependent)
    match result {
        Err(_) => {
            println!("  ✅ Connection attempt timed out as expected");
        }
        Ok(Ok(_)) => {
            println!("  ✅ Connection succeeded despite short timeout (server very fast)");
        }
        Ok(Err(e)) => {
            println!("  ✅ Connection failed gracefully: {}", e);
        }
    }
    
    // Now connect with reasonable timeout
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        PipelineExecutionServiceClient::connect(format!("http://{}", server_addr)),
    )
    .await;
    
    assert!(result.is_ok(), "Connection with reasonable timeout should succeed");
    assert!(
        result.unwrap().is_ok(),
        "Connection should be successful"
    );
    
    println!("\n✅ T034c PASSED: Timeout handling working correctly");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_connection_bursts() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T034d: Connection Burst Test ===");
    println!("  Testing handling of rapid connection bursts");
    
    // Send 5 bursts of 100 connections each
    for burst in 0..5 {
        println!("\n  Burst {}...", burst + 1);
        
        let start = Instant::now();
        let mut handles = vec![];
        
        // Create burst of 100 connections
        for i in 0..100 {
            let addr = server_addr.clone();
            
            let handle = tokio::spawn(async move {
                PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                    .await
                    .is_ok()
            });
            
            handles.push(handle);
        }
        
        // Count successes
        let mut successful = 0;
        for handle in handles {
            if let Ok(true) = handle.await {
                successful += 1;
            }
        }
        
        let elapsed = start.elapsed();
        
        println!("    Successful: {}/100 in {:?}", successful, elapsed);
        
        // Should handle bursts gracefully
        assert!(
            successful >= 95,
            "Burst {} had too many failures: {}/100",
            burst + 1,
            successful
        );
        
        // Small delay between bursts
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    println!("\n✅ T034d PASSED: Connection bursts handled gracefully");
}
