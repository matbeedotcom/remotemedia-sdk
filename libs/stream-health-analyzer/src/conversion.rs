//! JSON to HealthEvent conversion utilities
//!
//! This module provides functions to convert JSON output from analyzer nodes
//! into typed HealthEvent enum variants.

use crate::HealthEvent;
use serde_json::Value;

/// Convert JSON from HealthEmitterNode to HealthEvent enum
///
/// Handles both single events and arrays of events.
/// Returns `None` if the JSON is null or empty.
///
/// # Arguments
/// * `json` - The JSON value from a health analyzer node
///
/// # Returns
/// * `Some(Vec<HealthEvent>)` if events were successfully converted
/// * `None` if the JSON was null/empty or contained no valid events
pub fn convert_json_to_health_events(json: &Value) -> Option<Vec<HealthEvent>> {
    // Handle null (no events)
    if json.is_null() {
        return None;
    }

    // Handle array of events
    if let Some(array) = json.as_array() {
        let events: Vec<_> = array
            .iter()
            .filter_map(convert_single_json_event)
            .collect();
        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    } else {
        // Single event
        convert_single_json_event(json).map(|e| vec![e])
    }
}

/// Convert a single JSON event to HealthEvent
fn convert_single_json_event(json: &Value) -> Option<HealthEvent> {
    // Check for schema-based events from analysis nodes (uses _schema field)
    if let Some(schema) = json.get("_schema").and_then(|v| v.as_str()) {
        return convert_schema_event(schema, json);
    }

    let event_type = json.get("type")?.as_str()?;

    match event_type {
        "drift" => {
            let lead_ms = json.get("lead_ms")?.as_i64()?;
            let threshold_ms = json.get("threshold_ms")?.as_i64()?;
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::drift(lead_ms, threshold_ms, stream_id))
        }
        "freeze" => {
            let duration_ms = json.get("duration_ms")?.as_u64()?;
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::freeze(duration_ms, stream_id))
        }
        "health" => {
            let score = json.get("score")?.as_f64()?;
            let alerts = json
                .get("alerts")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Some(HealthEvent::health(score, alerts))
        }
        "cadence" => {
            let cv = json.get("cv")?.as_f64()?;
            let threshold = json.get("threshold")?.as_f64()?;
            Some(HealthEvent::cadence(cv, threshold))
        }
        "av_skew" => {
            let skew_ms = json.get("skew_ms")?.as_i64()?;
            let threshold_ms = json.get("threshold_ms")?.as_i64()?;
            Some(HealthEvent::av_skew(skew_ms, threshold_ms))
        }
        "silence" => {
            let duration_ms = json.get("duration_ms")?.as_f64()? as f32;
            let rms_db = json.get("rms_db")?.as_f64()? as f32;
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::silence(duration_ms, rms_db, stream_id))
        }
        "low_volume" => {
            let rms_db = json.get("rms_db")?.as_f64()? as f32;
            let peak_db = json.get("peak_db")?.as_f64()? as f32;
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::low_volume(rms_db, peak_db, stream_id))
        }
        "clipping" => {
            let saturation_ratio = json.get("saturation_ratio")?.as_f64()? as f32;
            let crest_factor_db = json.get("crest_factor_db")?.as_f64()? as f32;
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::clipping(
                saturation_ratio,
                crest_factor_db,
                stream_id,
            ))
        }
        "channel_imbalance" => {
            let imbalance_db = json.get("imbalance_db")?.as_f64()? as f32;
            let dead_channel = json.get("dead_channel")?.as_str()?.to_string();
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::channel_imbalance(
                imbalance_db,
                dead_channel,
                stream_id,
            ))
        }
        "dropouts" => {
            let dropout_count = json.get("dropout_count")?.as_u64()? as u32;
            let stream_id = json
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::dropouts(dropout_count, stream_id))
        }
        "stream_started" => {
            let session_id = json
                .get("session_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::stream_started(session_id))
        }
        "stream_ended" => {
            let relative_ms = json.get("relative_ms")?.as_u64()?;
            let reason = json.get("reason")?.as_str()?.to_string();
            let session_id = json
                .get("session_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::stream_ended(relative_ms, reason, session_id))
        }
        _ => None,
    }
}

