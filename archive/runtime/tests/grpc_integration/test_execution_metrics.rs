//! Metrics test for ExecutePipeline RPC
//!
//! Verifies that ExecutionMetrics includes all required fields:
//! - wall_time_ms
//! - memory_used_bytes
//! - node_metrics (per-node statistics)

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    AudioBuffer, AudioFormat, ExecuteRequest, ExecutionStatus, NodeManifest, PipelineManifest,
};

#[tokio::test]
#[ignore] // Will pass once ExecutePipeline is implemented
async fn test_execution_metrics_populated() {
    // Create simple pipeline
    let manifest = PipelineManifest {
        version: "1.0".to_string(),
        metadata: None,
        nodes: vec![NodeManifest {
            id: "resample".to_string(),
            node_type: "AudioResample".to_string(),
            params: r#"{"target_sample_rate": 16000}"#.to_string(),
            is_streaming: false,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,
            input_types: vec![1],  // Audio
            output_types: vec![1], // Audio
        }],
        connections: vec![],
    };

    let input_samples = 44100;
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
        data_inputs: std::collections::HashMap::new(),
        resource_limits: None,
        client_version: "v1".to_string(),
    };

    // TODO: Call ExecutePipeline service once implemented
    // let response = service.execute_pipeline(request).await.unwrap();

    // Verify metrics are present
    // assert_eq!(response.status, ExecutionStatus::Success as i32);
    // assert!(response.metrics.is_some());

    // let metrics = response.metrics.unwrap();
    // assert!(metrics.wall_time_ms > 0.0);
    // assert!(metrics.memory_used_bytes > 0);
    // assert!(metrics.node_metrics.contains_key("resample"));

    // let node_metrics = &metrics.node_metrics["resample"];
    // assert!(node_metrics.execution_time_ms > 0.0);
    // assert_eq!(node_metrics.items_processed, 44100);
}

#[test]
fn test_metrics_structure() {
    use remotemedia_runtime::grpc_service::{ExecutionMetrics, NodeMetrics};

    // Verify metrics can be constructed
    let node_metrics = NodeMetrics {
        execution_time_ms: 1.5,
        memory_bytes: 1_000_000,
        items_processed: 44100,
        custom_metrics: String::new(),
    };

    let metrics = ExecutionMetrics {
        wall_time_ms: 5.0,
        cpu_time_ms: 4.5,
        memory_used_bytes: 10_000_000,
        node_metrics: vec![("test_node".to_string(), node_metrics)]
            .into_iter()
            .collect(),
        serialization_time_ms: 0.5,
        proto_to_runtime_ms: 0.0,
        runtime_to_proto_ms: 0.0,
        data_type_breakdown: std::collections::HashMap::new(),
    };

    assert_eq!(metrics.wall_time_ms, 5.0);
    assert_eq!(metrics.node_metrics.len(), 1);
}
