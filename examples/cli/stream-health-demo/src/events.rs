//! Health event types and JSONL output
//!
//! This module defines the health events emitted by the stream health monitor
//! and provides utilities for JSONL formatting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;

/// Health event types emitted by the stream health monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HealthEvent {
    /// Audio/video drift exceeds threshold
    Drift {
        /// Timestamp when the event was detected
        ts: DateTime<Utc>,
        /// Current lead/drift in milliseconds (positive = ahead, negative = behind)
        lead_ms: i64,
        /// Configured threshold in milliseconds
        threshold_ms: i64,
        /// Stream identifier (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        stream_id: Option<String>,
    },
    
    /// Stream freeze detected
    Freeze {
        /// Timestamp when the freeze was detected
        ts: DateTime<Utc>,
        /// Duration of the freeze in milliseconds
        duration_ms: u64,
        /// Stream identifier (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        stream_id: Option<String>,
    },
    
    /// Periodic health score update
    Health {
        /// Timestamp of the health score
        ts: DateTime<Utc>,
        /// Health score (0.0 to 1.0)
        score: f64,
        /// List of active alert names
        alerts: Vec<String>,
    },
    
    /// Cadence variance alert
    Cadence {
        /// Timestamp when the alert was triggered
        ts: DateTime<Utc>,
        /// Coefficient of variation
        cv: f64,
        /// Configured threshold
        threshold: f64,
    },
    
    /// Audio/video synchronization skew
    AvSkew {
        /// Timestamp when the skew was detected
        ts: DateTime<Utc>,
        /// Current skew in milliseconds (positive = video ahead)
        skew_ms: i64,
        /// Configured threshold in milliseconds
        threshold_ms: i64,
    },
}

impl HealthEvent {
    /// Create a new drift event
    pub fn drift(lead_ms: i64, threshold_ms: i64, stream_id: Option<String>) -> Self {
        Self::Drift {
            ts: Utc::now(),
            lead_ms,
            threshold_ms,
            stream_id,
        }
    }

    /// Create a new freeze event
    pub fn freeze(duration_ms: u64, stream_id: Option<String>) -> Self {
        Self::Freeze {
            ts: Utc::now(),
            duration_ms,
            stream_id,
        }
    }

    /// Create a new health score event
    pub fn health(score: f64, alerts: Vec<String>) -> Self {
        Self::Health {
            ts: Utc::now(),
            score,
            alerts,
        }
    }

    /// Create a new cadence alert event
    pub fn cadence(cv: f64, threshold: f64) -> Self {
        Self::Cadence {
            ts: Utc::now(),
            cv,
            threshold,
        }
    }

    /// Create a new A/V skew event
    pub fn av_skew(skew_ms: i64, threshold_ms: i64) -> Self {
        Self::AvSkew {
            ts: Utc::now(),
            skew_ms,
            threshold_ms,
        }
    }

    /// Get the timestamp of the event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Drift { ts, .. } => *ts,
            Self::Freeze { ts, .. } => *ts,
            Self::Health { ts, .. } => *ts,
            Self::Cadence { ts, .. } => *ts,
            Self::AvSkew { ts, .. } => *ts,
        }
    }

    /// Check if this is a drift event
    pub fn is_drift(&self) -> bool {
        matches!(self, Self::Drift { .. })
    }

    /// Check if this is a freeze event
    pub fn is_freeze(&self) -> bool {
        matches!(self, Self::Freeze { .. })
    }

    /// Check if this is a health event
    pub fn is_health(&self) -> bool {
        matches!(self, Self::Health { .. })
    }
}

/// Event emitter that writes JSONL to output and collects events for summary
pub struct EventEmitter {
    writer: Box<dyn Write + Send>,
    events: Vec<HealthEvent>,
}

impl EventEmitter {
    /// Create a new event emitter writing to the specified output
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer,
            events: Vec::new(),
        }
    }

    /// Create an event emitter writing to stdout
    pub fn stdout() -> Self {
        Self::new(Box::new(std::io::stdout()))
    }

    /// Emit a health event as JSONL
    pub fn emit(&mut self, event: HealthEvent) -> std::io::Result<()> {
        let line = serde_json::to_string(&event)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writeln!(self.writer, "{}", line)?;
        self.writer.flush()?;
        self.events.push(event);
        Ok(())
    }

    /// Get all collected events
    pub fn events(&self) -> &[HealthEvent] {
        &self.events
    }

    /// Take ownership of collected events
    pub fn into_events(self) -> Vec<HealthEvent> {
        self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drift_event_serialization() {
        let event = HealthEvent::drift(52, 50, Some("audio".to_string()));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"drift\""));
        assert!(json.contains("\"lead_ms\":52"));
        assert!(json.contains("\"threshold_ms\":50"));
        assert!(json.contains("\"stream_id\":\"audio\""));
    }

    #[test]
    fn test_freeze_event_serialization() {
        let event = HealthEvent::freeze(823, None);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"freeze\""));
        assert!(json.contains("\"duration_ms\":823"));
        assert!(!json.contains("stream_id")); // Should be skipped when None
    }

    #[test]
    fn test_health_event_serialization() {
        let event = HealthEvent::health(0.72, vec!["DRIFT_SLOPE".to_string(), "FREEZE".to_string()]);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"health\""));
        assert!(json.contains("\"score\":0.72"));
        assert!(json.contains("\"alerts\":[\"DRIFT_SLOPE\",\"FREEZE\"]"));
    }

    #[test]
    fn test_event_emitter_jsonl() {
        use std::io::Cursor;
        use std::sync::{Arc, Mutex};

        // Use a shared buffer wrapped in Arc<Mutex<_>>
        let buffer = Arc::new(Mutex::new(Cursor::new(Vec::new())));
        
        // Create a wrapper that implements Write + Send
        struct SharedBuffer(Arc<Mutex<Cursor<Vec<u8>>>>);
        
        impl std::io::Write for SharedBuffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0.lock().unwrap().flush()
            }
        }
        
        let buffer_clone = buffer.clone();
        let mut emitter = EventEmitter::new(Box::new(SharedBuffer(buffer_clone)));
        
        emitter.emit(HealthEvent::drift(50, 50, None)).unwrap();
        emitter.emit(HealthEvent::health(0.85, vec![])).unwrap();
        
        // Check the collected events
        assert_eq!(emitter.events().len(), 2);
        assert!(emitter.events()[0].is_drift());
        assert!(emitter.events()[1].is_health());
        
        // Also verify the buffer content
        let inner = buffer.lock().unwrap();
        let output = String::from_utf8(inner.get_ref().clone()).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"type\":\"drift\""));
        assert!(lines[1].contains("\"type\":\"health\""));
    }
}