/// Convert schema-based events from analysis nodes
///
/// These events use the `_schema` field to identify the event type,
/// rather than the `type` field used by HealthEmitterNode.
fn convert_schema_event(schema: &str, data: &Value) -> Option<HealthEvent> {
    match schema {
        "audio_level_event" => {
            let is_low_volume = data.get("is_low_volume")?.as_bool()?;
            let is_silence = data.get("is_silence")?.as_bool()?;
            let rms_db = data.get("rms_db")?.as_f64()? as f32;
            let peak_db = data.get("peak_db")?.as_f64()? as f32;
            let stream_id = data
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));

            if is_silence {
                Some(HealthEvent::silence(0.0, rms_db, stream_id))
            } else if is_low_volume {
                Some(HealthEvent::low_volume(rms_db, peak_db, stream_id))
            } else {
                None // No issue detected
            }
        }
        "silence_event" => {
            let is_sustained = data.get("is_sustained_silence")?.as_bool()?;
            let has_dropouts = data.get("has_intermittent_dropouts")?.as_bool()?;
            let silence_duration_ms = data.get("silence_duration_ms")?.as_f64()? as f32;
            let rms_db = data.get("rms_db")?.as_f64()? as f32;
            let dropout_count = data.get("dropout_count")?.as_u64()? as u32;
            let stream_id = data
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));

            if has_dropouts {
                Some(HealthEvent::dropouts(dropout_count, stream_id))
            } else if is_sustained {
                Some(HealthEvent::silence(silence_duration_ms, rms_db, stream_id))
            } else {
                None
            }
        }
        "clipping_event" => {
            let is_clipping = data.get("is_clipping")?.as_bool()?;
            if !is_clipping {
                return None;
            }
            let saturation_ratio = data.get("saturation_ratio")?.as_f64()? as f32;
            let crest_factor_db = data.get("crest_factor_db")?.as_f64()? as f32;
            let stream_id = data
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::clipping(
                saturation_ratio,
                crest_factor_db,
                stream_id,
            ))
        }
        "channel_balance_event" => {
            let is_imbalanced = data.get("is_imbalanced")?.as_bool()?;
            if !is_imbalanced {
                return None;
            }
            let imbalance_db = data.get("imbalance_db")?.as_f64()? as f32;
            let dead_channel = data.get("dead_channel")?.as_str()?.to_string();
            let stream_id = data
                .get("stream_id")
                .and_then(|v| v.as_str().map(String::from));
            Some(HealthEvent::channel_imbalance(
                imbalance_db,
                dead_channel,
                stream_id,
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_drift_event() {
        let json = json!({
            "type": "drift",
            "lead_ms": 52,
            "threshold_ms": 50,
            "stream_id": "audio"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_drift());
    }

    #[test]
    fn test_convert_health_event() {
        let json = json!({
            "type": "health",
            "score": 0.85,
            "alerts": ["DRIFT", "LOW_VOLUME"]
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_health());
    }

    #[test]
    fn test_convert_array_of_events() {
        let json = json!([
            { "type": "drift", "lead_ms": 50, "threshold_ms": 50 },
            { "type": "freeze", "duration_ms": 823 }
        ]);

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 2);
        assert!(events[0].is_drift());
        assert!(events[1].is_freeze());
    }

    #[test]
    fn test_convert_null_returns_none() {
        let json = json!(null);
        assert!(convert_json_to_health_events(&json).is_none());
    }

    #[test]
    fn test_convert_audio_level_schema_event() {
        let json = json!({
            "_schema": "audio_level_event",
            "is_low_volume": true,
            "is_silence": false,
            "rms_db": -25.0,
            "peak_db": -10.0,
            "stream_id": "audio:0"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            HealthEvent::LowVolume {
                rms_db, peak_db, ..
            } => {
                assert!((*rms_db - -25.0).abs() < 0.01);
                assert!((*peak_db - -10.0).abs() < 0.01);
            }
            _ => panic!("Expected LowVolume event"),
        }
    }

    #[test]
    fn test_convert_silence_schema_event() {
        let json = json!({
            "_schema": "silence_event",
            "is_sustained_silence": true,
            "has_intermittent_dropouts": false,
            "silence_duration_ms": 3500.0,
            "rms_db": -60.0,
            "dropout_count": 0,
            "stream_id": "audio:0"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_silence());
    }

    #[test]
    fn test_convert_clipping_schema_event() {
        let json = json!({
            "_schema": "clipping_event",
            "is_clipping": true,
            "saturation_ratio": 0.12,
            "crest_factor_db": 2.5,
            "stream_id": "audio:0"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_clipping());
    }

    #[test]
    fn test_convert_channel_balance_schema_event() {
        let json = json!({
            "_schema": "channel_balance_event",
            "is_imbalanced": true,
            "imbalance_db": 15.0,
            "dead_channel": "right",
            "stream_id": "audio:0"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_channel_imbalance());
    }

    #[test]
    fn test_no_issue_returns_none() {
        // Audio level event with no issues should return None
        let json = json!({
            "_schema": "audio_level_event",
            "is_low_volume": false,
            "is_silence": false,
            "rms_db": -18.0,
            "peak_db": -6.0
        });

        assert!(convert_json_to_health_events(&json).is_none());
    }

    #[test]
    fn test_stream_started_event() {
        let json = json!({
            "type": "stream_started",
            "session_id": "sess_123"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_system());
    }

    #[test]
    fn test_stream_ended_event() {
        let json = json!({
            "type": "stream_ended",
            "relative_ms": 120000,
            "reason": "client_disconnect",
            "session_id": "sess_123"
        });

        let events = convert_json_to_health_events(&json).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_system());
    }
}
