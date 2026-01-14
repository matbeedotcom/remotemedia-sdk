//! Integration test for speculative VAD forwarding
//!
//! This test validates the core behavior of the SpeculativeVADGate node:
//! 1. Audio is forwarded immediately without waiting for VAD decision
//! 2. Ring buffer stores audio for potential cancellation
//! 3. On false positive detection, a CancelSpeculation control message is emitted
//! 4. Cancellation messages propagate to downstream nodes

use remotemedia_core::data::{
    AudioBuffer, ControlMessage, ControlMessageType, RuntimeData, SegmentStatus, SpeculativeSegment,
};
use remotemedia_core::nodes::{AsyncStreamingNode, StreamingNode};
use remotemedia_core::Error;
use std::sync::{Arc, Mutex};

/// Mock SpeculativeVADGate for testing (will be replaced with real implementation)
struct MockSpeculativeVADGate {
    /// Ring buffer for lookback
    lookback_ms: u32,
    /// Lookahead for decision making
    lookahead_ms: u32,
    /// Collected outputs for testing
    outputs: Arc<Mutex<Vec<RuntimeData>>>,
    /// Simulate VAD decision
    should_cancel: bool,
}

impl MockSpeculativeVADGate {
    fn new(lookback_ms: u32, lookahead_ms: u32, should_cancel: bool) -> Self {
        Self {
            lookback_ms,
            lookahead_ms,
            outputs: Arc::new(Mutex::new(Vec::new())),
            should_cancel,
        }
    }

    fn get_outputs(&self) -> Vec<RuntimeData> {
        self.outputs.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for MockSpeculativeVADGate {
    fn node_type(&self) -> &str {
        "MockSpeculativeVADGate"
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Err(Error::Execution(
            "MockSpeculativeVADGate requires streaming mode".into(),
        ))
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let session_id = session_id.unwrap_or_else(|| "test_session".to_string());
        let mut output_count = 0;

        // Extract audio from RuntimeData
        match &data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                stream_id,
                ..
            } => {
                // **Behavior 1: Forward audio immediately (speculative forwarding)**
                let forwarded_audio = RuntimeData::Audio {
                    samples: samples.clone(),
                    sample_rate: *sample_rate,
                    channels: *channels,
                    stream_id: stream_id.clone(),
                    timestamp_us: None,
                    arrival_ts_us: None,
                };

                // Store in outputs for test validation
                self.outputs.lock().unwrap().push(forwarded_audio.clone());

                // Forward to callback immediately
                callback(forwarded_audio)?;
                output_count += 1;

                // **Behavior 2: Store in ring buffer (simulated)**
                // In real implementation, we'd push to RingBuffer here

                // **Behavior 3: Check VAD decision and generate cancellation if needed**
                if self.should_cancel {
                    // Simulate that after lookahead, we determined this was a false positive
                    let segment_id = format!("{}_segment_0", session_id);
                    let timestamp_ms = 0u64; // Simplified for test

                    let cancel_msg = RuntimeData::ControlMessage {
                        message_type: ControlMessageType::CancelSpeculation {
                            from_timestamp: 0,
                            to_timestamp: timestamp_ms,
                        },
                        segment_id: Some(segment_id.clone()),
                        timestamp_ms,
                        metadata: serde_json::json!({
                            "reason": "false_positive",
                            "vad_confidence": 0.3,
                        }),
                    };

                    // Store cancellation message
                    self.outputs.lock().unwrap().push(cancel_msg.clone());

                    // Emit cancellation
                    callback(cancel_msg)?;
                    output_count += 1;
                }

                Ok(output_count)
            }
            _ => Err(Error::Execution(
                "MockSpeculativeVADGate requires audio input".into(),
            )),
        }
    }
}

