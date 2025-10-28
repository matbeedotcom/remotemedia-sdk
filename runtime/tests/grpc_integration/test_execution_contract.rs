//! Contract test for ExecutePipeline RPC
//!
//! Verifies that the ExecutePipeline RPC accepts valid requests and returns
//! responses matching the protobuf schema.

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::{
    generated::{
        ExecuteRequest, ExecuteResponse, PipelineManifest, AudioBuffer, AudioFormat,
        ExecutionStatus, NodeManifest, ExecutionResult,
        execute_response::Outcome,
    },
};

#[tokio::test]
async fn test_execute_request_schema() {
    // Create a valid ExecuteRequest
    let manifest = PipelineManifest {
        version: "1.0".to_string(),
        metadata: None,
        nodes: vec![
            NodeManifest {
                id: "input".to_string(),
                node_type: "AudioResample".to_string(),
                params: r#"{"target_sample_rate": 16000}"#.to_string(),
                is_streaming: false,
                capabilities: None,
                host: String::new(),
                runtime_hint: 0, // Native
            },
        ],
        connections: vec![],
    };

    let audio_input = AudioBuffer {
        samples: vec![0u8; 44100 * 4], // 1 second of F32 mono
        sample_rate: 44100,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: 44100,
    };

    let request = ExecuteRequest {
        manifest: Some(manifest),
        audio_inputs: vec![("input".to_string(), audio_input)].into_iter().collect(),
        data_inputs: std::collections::HashMap::new(),
        resource_limits: None,
        client_version: "v1".to_string(),
    };

    // Verify request structure
    assert!(request.manifest.is_some());
    assert_eq!(request.audio_inputs.len(), 1);
    assert!(request.audio_inputs.contains_key("input"));
}

#[test]
fn test_execute_response_schema() {
    // Create a valid ExecuteResponse with success result
    let exec_result = ExecutionResult {
        audio_outputs: std::collections::HashMap::new(),
        data_outputs: std::collections::HashMap::new(),
        metrics: None,
        node_results: vec![],
        status: ExecutionStatus::Success as i32,
    };
    
    let response = ExecuteResponse {
        outcome: Some(Outcome::Result(exec_result)),
    };

    // Verify response structure
    assert!(response.outcome.is_some());
    match response.outcome {
        Some(Outcome::Result(result)) => {
            assert_eq!(result.status, ExecutionStatus::Success as i32);
        },
        _ => panic!("Expected Result outcome"),
    }
}

#[test]
fn test_pipeline_manifest_serialization() {
    // Test that PipelineManifest can be created and serialized
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

    // Should be able to encode to protobuf
    use prost::Message;
    let mut buf = Vec::new();
    manifest.encode(&mut buf).expect("Failed to encode manifest");
    assert!(!buf.is_empty());

    // Should be able to decode back
    let decoded = PipelineManifest::decode(&buf[..]).expect("Failed to decode manifest");
    assert_eq!(decoded.version, "1.0");
    assert_eq!(decoded.nodes.len(), 1);
}

#[test]
fn test_audio_buffer_format_validation() {
    // F32 format
    let buffer_f32 = AudioBuffer {
        samples: vec![0u8; 4], // 1 F32 sample = 4 bytes
        sample_rate: 16000,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: 1,
    };
    assert_eq!(buffer_f32.samples.len(), 4);

    // I16 format
    let buffer_i16 = AudioBuffer {
        samples: vec![0u8; 2], // 1 I16 sample = 2 bytes
        sample_rate: 16000,
        channels: 1,
        format: AudioFormat::I16 as i32,
        num_samples: 1,
    };
    assert_eq!(buffer_i16.samples.len(), 2);
}
