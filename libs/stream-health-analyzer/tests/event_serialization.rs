//! Event Serialization Tests
//!
//! Tests that verify all HealthEvent variants serialize and deserialize correctly
//! as JSONL format for licensed build validation.

use remotemedia_health_analyzer::{HealthEvent, EventEmitter};
use std::io::Cursor;

/// Test that all HealthEvent variants can be serialized to JSON
#[test]
fn test_all_event_types_serialize() {
    let events = vec![
        HealthEvent::drift(100, 50, None),
        HealthEvent::freeze(500, None),
        HealthEvent::health(0.95, vec!["low_volume".into()]),
        HealthEvent::cadence(0.25, 0.2),
        HealthEvent::av_skew(75, 100),
        HealthEvent::silence(3500.0, -60.0, None),
        HealthEvent::low_volume(-45.0, -40.0, None),
        HealthEvent::clipping(0.05, 3.0, None),
        HealthEvent::channel_imbalance(6.0, "left".into(), None),
        HealthEvent::dropouts(3, None),
        HealthEvent::stream_started(Some("session-123".into())),
        HealthEvent::stream_ended(60000, "completed".into(), Some("session-123".into())),
    ];

    for event in events {
        let json = serde_json::to_string(&event);
        assert!(json.is_ok(), "Failed to serialize {:?}", event);
        
        let json_str = json.unwrap();
        assert!(!json_str.is_empty());
        assert!(json_str.contains("\"type\":"));
    }
}

/// Test round-trip serialization for all event types
#[test]
fn test_round_trip_serialization() {
    let events = vec![
        HealthEvent::drift(100, 50, Some("stream-1".into())),
        HealthEvent::freeze(500, Some("stream-2".into())),
        HealthEvent::health(0.85, vec!["silence".into(), "drift".into()]),
        HealthEvent::silence(2000.0, -55.0, None),
        HealthEvent::clipping(0.1, 2.5, Some("mic-channel".into())),
    ];

    for original in events {
        let json = serde_json::to_string(&original).expect("serialization failed");
        let deserialized: HealthEvent = serde_json::from_str(&json).expect("deserialization failed");
        
        assert_eq!(original, deserialized, "Round-trip failed for {:?}", original);
    }
}

/// Test that the "type" field is correctly set for each event variant
#[test]
fn test_event_type_field() {
    let test_cases = vec![
        (HealthEvent::drift(0, 0, None), "drift"),
        (HealthEvent::freeze(0, None), "freeze"),
        (HealthEvent::health(1.0, vec![]), "health"),
        (HealthEvent::cadence(0.0, 0.0), "cadence"),
        (HealthEvent::av_skew(0, 0), "av_skew"),
        (HealthEvent::silence(0.0, 0.0, None), "silence"),
        (HealthEvent::low_volume(0.0, 0.0, None), "low_volume"),
        (HealthEvent::clipping(0.0, 0.0, None), "clipping"),
        (HealthEvent::channel_imbalance(0.0, "".into(), None), "channel_imbalance"),
        (HealthEvent::dropouts(0, None), "dropouts"),
        (HealthEvent::stream_started(None), "stream_started"),
        (HealthEvent::stream_ended(0, "".into(), None), "stream_ended"),
    ];

    for (event, expected_type) in test_cases {
        let json = serde_json::to_string(&event).unwrap();
        let expected_field = format!("\"type\":\"{}\"", expected_type);
        assert!(
            json.contains(&expected_field),
            "Event {:?} should have type '{}' but got JSON: {}",
            event.event_type(),
            expected_type,
            json
        );
    }
}

/// Test that timestamps are in ISO 8601 UTC format
#[test]
fn test_timestamp_format() {
    let event = HealthEvent::health(0.9, vec![]);
    let json = serde_json::to_string(&event).unwrap();
    
    // Should contain a timestamp like "2024-01-01T00:00:00.000000Z"
    assert!(json.contains("\"ts\":\""), "JSON should contain 'ts' field");
    
    // Parse and verify it contains UTC indicator
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ts = value.get("ts").unwrap().as_str().unwrap();
    assert!(ts.ends_with("Z") || ts.contains("+00:00"), "Timestamp should be UTC: {}", ts);
}

/// Test that optional stream_id is omitted when None
#[test]
fn test_optional_stream_id_omitted() {
    let event_without = HealthEvent::silence(1000.0, -50.0, None);
    let json_without = serde_json::to_string(&event_without).unwrap();
    assert!(!json_without.contains("stream_id"), "stream_id should be omitted when None");

    let event_with = HealthEvent::silence(1000.0, -50.0, Some("test-stream".into()));
    let json_with = serde_json::to_string(&event_with).unwrap();
    assert!(json_with.contains("\"stream_id\":\"test-stream\""), "stream_id should be present when Some");
}

/// Test JSONL format via EventEmitter
#[test]
fn test_event_emitter_jsonl_format() {
    let buffer: Vec<u8> = Vec::new();
    let cursor = Cursor::new(buffer);
    let mut emitter = EventEmitter::new(Box::new(cursor));

    emitter.emit(HealthEvent::health(0.9, vec![])).unwrap();
    emitter.emit(HealthEvent::silence(500.0, -55.0, None)).unwrap();
    emitter.emit(HealthEvent::drift(25, 50, None)).unwrap();

    // Verify collected events
    let events = emitter.events();
    assert_eq!(events.len(), 3, "Should have 3 events");
}

/// Test that health score is within valid range
#[test]
fn test_health_score_serialization() {
    let events = vec![
        HealthEvent::health(0.0, vec![]),
        HealthEvent::health(0.5, vec!["warning".into()]),
        HealthEvent::health(1.0, vec![]),
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        
        let score = value.get("score").unwrap().as_f64().unwrap();
        assert!(score >= 0.0 && score <= 1.0, "Score should be 0-1: {}", score);
    }
}

/// Test alerts array in health events
#[test]
fn test_health_alerts_array() {
    let alerts = vec!["silence".to_string(), "low_volume".to_string(), "drift".to_string()];
    let event = HealthEvent::health(0.7, alerts.clone());
    
    let json = serde_json::to_string(&event).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    
    let parsed_alerts = value.get("alerts").unwrap().as_array().unwrap();
    assert_eq!(parsed_alerts.len(), 3);
    
    for alert in &alerts {
        assert!(json.contains(alert), "Should contain alert: {}", alert);
    }
}

/// Test event_type() method returns correct string
#[test]
fn test_event_type_method() {
    assert_eq!(HealthEvent::drift(0, 0, None).event_type(), "drift");
    assert_eq!(HealthEvent::freeze(0, None).event_type(), "freeze");
    assert_eq!(HealthEvent::health(0.0, vec![]).event_type(), "health");
    assert_eq!(HealthEvent::silence(0.0, 0.0, None).event_type(), "silence");
    assert_eq!(HealthEvent::clipping(0.0, 0.0, None).event_type(), "clipping");
}

/// Test is_* methods for event classification
#[test]
fn test_event_classification_methods() {
    assert!(HealthEvent::drift(0, 0, None).is_drift());
    assert!(!HealthEvent::drift(0, 0, None).is_freeze());
    
    assert!(HealthEvent::freeze(0, None).is_freeze());
    assert!(!HealthEvent::freeze(0, None).is_drift());
    
    assert!(HealthEvent::health(0.0, vec![]).is_health());
    assert!(HealthEvent::silence(0.0, 0.0, None).is_silence());
    assert!(HealthEvent::clipping(0.0, 0.0, None).is_clipping());
}
