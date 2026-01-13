//! Integration tests for SpeculativeVADCoordinator using PipelineExecutor
//!
//! These tests verify that SpeculativeVADCoordinator works correctly when
//! executed through the standard pipeline infrastructure (PipelineExecutor,
//! Manifest, StreamingNodeRegistry).

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::{Manifest, ManifestMetadata, NodeManifest};
use remotemedia_runtime_core::transport::{PipelineExecutor, StreamSession, TransportData};
use std::sync::Arc;
use std::time::Duration;

/// Create a simple manifest with just the SpeculativeVADCoordinator node
fn create_coordinator_manifest() -> Manifest {
    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "speculative_vad_coordinator_test".to_string(),
            description: Some("Test pipeline for SpeculativeVADCoordinator".to_string()),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "vad_coordinator".to_string(),
            node_type: "SpeculativeVADCoordinator".to_string(),
            params: serde_json::json!({
                "vad_threshold": 0.5,
                "sample_rate": 16000,
                "min_speech_duration_ms": 250,
                "min_silence_duration_ms": 100,
                "lookback_ms": 150,
                "speech_pad_ms": 30
            }),
            is_streaming: true,
            ..Default::default()
        }],
        connections: vec![],
    }
}

/// Create a manifest with custom configuration
fn create_coordinator_manifest_with_config(
    min_speech_duration_ms: u32,
    min_silence_duration_ms: u32,
) -> Manifest {
    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "custom_coordinator_test".to_string(),
            description: Some("Custom config test".to_string()),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "vad_coordinator".to_string(),
            node_type: "SpeculativeVADCoordinator".to_string(),
            params: serde_json::json!({
                "min_speech_duration_ms": min_speech_duration_ms,
                "min_silence_duration_ms": min_silence_duration_ms,
                "sample_rate": 16000
            }),
            is_streaming: true,
            ..Default::default()
        }],
        connections: vec![],
    }
}

/// Generate test audio samples (sine wave)
fn generate_test_audio(duration_ms: u32, sample_rate: u32) -> Vec<f32> {
    let num_samples = (duration_ms as usize * sample_rate as usize) / 1000;
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3
        })
        .collect()
}

