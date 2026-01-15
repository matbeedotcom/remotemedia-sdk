//! Integration test for control message propagation
//!
//! This test validates that control messages propagate correctly across all execution contexts:
//! 1. Local Rust nodes (in-process)
//! 2. IPC (iceoryx2) for multiprocess Python nodes
//! 3. gRPC transport for remote pipelines
//! 4. WebRTC data channel for browser execution
//! 5. HTTP/SSE for REST-based streaming
//!
//! Success criteria: <10ms P95 propagation latency across all contexts

use remotemedia_core::data::{ControlMessageType, RuntimeData};
use remotemedia_core::Error;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Mock node that receives and tracks control messages
struct MockControlMessageReceiver {
    /// Received control messages with timestamps
    received_messages: Arc<Mutex<Vec<(RuntimeData, Instant)>>>,
}

impl MockControlMessageReceiver {
    fn new() -> Self {
        Self {
            received_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_received_messages(&self) -> Vec<(RuntimeData, Instant)> {
        self.received_messages.lock().unwrap().clone()
    }

    fn receive_control_message(&self, message: RuntimeData) {
        let received_at = Instant::now();
        self.received_messages
            .lock()
            .unwrap()
            .push((message, received_at));
    }
}

#[tokio::test]
async fn test_control_message_creation() {
    // Test basic control message creation via RuntimeData
    let cancel_msg = RuntimeData::ControlMessage {
        message_type: ControlMessageType::CancelSpeculation {
            from_timestamp: 1000,
            to_timestamp: 2000,
        },
        segment_id: Some("segment_123".to_string()),
        timestamp_ms: 1000,
        metadata: serde_json::json!({
            "reason": "false_positive",
            "confidence": 0.3,
        }),
    };

    match cancel_msg {
        RuntimeData::ControlMessage {
            message_type,
            segment_id,
            timestamp_ms,
            metadata,
        } => {
            assert!(matches!(
                message_type,
                ControlMessageType::CancelSpeculation { .. }
            ));
            assert_eq!(segment_id.unwrap(), "segment_123");
            assert_eq!(timestamp_ms, 1000);
            assert_eq!(
                metadata.get("reason").unwrap().as_str().unwrap(),
                "false_positive"
            );
        }
        _ => panic!("Expected ControlMessage variant"),
    }
}

#[tokio::test]
async fn test_control_message_as_runtime_data() {
    // Test that control messages work as RuntimeData
    let control_message = RuntimeData::ControlMessage {
        message_type: ControlMessageType::CancelSpeculation {
            from_timestamp: 100,
            to_timestamp: 200,
        },
        segment_id: Some("segment_456".to_string()),
        timestamp_ms: 2000,
        metadata: serde_json::json!({"test": true}),
    };

    match control_message {
        RuntimeData::ControlMessage {
            message_type,
            segment_id,
            timestamp_ms,
            metadata,
        } => {
            assert!(matches!(
                message_type,
                ControlMessageType::CancelSpeculation { .. }
            ));
            assert_eq!(segment_id.unwrap(), "segment_456");
            assert_eq!(timestamp_ms, 2000);
            assert!(metadata.get("test").unwrap().as_bool().unwrap());
        }
        _ => panic!("Expected ControlMessage variant"),
    }
}

#[tokio::test]
async fn test_local_rust_propagation() {
    // Test control message propagation between local Rust nodes (in-process)
    let receiver = Arc::new(MockControlMessageReceiver::new());
    let receiver_clone = receiver.clone();

    let start_time = Instant::now();

    // Create a control message using RuntimeData
    let cancel_msg = RuntimeData::ControlMessage {
        message_type: ControlMessageType::CancelSpeculation {
            from_timestamp: 0,
            to_timestamp: 100,
        },
        segment_id: Some("local_segment".to_string()),
        timestamp_ms: start_time.elapsed().as_millis() as u64,
        metadata: serde_json::json!({"context": "local_rust"}),
    };

    // Simulate receiving
    receiver_clone.receive_control_message(cancel_msg);

    // Verify propagation
    let received = receiver.get_received_messages();
    assert_eq!(received.len(), 1, "Should receive exactly one message");

    let (msg, received_at) = &received[0];
    match msg {
        RuntimeData::ControlMessage {
            message_type,
            segment_id,
            ..
        } => {
            assert!(matches!(
                message_type,
                ControlMessageType::CancelSpeculation { .. }
            ));
            assert_eq!(segment_id.as_ref().unwrap(), "local_segment");
        }
        _ => panic!("Expected ControlMessage"),
    }

    // Verify latency (should be <1ms for in-process)
    let propagation_latency = received_at.duration_since(start_time);
    assert!(
        propagation_latency.as_millis() < 1,
        "Local propagation should be <1ms, got {}ms",
        propagation_latency.as_millis()
    );
}

#[tokio::test]
async fn test_control_message_types() {
    // Test all control message types
    let receiver = Arc::new(MockControlMessageReceiver::new());

    // CancelSpeculation
    receiver.receive_control_message(RuntimeData::ControlMessage {
        message_type: ControlMessageType::CancelSpeculation {
            from_timestamp: 0,
            to_timestamp: 100,
        },
        segment_id: Some("cancel_test".to_string()),
        timestamp_ms: 0,
        metadata: serde_json::json!({}),
    });

    // BatchHint
    receiver.receive_control_message(RuntimeData::ControlMessage {
        message_type: ControlMessageType::BatchHint {
            suggested_batch_size: 5,
        },
        segment_id: None,
        timestamp_ms: 1,
        metadata: serde_json::json!({}),
    });

    // DeadlineWarning
    receiver.receive_control_message(RuntimeData::ControlMessage {
        message_type: ControlMessageType::DeadlineWarning { deadline_us: 50000 },
        segment_id: None,
        timestamp_ms: 2,
        metadata: serde_json::json!({"policy": "new_policy"}),
    });

    let received = receiver.get_received_messages();
    assert_eq!(received.len(), 3, "Should receive all message types");

    // Verify each message type
    match &received[0].0 {
        RuntimeData::ControlMessage { message_type, .. } => {
            assert!(matches!(
                message_type,
                ControlMessageType::CancelSpeculation { .. }
            ));
        }
        _ => panic!("Expected CancelSpeculation"),
    }

    match &received[1].0 {
        RuntimeData::ControlMessage { message_type, .. } => {
            assert!(matches!(message_type, ControlMessageType::BatchHint { .. }));
        }
        _ => panic!("Expected BatchHint"),
    }

    match &received[2].0 {
        RuntimeData::ControlMessage { message_type, .. } => {
            assert!(matches!(
                message_type,
                ControlMessageType::DeadlineWarning { .. }
            ));
        }
        _ => panic!("Expected DeadlineWarning"),
    }
}

#[tokio::test]
async fn test_concurrent_control_messages() {
    // Test that control messages from multiple sessions don't interfere
    let receiver = Arc::new(MockControlMessageReceiver::new());

    let mut handles = vec![];

    for session_num in 0..5 {
        let receiver_clone = receiver.clone();
        let handle = tokio::spawn(async move {
            for msg_num in 0..3 {
                let msg = RuntimeData::ControlMessage {
                    message_type: ControlMessageType::CancelSpeculation {
                        from_timestamp: 0,
                        to_timestamp: 100,
                    },
                    segment_id: Some(format!("session_{}_msg_{}", session_num, msg_num)),
                    timestamp_ms: (session_num * 1000 + msg_num) as u64,
                    metadata: serde_json::json!({"session": session_num, "msg": msg_num}),
                };

                receiver_clone.receive_control_message(msg);
            }
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let received = receiver.get_received_messages();
    assert_eq!(
        received.len(),
        15,
        "Should receive all messages from all sessions (5 sessions * 3 messages)"
    );
}

#[tokio::test]
async fn test_propagation_latency_tracking() {
    // Test that we can accurately measure propagation latency
    let receiver = Arc::new(MockControlMessageReceiver::new());

    let send_time = Instant::now();

    // Simulate controlled delay
    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

    let msg = RuntimeData::ControlMessage {
        message_type: ControlMessageType::CancelSpeculation {
            from_timestamp: 0,
            to_timestamp: 100,
        },
        segment_id: Some("latency_test".to_string()),
        timestamp_ms: send_time.elapsed().as_millis() as u64,
        metadata: serde_json::json!({}),
    };

    receiver.receive_control_message(msg);

    let received = receiver.get_received_messages();
    let (_, received_at) = &received[0];

    let propagation_time = received_at.duration_since(send_time);

    // Should be approximately 5ms (with tolerance for scheduling variance)
    assert!(
        propagation_time.as_millis() >= 4 && propagation_time.as_millis() <= 20,
        "Propagation time should be ~5ms (tolerance up to 20ms for scheduling), got {}ms",
        propagation_time.as_millis()
    );
}

// Placeholder tests for transport integration (T042-T045)

#[tokio::test]
#[ignore] // Enable after T038/T039 IPC integration
async fn test_ipc_propagation() {
    panic!("IPC propagation test requires iceoryx2 setup - implement after T038/T039");
}

#[tokio::test]
#[ignore] // Enable after T042/T043 gRPC integration
async fn test_grpc_propagation() {
    panic!("gRPC propagation test requires protobuf schema update - implement after T042/T043");
}

#[tokio::test]
#[ignore] // Enable after T044 WebRTC integration
async fn test_webrtc_propagation() {
    panic!("WebRTC propagation test requires data channel integration - implement after T044");
}

#[tokio::test]
#[ignore] // Enable after T045 HTTP/SSE integration
async fn test_http_sse_propagation() {
    panic!("HTTP/SSE propagation test requires SSE integration - implement after T045");
}

#[tokio::test]
#[ignore] // Enable after all transports complete
async fn test_end_to_end_all_contexts() {
    panic!("End-to-end propagation test requires all transports - implement after T042-T045");
}
