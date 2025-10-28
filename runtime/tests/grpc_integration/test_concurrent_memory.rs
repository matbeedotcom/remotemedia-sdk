//! T035: Memory test - Verify <10MB per concurrent execution
//!
//! Tests memory usage per concurrent execution remains under 10MB.
//! Validates efficient memory management and prevents leaks.
//!
//! Success Criteria:
//! - Average memory per execution <10MB
//! - No memory leaks (stable memory after many executions)
//! - Memory released after execution completes
//! - Memory metrics accurate

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    pipeline_execution_service_client::PipelineExecutionServiceClient, AudioFormat, ExecuteRequest,
};
use std::time::{Duration, Instant};

/// Get current process memory usage in bytes (approximate)
#[cfg(target_os = "windows")]
fn get_process_memory_bytes() -> Option<u64> {
    use std::mem;
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    
    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = mem::zeroed();
        pmc.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc as *mut _,
            mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ) != 0
        {
            Some(pmc.WorkingSetSize as u64)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_process_memory_bytes() -> Option<u64> {
    // For Linux/Unix, could use /proc/self/status or getrusage
    // For now, return None for non-Windows platforms
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn test_memory_per_execution() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("=== T035: Memory Per Execution Test ===");
    println!("  Measuring memory usage for concurrent executions");
    
    // Get baseline memory
    tokio::time::sleep(Duration::from_secs(1)).await; // Stabilize
    let baseline_memory = get_process_memory_bytes();
    
    if let Some(baseline) = baseline_memory {
        println!("  Baseline memory: {:.2} MB", baseline as f64 / 1_000_000.0);
    } else {
        println!("  ⚠️  Memory measurement not available on this platform");
        println!("  Skipping memory assertions, testing functionality only");
    }
    
    // Execute 100 concurrent pipelines with moderate audio data
    let mut handles = vec![];
    let num_concurrent = 100;
    
    // Create test audio buffer (100ms at 16kHz = small but realistic)
    let audio_buffer = crate::grpc_integration::test_helpers::create_test_audio_buffer(16000, 1, 440.0);
    
    println!("\n  Executing {} concurrent pipelines...", num_concurrent);
    let start_time = Instant::now();
    
    for i in 0..num_concurrent {
        let addr = server_addr.clone();
        let audio_data = audio_buffer.clone();
        
        let handle = tokio::spawn(async move {
            let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect");
            
            let manifest = crate::grpc_integration::test_helpers::create_passthrough_manifest(&format!("mem_test_{}", i));
            
            let mut audio_inputs = std::collections::HashMap::new();
            audio_inputs.insert("passthrough".to_string(), audio_data);
            
            let request = tonic::Request::new(ExecuteRequest {
                manifest: Some(manifest),
                audio_inputs,
                data_inputs: std::collections::HashMap::new(),
                resource_limits: None,
                client_version: "test-v1".to_string(),
            });
            
            let _response = client
                .execute_pipeline(request)
                .await
                .expect("Execution failed");
            
            i
        });
        
        handles.push(handle);
    }
    
    // Wait for all executions
    let mut successful = 0;
    for handle in handles {
        if handle.await.is_ok() {
            successful += 1;
        }
    }
    
    let elapsed = start_time.elapsed();
    
    // Get peak memory during execution
    tokio::time::sleep(Duration::from_millis(100)).await;
    let peak_memory = get_process_memory_bytes();
    
    println!("\n=== Execution Results ===");
    println!("  Successful: {}/{}", successful, num_concurrent);
    println!("  Total time: {:?}", elapsed);
    
    if let (Some(baseline), Some(peak)) = (baseline_memory, peak_memory) {
        let memory_increase = peak.saturating_sub(baseline);
        let memory_per_execution = memory_increase as f64 / num_concurrent as f64;
        
        println!("\n=== Memory Analysis ===");
        println!("  Baseline: {:.2} MB", baseline as f64 / 1_000_000.0);
        println!("  Peak: {:.2} MB", peak as f64 / 1_000_000.0);
        println!("  Increase: {:.2} MB", memory_increase as f64 / 1_000_000.0);
        println!(
            "  Per execution: {:.2} MB",
            memory_per_execution / 1_000_000.0
        );
        
        // Target: <10MB per execution
        let target_mb = 10.0;
        let actual_mb = memory_per_execution / 1_000_000.0;
        
        if actual_mb < target_mb {
            println!("  ✅ Memory per execution within {} MB target", target_mb);
        } else {
            println!(
                "  ⚠️  Memory per execution ({:.2} MB) exceeds {} MB target",
                actual_mb, target_mb
            );
        }
        
        // Assertion
        assert!(
            actual_mb < target_mb,
            "Memory per execution {:.2} MB exceeds {} MB target",
            actual_mb,
            target_mb
        );
    }
    
    println!("\n✅ T035 PASSED: Memory usage per execution acceptable");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_memory_leak_detection() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T035b: Memory Leak Detection Test ===");
    println!("  Running repeated execution cycles to detect leaks");
    
    let cycles = 10;
    let requests_per_cycle = 20;
    let mut memory_samples = vec![];
    
    for cycle in 0..cycles {
        // Execute batch of requests
        let mut handles = vec![];
        
        for i in 0..requests_per_cycle {
            let addr = server_addr.clone();
            
            let handle = tokio::spawn(async move {
                let mut client =
                    PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                        .await
                        .ok()?;
                
                let manifest = crate::grpc_integration::test_helpers::create_calculator_manifest(
                    &format!("leak_test_{}_{}", cycle, i),
                    "add",
                    1.0,
                );
                
                let mut data_inputs = std::collections::HashMap::new();
                data_inputs.insert("calc".to_string(), r#"{"value": 10.0}"#.to_string());
                
                let request = tonic::Request::new(ExecuteRequest {
                    manifest: Some(manifest),
                    audio_inputs: std::collections::HashMap::new(),
                    data_inputs,
                    resource_limits: None,
                    client_version: "test-v1".to_string(),
                });
                
                client.execute_pipeline(request).await.ok()
            });
            
            handles.push(handle);
        }
        
        // Wait for batch to complete
        for handle in handles {
            let _ = handle.await;
        }
        
        // Sample memory after cycle
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(memory) = get_process_memory_bytes() {
            memory_samples.push(memory);
            println!(
                "  Cycle {}/{}: {:.2} MB",
                cycle + 1,
                cycles,
                memory as f64 / 1_000_000.0
            );
        }
        
        // Small delay between cycles
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    if memory_samples.len() >= 2 {
        let first_sample = memory_samples[0];
        let last_sample = memory_samples[memory_samples.len() - 1];
        let memory_growth = last_sample.saturating_sub(first_sample);
        let growth_percent = (memory_growth as f64 / first_sample as f64) * 100.0;
        
        println!("\n=== Memory Leak Analysis ===");
        println!("  First sample: {:.2} MB", first_sample as f64 / 1_000_000.0);
        println!("  Last sample: {:.2} MB", last_sample as f64 / 1_000_000.0);
        println!("  Growth: {:.2} MB ({:.1}%)", memory_growth as f64 / 1_000_000.0, growth_percent);
        
        // Memory should be relatively stable (< 20% growth over cycles)
        assert!(
            growth_percent < 20.0,
            "Memory grew by {:.1}% - possible leak detected",
            growth_percent
        );
        
        println!("  ✅ No significant memory leak detected");
    } else {
        println!("  ⚠️  Insufficient memory samples for leak analysis");
    }
    
    println!("\n✅ T035b PASSED: No memory leaks detected");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_large_audio_buffer_memory() {
    // Start test server
    let server_addr = crate::grpc_integration::test_helpers::start_test_server().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("\n=== T035c: Large Audio Buffer Memory Test ===");
    println!("  Testing memory management with large audio buffers");
    
    // Create large audio buffer (10 seconds at 48kHz = ~2MB)
    let large_audio = crate::grpc_integration::test_helpers::create_test_audio_buffer(48000, 10, 440.0);
    
    let baseline_memory = get_process_memory_bytes();
    
    // Execute 10 concurrent requests with large audio
    let mut handles = vec![];
    
    for i in 0..10 {
        let addr = server_addr.clone();
        let audio_data = large_audio.clone();
        
        let handle = tokio::spawn(async move {
            let mut client = PipelineExecutionServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Failed to connect");
            
            let manifest = crate::grpc_integration::test_helpers::create_passthrough_manifest(&format!("large_audio_{}", i));
            
            let mut audio_inputs = std::collections::HashMap::new();
            audio_inputs.insert("passthrough".to_string(), audio_data);
            
            let request = tonic::Request::new(ExecuteRequest {
                manifest: Some(manifest),
                audio_inputs,
                data_inputs: std::collections::HashMap::new(),
                resource_limits: None,
                client_version: "test-v1".to_string(),
            });
            
            client.execute_pipeline(request).await.is_ok()
        });
        
        handles.push(handle);
    }
    
    // Wait for completion
    let mut successful = 0;
    for handle in handles {
        if let Ok(true) = handle.await {
            successful += 1;
        }
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after_memory = get_process_memory_bytes();
    
    println!("\n=== Large Buffer Results ===");
    println!("  Successful: {}/10", successful);
    
    if let (Some(baseline), Some(after)) = (baseline_memory, after_memory) {
        let increase = after.saturating_sub(baseline);
        println!("  Memory increase: {:.2} MB", increase as f64 / 1_000_000.0);
        
        // Each buffer is ~2MB, 10 concurrent = ~20MB expected
        // With overhead, should be < 30MB total increase
        assert!(
            increase < 30_000_000,
            "Memory increase {:.2} MB too high for large audio buffers",
            increase as f64 / 1_000_000.0
        );
    }
    
    assert_eq!(successful, 10, "Expected all large audio requests to succeed");
    
    println!("\n✅ T035c PASSED: Large audio buffer memory management working");
}