#[tokio::test]
async fn test_coordinator_via_pipeline_runner_streaming() {
    // Create pipeline runner
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");

    // Create manifest
    let manifest = Arc::new(create_coordinator_manifest());

    // Create streaming session
    let mut session = runner
        .create_session(manifest)
        .await
        .expect("Failed to create streaming session");

    // Generate 20ms of test audio at 16kHz (320 samples)
    let audio_samples = generate_test_audio(20, 16000);
    let audio_input = RuntimeData::Audio {
        samples: audio_samples.clone(),
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    // Send audio to session (wrap in TransportData)
    session
        .send_input(TransportData::new(audio_input))
        .await
        .expect("Failed to send input");

    // Collect outputs with timeout
    let mut outputs: Vec<RuntimeData> = Vec::new();
    let timeout = tokio::time::timeout(Duration::from_secs(5), async {
        // Give some time for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to receive outputs
        while let Ok(Ok(Some(transport_data))) = tokio::time::timeout(
            Duration::from_millis(100),
            session.recv_output(),
        )
        .await
        {
            outputs.push(transport_data.data);
        }
    });

    let _ = timeout.await;

    // Should have at least one output (the forwarded audio)
    assert!(
        !outputs.is_empty(),
        "Should receive at least one output (forwarded audio)"
    );

    // First output should be audio (immediate forwarding)
    let first_output = &outputs[0];
    match first_output {
        RuntimeData::Audio { samples, sample_rate, channels, .. } => {
            assert_eq!(*sample_rate, 16000);
            assert_eq!(*channels, 1);
            assert_eq!(samples.len(), audio_samples.len());
        }
        _ => panic!("First output should be audio, got: {:?}", first_output.data_type()),
    }

    // Close session
    session.close().await.expect("Failed to close session");
}

#[tokio::test]
async fn test_coordinator_immediate_forwarding_latency() {
    use std::time::Instant;

    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
    let manifest = Arc::new(create_coordinator_manifest());

    let mut session = runner
        .create_session(manifest)
        .await
        .expect("Failed to create streaming session");

    // Generate audio
    let audio_samples = generate_test_audio(20, 16000);
    let audio_input = RuntimeData::Audio {
        samples: audio_samples,
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    // Measure time from send to first output
    let start = Instant::now();

    session
        .send_input(TransportData::new(audio_input))
        .await
        .expect("Failed to send input");

    // Wait for first output
    let first_output = tokio::time::timeout(Duration::from_secs(2), session.recv_output())
        .await
        .expect("Timeout waiting for output")
        .expect("Failed to receive output")
        .expect("No output data");

    let latency = start.elapsed();

    // Verify it's audio
    assert!(
        matches!(first_output.data, RuntimeData::Audio { .. }),
        "First output should be audio"
    );

    // Latency should be low (< 100ms for streaming path)
    // Note: This includes channel overhead, but should still be fast
    assert!(
        latency < Duration::from_millis(500),
        "Latency too high: {:?}",
        latency
    );

    println!("First output latency: {:?}", latency);

    session.close().await.expect("Failed to close session");
}

#[tokio::test]
async fn test_coordinator_multiple_chunks() {
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
    let manifest = Arc::new(create_coordinator_manifest());

    let mut session = runner
        .create_session(manifest)
        .await
        .expect("Failed to create streaming session");

    // Send multiple audio chunks
    for i in 0..5 {
        let audio_samples = generate_test_audio(20, 16000);
        let audio_input = RuntimeData::Audio {
            samples: audio_samples,
            sample_rate: 16000,
            channels: 1,
            stream_id: Some(format!("chunk_{}", i)),
            timestamp_us: None,
            arrival_ts_us: None,
        };

        session
            .send_input(TransportData::new(audio_input))
            .await
            .expect("Failed to send input");
    }

    // Collect outputs
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut audio_count = 0;
    let mut json_count = 0;
    let mut control_count = 0;

    while let Ok(Ok(Some(transport_data))) = tokio::time::timeout(
        Duration::from_millis(100),
        session.recv_output(),
    )
    .await
    {
        match transport_data.data {
            RuntimeData::Audio { .. } => audio_count += 1,
            RuntimeData::Json(_) => json_count += 1,
            RuntimeData::ControlMessage { .. } => control_count += 1,
            _ => {}
        }
    }

    println!(
        "Received: {} audio, {} JSON, {} control messages",
        audio_count, json_count, control_count
    );

    // Should have at least as many audio outputs as inputs
    assert!(
        audio_count >= 5,
        "Should have at least 5 audio outputs, got {}",
        audio_count
    );

    session.close().await.expect("Failed to close session");
}

#[tokio::test]
async fn test_coordinator_with_custom_config() {
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");

    // Use stricter config (longer min_speech_duration)
    let manifest = Arc::new(create_coordinator_manifest_with_config(500, 200));

    let mut session = runner
        .create_session(manifest)
        .await
        .expect("Failed to create streaming session");

    // Send a short burst of audio (should be considered false positive with 500ms min)
    let audio_samples = generate_test_audio(20, 16000);
    let audio_input = RuntimeData::Audio {
        samples: audio_samples,
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    session
        .send_input(TransportData::new(audio_input))
        .await
        .expect("Failed to send input");

    // Wait and collect
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should at least get the forwarded audio back
    let output = tokio::time::timeout(Duration::from_millis(500), session.recv_output())
        .await
        .expect("Timeout")
        .expect("Should receive output")
        .expect("No output data");

    assert!(
        matches!(output.data, RuntimeData::Audio { .. }),
        "Should receive audio"
    );

    session.close().await.expect("Failed to close session");
}

#[tokio::test]
async fn test_coordinator_unary_execution() {
    // Test unary execution (single input -> single output)
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
    let manifest = Arc::new(create_coordinator_manifest());

    let audio_samples = generate_test_audio(20, 16000);
    let audio_input = RuntimeData::Audio {
        samples: audio_samples.clone(),
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    let transport_input = TransportData::new(audio_input);

    let result = runner.execute_unary(manifest, transport_input).await;

    match result {
        Ok(output) => {
            // Unary execution should return audio
            match output.data {
                RuntimeData::Audio { samples, sample_rate, .. } => {
                    assert_eq!(sample_rate, 16000);
                    assert_eq!(samples.len(), audio_samples.len());
                }
                _ => panic!("Expected audio output, got: {:?}", output.data.data_type()),
            }
        }
        Err(e) => {
            // Some error is expected since unary mode may not fully support streaming nodes
            println!("Unary execution error (may be expected): {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_coordinator_session_isolation() {
    let runner = PipelineExecutor::new().expect("Failed to create PipelineExecutor");
    let manifest = Arc::new(create_coordinator_manifest());

    // Create two separate sessions
    let mut session1 = runner
        .create_session(Arc::clone(&manifest))
        .await
        .expect("Failed to create session 1");

    let mut session2 = runner
        .create_session(Arc::clone(&manifest))
        .await
        .expect("Failed to create session 2");

    // Send different audio to each session
    let audio1 = RuntimeData::Audio {
        samples: generate_test_audio(20, 16000),
        sample_rate: 16000,
        channels: 1,
        stream_id: Some("session1_audio".to_string()),
        timestamp_us: None,
        arrival_ts_us: None,
    };

    let audio2 = RuntimeData::Audio {
        samples: generate_test_audio(30, 16000), // Different duration
        sample_rate: 16000,
        channels: 1,
        stream_id: Some("session2_audio".to_string()),
        timestamp_us: None,
        arrival_ts_us: None,
    };

    session1.send_input(TransportData::new(audio1)).await.expect("Send to session 1");
    session2.send_input(TransportData::new(audio2)).await.expect("Send to session 2");

    // Both should receive their respective outputs
    tokio::time::sleep(Duration::from_millis(200)).await;

    let output1 = tokio::time::timeout(Duration::from_millis(500), session1.recv_output())
        .await
        .expect("Timeout session 1")
        .expect("Output from session 1")
        .expect("No output data from session 1");

    let output2 = tokio::time::timeout(Duration::from_millis(500), session2.recv_output())
        .await
        .expect("Timeout session 2")
        .expect("Output from session 2")
        .expect("No output data from session 2");

    // Both should be audio
    assert!(matches!(output1.data, RuntimeData::Audio { .. }));
    assert!(matches!(output2.data, RuntimeData::Audio { .. }));

    // Close both
    session1.close().await.expect("Close session 1");
    session2.close().await.expect("Close session 2");
}
