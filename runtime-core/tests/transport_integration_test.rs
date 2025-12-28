//! Integration tests for transport abstraction layer
//!
//! Verifies that PipelineRunner and transport traits work correctly
//! without any transport-specific dependencies.

mod mock_transport;

use mock_transport::MockTransport;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::{
    PipelineRunner, PipelineTransport, StreamSession, TransportData,
};
use std::sync::Arc;

/// Valid test manifest with a PassThrough node
fn test_manifest() -> Manifest {
    serde_json::from_str(r#"{
        "version": "v1",
        "metadata": { "name": "test" },
        "nodes": [{"id": "passthrough", "node_type": "PassThrough", "params": {}}],
        "connections": []
    }"#).unwrap()
}

#[tokio::test]
async fn test_pipeline_runner_creation() {
    let runner = PipelineRunner::new();
    assert!(
        runner.is_ok(),
        "PipelineRunner should initialize successfully"
    );
}

#[tokio::test]
async fn test_unary_execution_via_runner() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(test_manifest());

    let input = TransportData::new(RuntimeData::Text("test".into()));
    let output = runner.execute_unary(manifest, input).await;

    assert!(output.is_ok(), "Unary execution should succeed: {:?}", output.err());
    let result = output.unwrap();
    // PassThrough node echoes input
    assert_eq!(result.data, RuntimeData::Text("test".into()));
}

#[tokio::test]
async fn test_streaming_session_creation() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(test_manifest());

    let session_result = runner.create_stream_session(manifest).await;
    assert!(session_result.is_ok(), "Session creation should succeed: {:?}", session_result.err());

    let session = session_result.unwrap();
    assert!(session.is_active());
    assert!(!session.session_id().is_empty());
}

#[tokio::test]
async fn test_streaming_send_and_receive() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(test_manifest());

    let mut session = runner.create_stream_session(manifest).await.unwrap();

    // Send input
    let input = TransportData::new(RuntimeData::Audio {
        samples: vec![0.0, 0.1, 0.2],
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
    })
    .with_sequence(1);

    session.send_input(input).await.unwrap();

    // Receive output
    let output = session.recv_output().await.unwrap();
    assert!(output.is_some());

    let data = output.unwrap();
    match data.data {
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels,
            ..
        } => {
            assert_eq!(samples.len(), 3);
            assert_eq!(sample_rate, 16000);
            assert_eq!(channels, 1);
        }
        _ => panic!("Expected audio output"),
    }

    // Close session
    session.close().await.unwrap();
    assert!(!session.is_active());
}

#[tokio::test]
async fn test_session_close_idempotency() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(test_manifest());

    let mut session = runner.create_stream_session(manifest).await.unwrap();

    // Close once
    session.close().await.unwrap();
    assert!(!session.is_active());

    // Close again - should be idempotent
    let result = session.close().await;
    assert!(result.is_ok(), "Second close should succeed (idempotent)");
}

#[tokio::test]
async fn test_send_after_close_fails() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(test_manifest());

    let mut session = runner.create_stream_session(manifest).await.unwrap();

    // Close session
    session.close().await.unwrap();

    // Try to send after close
    let input = TransportData::new(RuntimeData::Text("test".into()));
    let result = session.send_input(input).await;

    assert!(result.is_err(), "Send after close should fail");
}

#[tokio::test]
async fn test_mock_transport_trait_implementation() {
    let transport = MockTransport::new().unwrap();
    let manifest = Arc::new(test_manifest());
    let input = TransportData::new(RuntimeData::Text("via trait".into()));

    // Call via trait - PassThrough echoes input
    let output = transport.execute(manifest, input).await.unwrap();
    assert_eq!(output.data, RuntimeData::Text("via trait".into()));
}

#[tokio::test]
async fn test_transport_data_builder_pattern() {
    let data = TransportData::new(RuntimeData::Text("test".into()))
        .with_sequence(42)
        .with_metadata("key1".into(), "value1".into())
        .with_metadata("key2".into(), "value2".into());

    assert_eq!(data.sequence, Some(42));
    assert_eq!(data.get_metadata("key1"), Some(&"value1".to_string()));
    assert_eq!(data.get_metadata("key2"), Some(&"value2".to_string()));
    assert_eq!(data.get_metadata("nonexistent"), None);
}

#[tokio::test]
async fn test_multiple_concurrent_sessions() {
    let runner = PipelineRunner::new().unwrap();
    let manifest = Arc::new(test_manifest());

    // Create multiple sessions concurrently
    let session1 = runner
        .create_stream_session(Arc::clone(&manifest))
        .await
        .unwrap();
    let session2 = runner
        .create_stream_session(Arc::clone(&manifest))
        .await
        .unwrap();
    let session3 = runner.create_stream_session(manifest).await.unwrap();

    // All should have unique IDs
    assert_ne!(session1.session_id(), session2.session_id());
    assert_ne!(session2.session_id(), session3.session_id());
    assert_ne!(session1.session_id(), session3.session_id());

    // All should be active
    assert!(session1.is_active());
    assert!(session2.is_active());
    assert!(session3.is_active());
}
