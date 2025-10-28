//! Performance test for ExecutePipeline RPC
//!
//! Measures latency for 1s audio resample operation.
//! Target: <5ms p50, <10ms p95 (SC-001)

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    ExecuteRequest, PipelineManifest, AudioBuffer, AudioFormat, NodeManifest,
};
use std::time::Instant;

#[tokio::test]
#[ignore] // Will pass once ExecutePipeline is implemented
async fn test_execution_latency() {
    // Create simple resample pipeline
    let manifest = PipelineManifest {
        version: "1.0".to_string(),
        metadata: None,
        nodes: vec![
            NodeManifest {
                id: "resample".to_string(),
                node_type: "AudioResample".to_string(),
                params: r#"{"target_sample_rate": 16000}"#.to_string(),
                is_streaming: false,
                capabilities: None,
                host: String::new(),
                runtime_hint: 0,
            },
        ],
        connections: vec![],
    };

    // Small audio buffer for latency test (100ms)
    let input_samples = 4410; // 100ms at 44.1kHz
    let samples_bytes = vec![0u8; input_samples * 4];

    let audio_input = AudioBuffer {
        samples: samples_bytes,
        sample_rate: 44100,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: input_samples as u64,
    };

    let request = ExecuteRequest {
        manifest: Some(manifest),
        audio_inputs: vec![("resample".to_string(), audio_input)].into_iter().collect(),
        data_inputs: std::collections::HashMap::new(),
        resource_limits: None,
        client_version: "v1".to_string(),
    };

    // Warmup
    // TODO: service.execute_pipeline(request.clone()).await.unwrap();

    // Measure 100 iterations
    let iterations = 100;
    let mut latencies = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        // TODO: let _response = service.execute_pipeline(request.clone()).await.unwrap();
        let latency = start.elapsed();
        latencies.push(latency.as_micros());
    }

    // Calculate percentiles
    latencies.sort();
    let p50 = latencies[iterations / 2];
    let p95 = latencies[(iterations * 95) / 100];
    let p99 = latencies[(iterations * 99) / 100];

    println!("Latency (µs): p50={}, p95={}, p99={}", p50, p95, p99);

    // Verify against targets (SC-001)
    // assert!(p50 < 5_000, "p50 latency {} µs exceeds 5ms target", p50);
    // assert!(p95 < 10_000, "p95 latency {} µs exceeds 10ms target", p95);
}

#[test]
fn test_latency_calculation() {
    // Verify percentile calculation
    let mut latencies = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    latencies.sort();

    let p50 = latencies[5]; // 50th percentile
    let p95 = latencies[9]; // 95th percentile

    assert_eq!(p50, 6);
    assert_eq!(p95, 10);
}
