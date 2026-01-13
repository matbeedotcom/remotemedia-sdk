//! RuntimeData to HealthEvent conversion
//!
//! Centralizes the logic for converting pipeline outputs to health events,
//! eliminating duplication between ingest-srt and stream-health-demo.

use remotemedia_health_analyzer::{convert_json_to_health_events, HealthEvent};
use remotemedia_runtime_core::data::RuntimeData;

/// Convert RuntimeData output to a HealthEvent
///
/// This function handles the conversion of various pipeline outputs to
/// the unified `HealthEvent` type used by both the ingest gateway and CLI.
///
/// # Supported Conversions
///
/// - `RuntimeData::Json` with event_type field -> corresponding `HealthEvent`
/// - Other types -> `None` (passthrough, not an event)
///
/// # Example
///
/// ```ignore
/// use remotemedia_pipeline_runner::convert_output_to_health_event;
/// use remotemedia_runtime_core::data::RuntimeData;
///
/// let json = serde_json::json!({
///     "event_type": "silence",
///     "duration_ms": 5000
/// });
/// let data = RuntimeData::Json(json);
///
/// if let Some(event) = convert_output_to_health_event(&data) {
///     println!("Detected: {:?}", event);
/// }
/// ```
pub fn convert_output_to_health_event(data: &RuntimeData) -> Option<HealthEvent> {
    match data {
        RuntimeData::Json(value) => {
            // Use the library's conversion function which handles all event types:
            // health, silence, low_volume, clipping, channel_imbalance, dropouts,
            // drift, freeze, cadence, av_skew, stream_started, stream_ended
            if let Some(events) = convert_json_to_health_events(value) {
                // Return the first event (most common case is single event)
                events.into_iter().next()
            } else {
                None
            }
        }
        // Audio/video passthrough - not events themselves
        RuntimeData::Audio { .. } | RuntimeData::Video { .. } => None,
        // Other types (Text, Bytes, etc.) - not events
        _ => None,
    }
}

/// Convert RuntimeData output to multiple HealthEvents
///
/// Similar to `convert_output_to_health_event`, but returns all events
/// when the JSON contains an array of events.
pub fn convert_output_to_health_events(data: &RuntimeData) -> Vec<HealthEvent> {
    match data {
        RuntimeData::Json(value) => {
            convert_json_to_health_events(value).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_health_event() {
        // Uses "type" field, not "event_type"
        let json = serde_json::json!({
            "type": "health",
            "score": 0.95,
            "alerts": []
        });
        let data = RuntimeData::Json(json);
        let event = convert_output_to_health_event(&data);
        assert!(event.is_some());
        assert_eq!(event.unwrap().event_type(), "health");
    }

    #[test]
    fn test_convert_silence_event() {
        // Silence requires duration_ms and rms_db
        let json = serde_json::json!({
            "type": "silence",
            "duration_ms": 5000.0,
            "rms_db": -60.0
        });
        let data = RuntimeData::Json(json);
        let event = convert_output_to_health_event(&data);
        assert!(event.is_some());
        assert_eq!(event.unwrap().event_type(), "silence");
    }

    #[test]
    fn test_convert_audio_passthrough() {
        let data = RuntimeData::Audio {
            samples: vec![0.0, 0.1, 0.2],
            sample_rate: 16000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
        };
        let event = convert_output_to_health_event(&data);
        assert!(event.is_none());
    }

    #[test]
    fn test_convert_multiple_events() {
        // Array of events with correct format
        let json = serde_json::json!([
            {"type": "silence", "duration_ms": 1000.0, "rms_db": -55.0},
            {"type": "low_volume", "rms_db": -45.0, "peak_db": -30.0}
        ]);
        let data = RuntimeData::Json(json);
        let events = convert_output_to_health_events(&data);
        assert_eq!(events.len(), 2);
    }
}
