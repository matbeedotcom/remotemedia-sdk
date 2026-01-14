//! Event Flow Tests
//!
//! Tests that verify events flow correctly through sinks and channels.

use remotemedia_health_analyzer::{HealthEvent, EventSink, ChannelSink, TerminalSink};

/// Test ChannelSink delivers events to receiver
#[tokio::test]
async fn test_channel_sink_delivery() {
    let (sink, mut rx) = ChannelSink::with_default_capacity();

    let events = vec![
        HealthEvent::health(0.95, vec![]),
        HealthEvent::silence(1500.0, -55.0, None),
        HealthEvent::drift(30, 50, None),
    ];

    for event in &events {
        sink.emit(event.clone()).expect("emit should succeed");
    }

    // Verify events received
    for expected in &events {
        let received = rx.recv().await.expect("should receive event");
        assert_eq!(received, *expected);
    }
}

/// Test events maintain order through channel
#[tokio::test]
async fn test_event_ordering() {
    let (sink, mut rx) = ChannelSink::new(32);

    // Send sequence of health scores
    for i in 0..10 {
        let score = i as f64 / 10.0;
        sink.emit(HealthEvent::health(score, vec![])).unwrap();
    }

    // Verify order is preserved
    for i in 0..10 {
        let expected_score = i as f64 / 10.0;
        let received = rx.recv().await.unwrap();
        
        if let HealthEvent::Health { score, .. } = received {
            assert!((score - expected_score).abs() < 0.001, 
                "Expected score {} but got {}", expected_score, score);
        } else {
            panic!("Expected Health event");
        }
    }
}

/// Test multiple event types in sequence
#[tokio::test]
async fn test_mixed_event_sequence() {
    let (sink, mut rx) = ChannelSink::new(32);

    // Simulate realistic detection sequence
    sink.emit(HealthEvent::stream_started(Some("test-session".into()))).unwrap();
    sink.emit(HealthEvent::health(1.0, vec![])).unwrap();
    sink.emit(HealthEvent::silence(2000.0, -60.0, None)).unwrap();
    sink.emit(HealthEvent::health(0.7, vec!["silence".into()])).unwrap();
    sink.emit(HealthEvent::low_volume(-48.0, -45.0, None)).unwrap();
    sink.emit(HealthEvent::health(0.5, vec!["silence".into(), "low_volume".into()])).unwrap();
    sink.emit(HealthEvent::stream_ended(5000, "completed".into(), Some("test-session".into()))).unwrap();

    // Collect all events
    let mut events = Vec::new();
    for _ in 0..7 {
        events.push(rx.recv().await.unwrap());
    }
    
    assert_eq!(events.len(), 7, "Should receive all 7 events");
    
    // Verify first is stream_started
    assert!(matches!(events[0], HealthEvent::StreamStarted { .. }));
    
    // Verify last is stream_ended
    assert!(matches!(events[6], HealthEvent::StreamEnded { .. }));
}

/// Test TerminalSink basic functionality via EventSink trait
#[test]
fn test_terminal_sink_output() {
    use std::sync::{Arc, Mutex};
    use std::io::{Write, Cursor};
    
    struct SharedBuffer(Arc<Mutex<Cursor<Vec<u8>>>>);
    
    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }
    
    unsafe impl Send for SharedBuffer {}
    unsafe impl Sync for SharedBuffer {}
    
    let buffer = Arc::new(Mutex::new(Cursor::new(Vec::new())));
    let sink = TerminalSink::new(SharedBuffer(buffer.clone()));
    
    sink.emit(HealthEvent::health(0.9, vec![])).unwrap();
    
    let inner = buffer.lock().unwrap();
    let output_str = String::from_utf8(inner.get_ref().clone()).unwrap();
    assert!(!output_str.is_empty(), "TerminalSink should produce output");
    assert!(output_str.contains("\"type\":\"health\""));
}

/// Test channel sink handles high volume events
#[tokio::test]
async fn test_high_volume_events() {
    let (sink, mut rx) = ChannelSink::new(128);

    // Send many events quickly
    for i in 0..100 {
        let event = if i % 3 == 0 {
            HealthEvent::health(0.9, vec![])
        } else if i % 3 == 1 {
            HealthEvent::silence(100.0 * i as f32, -50.0, None)
        } else {
            HealthEvent::drift(i as i64, 100, None)
        };
        sink.emit(event).unwrap();
    }

    // Receive all events
    let mut received_count = 0;
    for _ in 0..100 {
        rx.recv().await.unwrap();
        received_count += 1;
    }
    assert_eq!(received_count, 100, "All events should be delivered");
}

/// Test event timestamp progression
#[test]
fn test_event_timestamps_progress() {
    let event1 = HealthEvent::health(0.9, vec![]);
    std::thread::sleep(std::time::Duration::from_millis(10));
    let event2 = HealthEvent::health(0.8, vec![]);
    
    assert!(
        event2.timestamp() >= event1.timestamp(),
        "Later events should have equal or later timestamps"
    );
}

/// Test cloning events preserves data
#[test]
fn test_event_clone() {
    let original = HealthEvent::silence(5000.0, -55.0, Some("test-stream".into()));
    let cloned = original.clone();
    
    assert_eq!(original, cloned);
    
    // Serialize and compare
    let orig_json = serde_json::to_string(&original).unwrap();
    let clone_json = serde_json::to_string(&cloned).unwrap();
    assert_eq!(orig_json, clone_json);
}

/// Test multiple subscribers receive same event
#[tokio::test]
async fn test_multiple_subscribers() {
    let (sink, mut rx1) = ChannelSink::new(32);
    let mut rx2 = sink.subscribe();
    
    assert_eq!(sink.receiver_count(), 2);
    
    sink.emit(HealthEvent::health(0.75, vec![])).unwrap();
    
    let event1 = rx1.recv().await.unwrap();
    let event2 = rx2.recv().await.unwrap();
    
    assert_eq!(event1, event2);
}
