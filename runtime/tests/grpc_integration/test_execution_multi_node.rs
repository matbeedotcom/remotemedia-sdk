//! Integration test for multi-node pipeline
//!
//! Tests pipeline with multiple nodes: Resample → VAD → output.
//! Verifies node execution order and data flow.

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    data_buffer, AudioBuffer, AudioFormat, Connection, DataBuffer, ExecuteRequest, ExecutionStatus,
    NodeManifest, PipelineManifest,
};
use std::collections::HashMap;

#[tokio::test]
#[ignore] // Will pass once ExecutePipeline is implemented
async fn test_multi_node_pipeline() {
    // Create manifest: input → resample → vad → output
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
                input_types: vec![1],  // Audio
                output_types: vec![1], // Audio
            },
            NodeManifest {
                id: "vad".to_string(),
                node_type: "VAD".to_string(),
                params: r#"{}"#.to_string(),
                is_streaming: false,
                capabilities: None,
                host: String::new(),
                runtime_hint: 0,
                input_types: vec![1],  // Audio
                output_types: vec![1], // Audio
            },
        ],
        connections: vec![Connection {
            from: "resample".to_string(),
            to: "vad".to_string(),
        }],
    };

    // Create input audio
    let input_samples = 44100;
    let samples_bytes = vec![0u8; input_samples * 4]; // Silent audio

    let audio_input = AudioBuffer {
        samples: samples_bytes,
        sample_rate: 44100,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: input_samples as u64,
    };

    let mut data_inputs = HashMap::new();
    data_inputs.insert(
        "resample".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Audio(audio_input)),
            metadata: HashMap::new(),
        },
    );

    let request = ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "v1".to_string(),
    };

    // TODO: Call ExecutePipeline service once implemented
    // let response = service.execute_pipeline(request).await.unwrap();

    // Verify response
    // assert_eq!(response.status, ExecutionStatus::Success as i32);
    // assert!(response.result.is_some());

    // Verify node execution order in metrics
    // let metrics = response.metrics.unwrap();
    // assert!(metrics.node_metrics.contains_key("resample"));
    // assert!(metrics.node_metrics.contains_key("vad"));
}

#[test]
fn test_connection_validation() {
    // Verify connection structure
    let conn = Connection {
        from: "node1".to_string(),
        to: "node2".to_string(),
    };

    assert_eq!(conn.from, "node1");
    assert_eq!(conn.to, "node2");
}
