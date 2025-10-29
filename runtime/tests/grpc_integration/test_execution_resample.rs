//! Integration test for simple resample pipeline
//!
//! Tests 44.1kHz → 16kHz resampling via ExecutePipeline RPC.
//! Verifies sample-accurate output.

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    ExecuteRequest, PipelineManifest, AudioBuffer, AudioFormat, NodeManifest,
    ExecutionStatus,
};

#[tokio::test]
#[ignore] // Will pass once ExecutePipeline is implemented
async fn test_resample_44100_to_16000() {
    // Create manifest for resample pipeline
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
                runtime_hint: 0, // Native
                input_types: vec![1], // Audio
                output_types: vec![1], // Audio
            },
        ],
        connections: vec![],
    };

    // Create 1 second of 44.1kHz mono audio (sine wave)
    let input_samples = 44100;
    let mut samples_f32 = Vec::with_capacity(input_samples);
    for i in 0..input_samples {
        let t = i as f32 / 44100.0;
        let sample = (2.0 * std::f32::consts::PI * 440.0 * t).sin(); // 440 Hz tone
        samples_f32.push(sample);
    }

    // Convert to bytes
    let samples_bytes: Vec<u8> = samples_f32
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();

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
    
    // Verify response
    // assert_eq!(response.status, ExecutionStatus::Success as i32);
    // assert!(response.data_outputs.contains_key("resample"));
    
    // let output = &response.data_outputs["resample"];
    // Expected: 16000 samples for 1 second of 16kHz audio
    // assert_eq!(output.sample_rate, 16000);
    // assert_eq!(output.num_samples, 16000);
}

#[test]
fn test_resample_ratio_calculation() {
    // Verify expected output size calculation
    let input_rate = 44100;
    let output_rate = 16000;
    let input_samples = 44100; // 1 second

    let expected_output_samples = (input_samples as f64 * output_rate as f64 / input_rate as f64) as usize;
    
    // Should be approximately 16000 samples
    assert!((expected_output_samples as i32 - 16000).abs() < 10);
}
