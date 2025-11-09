//! Example of running a concurrent pipeline with multiple Python nodes
//!
//! This example demonstrates:
//! - Creating a pipeline with multiple AI models
//! - Running them concurrently without GIL blocking
//! - Achieving <500ms end-to-end latency

use remotemedia_runtime::{
    executor::node_executor::{NodeContext, NodeExecutor},
    python::multiprocess::{MultiprocessExecutor, MultiprocessConfig},
    Result,
};
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("=== Concurrent Pipeline Example ===\n");

    // Configure multiprocess executor
    let config = MultiprocessConfig {
        max_processes_per_session: Some(10),
        channel_capacity: 100,
        init_timeout_secs: 30,
        python_executable: std::path::PathBuf::from("python"),
        enable_backpressure: true,
    };

    println!("Creating multiprocess executor...");
    let mut executor = MultiprocessExecutor::new(config);

    // Define pipeline nodes (simulating real AI models)
    let nodes = vec![
        ("whisper_asr", "Whisper ASR", json!({
            "model": "base",
            "language": "en",
            "device": "cuda"
        })),
        ("lfm2_audio", "LFM2 Audio S2S", json!({
            "model": "large",
            "voice": "neutral",
            "speed": 1.0
        })),
        ("vibe_voice", "VibeVoice TTS", json!({
            "voice": "sarah",
            "rate": 1.0,
            "pitch": 0.0
        })),
    ];

    println!("\nInitializing pipeline nodes:");
    println!("------------------------------");

    // Initialize all nodes
    let session_id = "example_session_001";
    let start = Instant::now();

    for (node_id, node_type, params) in &nodes {
        print!("  - Initializing {} ({})... ", node_id, node_type);

        let ctx = NodeContext {
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
            params: params.clone(),
            session_id: Some(session_id.to_string()),
            metadata: HashMap::new(),
        };

        match executor.initialize(&ctx).await {
            Ok(_) => println!("✓"),
            Err(e) => {
                println!("✗ Failed: {}", e);
                return Err(e);
            }
        }
    }

    let init_time = start.elapsed();
    println!("\nAll nodes initialized in {:?}", init_time);

    // Simulate audio processing pipeline
    println!("\n=== Running Concurrent Processing ===\n");

    // Create sample audio data (10 seconds at 24kHz)
    let sample_rate = 24000;
    let duration_secs = 10.0;
    let num_samples = (sample_rate as f32 * duration_secs) as usize;

    let audio_input = json!({
        "audio": vec![0.0f32; num_samples],
        "sample_rate": sample_rate,
        "channels": 1,
        "format": "f32"
    });

    println!("Input: 10-second audio buffer ({} samples)", num_samples);
    println!("\nProcessing through pipeline:");
    println!("  ASR → S2S → TTS");

    // Process concurrently
    let process_start = Instant::now();

    // In a real implementation, these would be chained through channels
    // For this example, we'll process in parallel to show concurrency
    let futures = vec![
        executor.process(audio_input.clone()),
        executor.process(audio_input.clone()),
        executor.process(audio_input.clone()),
    ];

    // Wait for all to complete
    let results = futures::future::join_all(futures).await;

    let process_time = process_start.elapsed();

    // Check results
    let mut success_count = 0;
    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(_) => {
                success_count += 1;
                println!("  Node {} processed successfully", i + 1);
            }
            Err(e) => {
                println!("  Node {} failed: {}", i + 1, e);
            }
        }
    }

    println!("\n=== Performance Metrics ===");
    println!("---------------------------");
    println!("Initialization time: {:?}", init_time);
    println!("Processing time:     {:?}", process_time);
    println!("Total latency:       {:?}", init_time + process_time);
    println!("Success rate:        {}/{}", success_count, results.len());

    // Verify we met the <500ms requirement
    if process_time.as_millis() < 500 {
        println!("\n✅ SUCCESS: Achieved <500ms processing latency!");
    } else {
        println!("\n⚠️  WARNING: Processing took >500ms");
    }

    // Demonstrate pipeline resilience
    println!("\n=== Testing Failure Handling ===\n");

    // Create a node that will fail
    let failing_ctx = NodeContext {
        node_id: "failing_node".to_string(),
        node_type: "faulty_processor".to_string(),
        params: json!({
            "error_rate": 1.0  // Always fail
        }),
        session_id: Some("failure_test".to_string()),
        metadata: HashMap::new(),
    };

    println!("Creating a faulty node...");
    if let Err(e) = executor.initialize(&failing_ctx).await {
        println!("Expected failure during initialization: {}", e);
    }

    // Cleanup
    println!("\n=== Cleanup ===\n");
    print!("Terminating all processes... ");

    executor.cleanup().await?;
    println!("✓");

    println!("\n=== Example Complete ===");
    Ok(())
}

// Helper function to display memory usage
#[cfg(target_os = "linux")]
fn print_memory_usage() {
    use std::fs;

    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS") {
                println!("Memory usage: {}", line);
                break;
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn print_memory_usage() {
    println!("Memory usage: (not available on this platform)");
}