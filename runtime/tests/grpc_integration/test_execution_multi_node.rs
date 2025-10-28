//! Integration test for multi-node pipeline
//!
//! Tests pipeline with multiple nodes: Resample → VAD → output.
//! Verifies node execution order and data flow.

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    ExecuteRequest, PipelineManifest, AudioBuffer, AudioFormat, NodeManifest,
    Connection, ExecutionStatus,
};

#[tokio::test]
#[ignore] // Will pass once ExecutePipeline is implemented
async fn test_multi_node_pipeline() {
    // Create manifest: input → resample → vad → output
    let manifest = PipelineManifest {
        version: "1.0".to_string(),
        nodes: vec![
            NodeManifest {
                id: "resample".to_string(),
                node_type: "AudioResample".to_string(),
                parameters: r#"{"target_sample_rate": 16000}"#.to_string(),
                runtime_hint: 0,
            },
            NodeManifest {
                id: "vad".to_string(),
                node_type: "VAD".to_string(),
                parameters: r#"{}"#.to_string(),
                runtime_hint: 0,
            },
        ],
        connections: vec![
            Connection {
                from_node: "resample".to_string(),
                from_output: "audio".to_string(),
                to_node: "vad".to_string(),
                to_input: "audio".to_string(),
            },
        ],
        metadata: None,
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

    let request = ExecuteRequest {
        manifest: Some(manifest),
        audio_inputs: vec![("resample".to_string(), audio_input)].into_iter().collect(),
        data_inputs: std::collections::HashMap::new(),
        resource_limits: None,
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
        from_node: "node1".to_string(),
        from_output: "out".to_string(),
        to_node: "node2".to_string(),
        to_input: "in".to_string(),
    };

    assert_eq!(conn.from_node, "node1");
    assert_eq!(conn.to_node, "node2");
}
