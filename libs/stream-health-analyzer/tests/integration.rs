//! Integration Tests
//!
//! End-to-end tests that verify the full health event flow
//! from detection through emission and collection.

use remotemedia_health_analyzer::{
    HealthEvent, EventEmitter, EventSink, ChannelSink,
    Watermark, convert_json_to_health_events,
};
use std::io::Cursor;

/// Test full flow: generate events → emit → collect → validate
#[test]
fn test_full_event_pipeline() {
    // Create buffer for JSONL output
    let buffer: Vec<u8> = Vec::new();
    let cursor = Cursor::new(buffer);
    
    let mut emitter = EventEmitter::new(Box::new(cursor));
    
    // Simulate a complete session
    emitter.emit(HealthEvent::stream_started(Some("integration-test".into()))).unwrap();
    emitter.emit(HealthEvent::health(1.0, vec![])).unwrap();
    
    // Simulate some issues detected
    emitter.emit(HealthEvent::silence(3000.0, -58.0, None)).unwrap();
    emitter.emit(HealthEvent::health(0.8, vec!["silence".into()])).unwrap();
    
    emitter.emit(HealthEvent::low_volume(-42.0, -38.0, None)).unwrap();
    emitter.emit(HealthEvent::health(0.6, vec!["silence".into(), "low_volume".into()])).unwrap();
    
    // Recovery
    emitter.emit(HealthEvent::health(0.9, vec![])).unwrap();
    
    emitter.emit(HealthEvent::stream_ended(10000, "completed".into(), Some("integration-test".into()))).unwrap();
    
    // Verify collected events count
    let collected = emitter.events();
    assert_eq!(collected.len(), 8, "Should have 8 events");
    
    // Verify event sequence
    assert!(matches!(collected[0], HealthEvent::StreamStarted { .. }));
    if let HealthEvent::Health { score, .. } = &collected[1] {
        assert_eq!(*score, 1.0);
    }
    assert!(matches!(collected[7], HealthEvent::StreamEnded { .. }));
}

/// Test realistic event sequence with timestamp validation
#[test]
fn test_realistic_detection_sequence() {
    let buffer: Vec<u8> = Vec::new();
    let cursor = Cursor::new(buffer);
    let mut emitter = EventEmitter::new(Box::new(cursor));
    
    // Normal audio
    for _ in 0..3 {
        emitter.emit(HealthEvent::health(1.0, vec![])).unwrap();
    }
    
    // Gradual degradation
    emitter.emit(HealthEvent::silence(500.0, -55.0, None)).unwrap();
    emitter.emit(HealthEvent::health(0.9, vec!["silence".into()])).unwrap();
    
    emitter.emit(HealthEvent::silence(1500.0, -58.0, None)).unwrap();
    emitter.emit(HealthEvent::health(0.7, vec!["silence".into()])).unwrap();
    
    // Audio clipping event
    emitter.emit(HealthEvent::clipping(0.08, 2.5, None)).unwrap();
    emitter.emit(HealthEvent::health(0.5, vec!["silence".into(), "clipping".into()])).unwrap();
    
    let events = emitter.events();
    assert_eq!(events.len(), 9);
    
    // Verify health score degradation
    let health_events: Vec<f64> = events.iter()
        .filter_map(|e| {
            if let HealthEvent::Health { score, .. } = e {
                Some(*score)
            } else {
                None
            }
        })
        .collect();
    
    assert_eq!(health_events, vec![1.0, 1.0, 1.0, 0.9, 0.7, 0.5]);
}

/// Test watermark embedding in events
#[test]
fn test_watermark_embedding() {
    let watermark = Watermark {
        demo: true,
        customer_id: Some("TEST-CUSTOMER".into()),
        license_id: Some("LIC-123".into()),
        watermark: "TEST-CUSTOMER-123".into(),
        expires_at: Some("2027-01-01".into()),
    };
    
    // Watermark should serialize correctly
    let json = serde_json::to_string(&watermark).expect("watermark should serialize");
    assert!(json.contains("TEST-CUSTOMER-123"));
    assert!(json.contains("demo"));
}

/// Test convert_json_to_health_events with various inputs
#[test]
fn test_json_conversion() {
    // Single event
    let single_json: serde_json::Value = serde_json::json!({
        "type": "health",
        "ts": "2024-01-01T00:00:00Z",
        "score": 0.9,
        "alerts": []
    });
    let events = convert_json_to_health_events(&single_json);
    assert!(events.is_some());
    assert_eq!(events.unwrap().len(), 1);
    
    // Array of events
    let array_json: serde_json::Value = serde_json::json!([
        {"type": "health", "ts": "2024-01-01T00:00:00Z", "score": 0.9, "alerts": []},
        {"type": "silence", "ts": "2024-01-01T00:00:01Z", "duration_ms": 1000.0, "rms_db": -55.0}
    ]);
    let events = convert_json_to_health_events(&array_json);
    assert!(events.is_some());
    assert_eq!(events.unwrap().len(), 2);
    
    // Null case
    let null_json: serde_json::Value = serde_json::Value::Null;
    let events = convert_json_to_health_events(&null_json);
    assert!(events.is_none());
}

/// Test event collection with channel sink
#[tokio::test]
async fn test_event_collection_via_channel() {
    let (sink, mut rx) = ChannelSink::new(32);
    
    // Emit events through sink
    sink.emit(HealthEvent::stream_started(None)).unwrap();
    for i in 0..5 {
        sink.emit(HealthEvent::health(1.0 - (i as f64 * 0.1), vec![])).unwrap();
    }
    sink.emit(HealthEvent::stream_ended(5000, "test".into(), None)).unwrap();
    
    // Collect all
    let mut collected = Vec::new();
    for _ in 0..7 {
        collected.push(rx.recv().await.unwrap());
    }
    
    assert_eq!(collected.len(), 7);
    assert!(matches!(collected.first().unwrap(), HealthEvent::StreamStarted { .. }));
    assert!(matches!(collected.last().unwrap(), HealthEvent::StreamEnded { .. }));
}

/// Test that event timestamps are reasonable
#[test]
fn test_reasonable_timestamps() {
    use chrono::Utc;
    
    let now = Utc::now();
    let event = HealthEvent::health(0.9, vec![]);
    let event_ts = event.timestamp();
    
    // Event timestamp should be very close to now (within 1 second)
    let diff = (now - event_ts).num_seconds().abs();
    assert!(diff < 1, "Event timestamp should be within 1 second of now");
}

/// Test edge cases for health scores
#[test]
fn test_health_score_edge_cases() {
    // Zero score
    let event = HealthEvent::health(0.0, vec!["critical".into()]);
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"score\":0"));
    
    // Max score
    let event = HealthEvent::health(1.0, vec![]);
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"score\":1"));
}

/// Test empty alerts array
#[test]
fn test_empty_alerts() {
    let event = HealthEvent::health(1.0, vec![]);
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"alerts\":[]"));
}

/// Test multiple alerts
#[test]
fn test_multiple_alerts() {
    let alerts = vec![
        "silence".to_string(),
        "low_volume".to_string(),
        "drift".to_string(),
        "clipping".to_string(),
    ];
    
    let event = HealthEvent::health(0.3, alerts);
    let json = serde_json::to_string(&event).unwrap();
    
    // All alerts should be present
    assert!(json.contains("silence"));
    assert!(json.contains("low_volume"));
    assert!(json.contains("drift"));
    assert!(json.contains("clipping"));
}