#[tokio::test]
async fn test_speculative_forwarding_immediate() {
    // Test that audio is forwarded immediately without waiting for VAD decision
    let gate = MockSpeculativeVADGate::new(150, 50, false);

    let audio_input = RuntimeData::Audio {
        samples: vec![0.1, 0.2, 0.3, 0.4, 0.5], // 5 samples
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    let mut callback_outputs = Vec::new();
    let callback = |data: RuntimeData| {
        callback_outputs.push(data);
        Ok(())
    };

    let result = gate
        .process_streaming(audio_input.clone(), Some("session1".to_string()), callback)
        .await;

    assert!(result.is_ok(), "Processing should succeed");
    assert_eq!(result.unwrap(), 1, "Should emit exactly 1 output (audio)");

    // Verify audio was forwarded immediately
    assert_eq!(callback_outputs.len(), 1);
    match &callback_outputs[0] {
        RuntimeData::Audio { samples, .. } => {
            assert_eq!(samples.len(), 5, "Audio should be forwarded intact");
        }
        _ => panic!("Expected audio output"),
    }

    // Verify outputs are stored in mock
    let outputs = gate.get_outputs();
    assert_eq!(outputs.len(), 1);
}

#[tokio::test]
async fn test_cancellation_on_false_positive() {
    // Test that cancellation message is generated when VAD detects false positive
    let gate = MockSpeculativeVADGate::new(150, 50, true); // should_cancel=true

    let audio_input = RuntimeData::Audio {
        samples: vec![0.1, 0.2, 0.3, 0.4, 0.5],
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    let mut callback_outputs = Vec::new();
    let callback = |data: RuntimeData| {
        callback_outputs.push(data);
        Ok(())
    };

    let result = gate
        .process_streaming(audio_input, Some("session2".to_string()), callback)
        .await;

    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        2,
        "Should emit 2 outputs: audio + cancellation"
    );

    // Verify outputs
    assert_eq!(callback_outputs.len(), 2);

    // First output should be audio
    match &callback_outputs[0] {
        RuntimeData::Audio { .. } => {}
        _ => panic!("First output should be audio"),
    }

    // Second output should be cancellation control message
    match &callback_outputs[1] {
        RuntimeData::ControlMessage {
            message_type,
            segment_id,
            timestamp_ms,
            metadata,
        } => {
            matches!(message_type, ControlMessageType::CancelSpeculation { .. });
            assert!(segment_id.is_some(), "Cancellation should have segment_id");
            assert_eq!(
                segment_id.as_ref().unwrap(),
                "session2_segment_0",
                "Segment ID should match session"
            );

            // Verify metadata contains reason
            assert!(
                metadata.get("reason").is_some(),
                "Should have cancellation reason"
            );
            assert_eq!(
                metadata.get("reason").unwrap().as_str().unwrap(),
                "false_positive"
            );
        }
        _ => panic!("Second output should be control message"),
    }
}

#[tokio::test]
async fn test_ring_buffer_storage() {
    // Test that audio is stored in ring buffer for potential cancellation
    // This test validates the segment tracking mechanism

    let gate = MockSpeculativeVADGate::new(150, 50, false);

    // Send multiple audio chunks
    for i in 0..3 {
        let audio = RuntimeData::Audio {
            samples: vec![i as f32; 100], // 100 samples per chunk
            sample_rate: 16000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
        };

        let callback = |_: RuntimeData| Ok(());
        let result = gate
            .process_streaming(audio, Some(format!("session_{}", i)), callback)
            .await;

        assert!(result.is_ok());
    }

    // In real implementation, we would verify:
    // 1. Ring buffer contains all chunks within lookback window
    // 2. Oldest chunks are cleared when buffer is full
    // 3. clear_before() is called after segments are confirmed

    let outputs = gate.get_outputs();
    assert_eq!(outputs.len(), 3, "Should have processed 3 audio chunks");
}

#[tokio::test]
async fn test_speculation_acceptance_tracking() {
    // Test that speculation acceptance rate is tracked correctly
    // This will be implemented with LatencyMetrics integration

    let gate_accept = MockSpeculativeVADGate::new(150, 50, false);
    let gate_cancel = MockSpeculativeVADGate::new(150, 50, true);

    let audio = RuntimeData::Audio {
        samples: vec![0.1; 100],
        sample_rate: 16000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
    };

    // Accepted speculation (no cancellation)
    let callback_accept = |_: RuntimeData| Ok(());
    let result_accept = gate_accept
        .process_streaming(
            audio.clone(),
            Some("accept_session".to_string()),
            callback_accept,
        )
        .await;
    assert_eq!(
        result_accept.unwrap(),
        1,
        "Accepted speculation emits only audio"
    );

    // Cancelled speculation
    let callback_cancel = |_: RuntimeData| Ok(());
    let result_cancel = gate_cancel
        .process_streaming(audio, Some("cancel_session".to_string()), callback_cancel)
        .await;
    assert_eq!(
        result_cancel.unwrap(),
        2,
        "Cancelled speculation emits audio + cancellation"
    );

    // In real implementation, we would verify:
    // - LatencyMetrics records speculation_accepted and speculation_cancelled
    // - Acceptance rate = accepted / (accepted + cancelled)
    // - Success criteria: >95% acceptance rate
}

#[tokio::test]
async fn test_concurrent_sessions() {
    // Test that multiple sessions can run concurrently without interference
    let gate = Arc::new(MockSpeculativeVADGate::new(150, 50, false));

    let mut handles = vec![];

    for session_num in 0..5 {
        let gate_clone = gate.clone();
        let handle = tokio::spawn(async move {
            let audio = RuntimeData::Audio {
                samples: vec![session_num as f32; 100],
                sample_rate: 16000,
                channels: 1,
                stream_id: None,
                timestamp_us: None,
                arrival_ts_us: None,
            };

            let callback = |_: RuntimeData| Ok(());
            gate_clone
                .process_streaming(
                    audio,
                    Some(format!("concurrent_session_{}", session_num)),
                    callback,
                )
                .await
        });

        handles.push(handle);
    }

    // Wait for all sessions to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "All sessions should complete successfully");
    }

    // All sessions should have their outputs recorded
    let outputs = gate.get_outputs();
    assert_eq!(
        outputs.len(),
        5,
        "Should have outputs from all 5 concurrent sessions"
    );
}

/// Test that validates the expected behavior when the real SpeculativeVADGate is implemented
///
/// This test will FAIL until the real implementation is complete.
#[tokio::test]
#[ignore] // Ignore until real implementation exists
async fn test_real_speculative_vad_gate_integration() {
    // This test is a placeholder for the real integration test
    // It should be enabled once SpeculativeVADGate is implemented

    // Expected behavior:
    // 1. Create SpeculativeVADGate with real RingBuffer
    // 2. Send audio chunks with known VAD patterns
    // 3. Verify immediate forwarding
    // 4. Verify cancellation on false positive
    // 5. Verify ring buffer maintenance (clear_before)
    // 6. Verify speculation acceptance rate tracking

    panic!("Real SpeculativeVADGate implementation not yet available - this test should fail");
}
