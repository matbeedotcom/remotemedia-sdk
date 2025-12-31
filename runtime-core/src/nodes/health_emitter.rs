//! HealthEmitterNode - Streaming node that emits health events as JSONL
//!
//! This node wraps DriftMetrics and emits health events (drift, freeze, health score)
//! as JSON output. It's designed for the stream-health-demo binary but can be used
//! in any pipeline requiring health monitoring output.
//!
//! # Features
//!
//! - Records audio/video timing samples using DriftMetrics
//! - Emits JSONL events for drift alerts, freeze detection, and health scores
//! - Configurable thresholds for all metrics
//! - Periodic health score emission
//!
//! # Example
//!
//! ```yaml
//! nodes:
//!   - id: health
//!     node_type: HealthEmitterNode
//!     params:
//!       lead_threshold_ms: 50
//!       freeze_threshold_ms: 500
//!       health_emit_interval_ms: 1000
//! ```

use crate::data::RuntimeData;
use crate::executor::drift_metrics::{DriftAlerts, DriftMetrics, DriftThresholds};
use crate::nodes::streaming_node::{StreamingNode, StreamingNodeFactory};
use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for HealthEmitterNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEmitterConfig {
    /// Lead/drift threshold in milliseconds (default: 50ms)
    #[serde(default = "default_lead_threshold_ms")]
    pub lead_threshold_ms: i64,

    /// Freeze detection threshold in milliseconds (default: 500ms)
    #[serde(default = "default_freeze_threshold_ms")]
    pub freeze_threshold_ms: u64,

    /// Health score emission interval in milliseconds (default: 1000ms)
    #[serde(default = "default_health_emit_interval_ms")]
    pub health_emit_interval_ms: u64,

    /// A/V skew threshold in milliseconds (default: 80ms)
    #[serde(default = "default_av_skew_threshold_ms")]
    pub av_skew_threshold_ms: i64,

    /// Cadence coefficient of variation threshold (default: 0.3)
    #[serde(default = "default_cadence_cv_threshold")]
    pub cadence_cv_threshold: f64,

    /// Health score threshold for alerts (default: 0.7)
    #[serde(default = "default_health_threshold")]
    pub health_threshold: f64,
}

fn default_lead_threshold_ms() -> i64 {
    50
}
fn default_freeze_threshold_ms() -> u64 {
    500
}
fn default_health_emit_interval_ms() -> u64 {
    1000
}
fn default_av_skew_threshold_ms() -> i64 {
    80
}
fn default_cadence_cv_threshold() -> f64 {
    0.3
}
fn default_health_threshold() -> f64 {
    0.7
}

impl Default for HealthEmitterConfig {
    fn default() -> Self {
        Self {
            lead_threshold_ms: default_lead_threshold_ms(),
            freeze_threshold_ms: default_freeze_threshold_ms(),
            health_emit_interval_ms: default_health_emit_interval_ms(),
            av_skew_threshold_ms: default_av_skew_threshold_ms(),
            cadence_cv_threshold: default_cadence_cv_threshold(),
            health_threshold: default_health_threshold(),
        }
    }
}

impl From<&HealthEmitterConfig> for DriftThresholds {
    fn from(config: &HealthEmitterConfig) -> Self {
        Self {
            slope_threshold_ms_per_s: 5.0, // Fixed slope threshold
            lead_jump_threshold_us: (config.lead_threshold_ms * 1000) as u64,
            av_skew_threshold_us: (config.av_skew_threshold_ms * 1000) as u64,
            freeze_threshold_us: config.freeze_threshold_ms * 1000,
            cadence_cv_threshold: config.cadence_cv_threshold,
            health_threshold: config.health_threshold,
            samples_to_raise: 5,
            samples_to_clear: 10,
            slope_ema_alpha: 0.1,
        }
    }
}

/// Internal state for the health emitter node
struct HealthEmitterState {
    /// DriftMetrics instance for tracking stream health
    drift_metrics: DriftMetrics,
    /// Last time we emitted a health score event
    last_health_emit_us: u64,
    /// Health emit interval in microseconds
    health_emit_interval_us: u64,
    /// Configured thresholds for event emission
    config: HealthEmitterConfig,
    /// Sample counter
    sample_count: u64,
}

/// Streaming node that emits health events based on DriftMetrics
pub struct HealthEmitterNode {
    node_id: String,
    state: RwLock<HealthEmitterState>,
}

impl HealthEmitterNode {
    /// Create a new HealthEmitterNode
    pub fn new(node_id: String, config: HealthEmitterConfig) -> Self {
        let thresholds = DriftThresholds::from(&config);
        let health_emit_interval_us = config.health_emit_interval_ms * 1000;

        Self {
            node_id: node_id.clone(),
            state: RwLock::new(HealthEmitterState {
                drift_metrics: DriftMetrics::new(node_id, thresholds),
                last_health_emit_us: 0,
                health_emit_interval_us,
                config,
                sample_count: 0,
            }),
        }
    }

    /// Get current timestamp in microseconds since epoch
    fn current_timestamp_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// Format timestamp as ISO 8601 string
    fn format_timestamp_iso(us: u64) -> String {
        let secs = us / 1_000_000;
        let _micros = us % 1_000_000;
        // Simple ISO format without chrono dependency
        format!("{}000000", secs)
    }

    /// Create a drift event JSON
    fn create_drift_event(lead_us: i64, threshold_us: u64, stream_id: Option<&str>) -> Value {
        let mut event = serde_json::json!({
            "type": "drift",
            "ts": Self::format_timestamp_iso(Self::current_timestamp_us()),
            "lead_ms": lead_us / 1000,
            "threshold_ms": threshold_us as i64 / 1000,
        });

        if let Some(id) = stream_id {
            event["stream_id"] = Value::String(id.to_string());
        }

        event
    }

    /// Create a freeze event JSON
    fn create_freeze_event(duration_ms: u64, stream_id: Option<&str>) -> Value {
        let mut event = serde_json::json!({
            "type": "freeze",
            "ts": Self::format_timestamp_iso(Self::current_timestamp_us()),
            "duration_ms": duration_ms,
        });

        if let Some(id) = stream_id {
            event["stream_id"] = Value::String(id.to_string());
        }

        event
    }

    /// Create a health score event JSON
    fn create_health_event(score: f64, alerts: DriftAlerts) -> Value {
        let alert_names: Vec<&str> = {
            let mut names = Vec::new();
            if alerts.contains(DriftAlerts::DRIFT_SLOPE) {
                names.push("DRIFT_SLOPE");
            }
            if alerts.contains(DriftAlerts::LEAD_JUMP) {
                names.push("LEAD_JUMP");
            }
            if alerts.contains(DriftAlerts::AV_SKEW) {
                names.push("AV_SKEW");
            }
            if alerts.contains(DriftAlerts::FREEZE) {
                names.push("FREEZE");
            }
            if alerts.contains(DriftAlerts::CADENCE_UNSTABLE) {
                names.push("CADENCE_UNSTABLE");
            }
            if alerts.contains(DriftAlerts::HEALTH_LOW) {
                names.push("HEALTH_LOW");
            }
            names
        };

        serde_json::json!({
            "type": "health",
            "ts": Self::format_timestamp_iso(Self::current_timestamp_us()),
            "score": score,
            "alerts": alert_names,
        })
    }

    /// Create a cadence event JSON
    fn create_cadence_event(cv: f64, threshold: f64) -> Value {
        serde_json::json!({
            "type": "cadence",
            "ts": Self::format_timestamp_iso(Self::current_timestamp_us()),
            "cv": cv,
            "threshold": threshold,
        })
    }

    /// Create an A/V skew event JSON
    fn create_av_skew_event(skew_ms: i64, threshold_ms: i64) -> Value {
        serde_json::json!({
            "type": "av_skew",
            "ts": Self::format_timestamp_iso(Self::current_timestamp_us()),
            "skew_ms": skew_ms,
            "threshold_ms": threshold_ms,
        })
    }
}

#[async_trait::async_trait]
impl StreamingNode for HealthEmitterNode {
    fn node_type(&self) -> &str {
        "HealthEmitterNode"
    }

    fn node_id(&self) -> &str {
        &self.node_id
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // For single-output mode, just record the sample and return health JSON
        let events = self.process_and_collect_events(&data)?;

        // Return the health events as JSON
        if events.is_empty() {
            // Return empty JSON if no events
            Ok(RuntimeData::Json(serde_json::json!(null)))
        } else if events.len() == 1 {
            Ok(RuntimeData::Json(events.into_iter().next().unwrap()))
        } else {
            Ok(RuntimeData::Json(Value::Array(events)))
        }
    }

    async fn process_multi_async(
        &self,
        inputs: HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        if let Some((_, data)) = inputs.into_iter().next() {
            self.process_async(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }

    async fn process_streaming_async(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        callback: Box<dyn FnMut(RuntimeData) -> Result<(), Error> + Send>,
    ) -> Result<usize, Error> {
        let mut callback = callback;
        let events = self.process_and_collect_events(&data)?;
        let count = events.len();

        // Emit each event as a separate JSON output
        for event in events {
            callback(RuntimeData::Json(event))?;
        }

        Ok(count)
    }
}

impl HealthEmitterNode {
    /// Process input data and collect health events
    fn process_and_collect_events(&self, data: &RuntimeData) -> Result<Vec<Value>, Error> {
        let mut events = Vec::new();
        let current_time_us = Self::current_timestamp_us();

        // Lock state for updating
        let mut state = self.state.write().map_err(|e| {
            Error::Execution(format!("Failed to lock health emitter state: {}", e))
        })?;

        // Extract timing from input data
        let (media_ts_us, arrival_ts_us, stream_id) = match data {
            RuntimeData::Audio {
                timestamp_us,
                arrival_ts_us,
                stream_id,
                samples,
                sample_rate,
                ..
            } => {
                // Use provided timestamps or estimate from sample count
                let media_ts = timestamp_us.unwrap_or_else(|| {
                    // Estimate from sample count: samples / sample_rate * 1_000_000
                    if *sample_rate > 0 {
                        (state.sample_count * samples.len() as u64 * 1_000_000) / *sample_rate as u64
                    } else {
                        current_time_us
                    }
                });
                let arrival_ts = arrival_ts_us.unwrap_or(current_time_us);
                (media_ts, arrival_ts, stream_id.clone())
            }
            RuntimeData::Video {
                timestamp_us,
                arrival_ts_us,
                stream_id,
                ..
            } => {
                // Video timestamp_us is u64 (not Option), arrival_ts_us is Option<u64>
                let arrival_ts = (*arrival_ts_us).unwrap_or(current_time_us);
                (*timestamp_us, arrival_ts, stream_id.clone())
            }
            _ => {
                // For non-media data, just return empty events
                return Ok(events);
            }
        };

        state.sample_count += 1;

        // Get alerts before recording sample
        let alerts_before = state.drift_metrics.alerts();

        // Record the sample
        let alerts_changed = state
            .drift_metrics
            .record_sample(media_ts_us, arrival_ts_us, None);

        // Get alerts after recording
        let alerts_after = state.drift_metrics.alerts();

        // Check for newly raised alerts and emit events
        if alerts_changed {
            let new_alerts = alerts_after - alerts_before;

            // Drift alert
            if new_alerts.contains(DriftAlerts::DRIFT_SLOPE)
                || new_alerts.contains(DriftAlerts::LEAD_JUMP)
            {
                if let Some(lead_us) = state.drift_metrics.current_lead_us() {
                    events.push(Self::create_drift_event(
                        lead_us,
                        state.config.lead_threshold_ms as u64 * 1000,
                        stream_id.as_deref(),
                    ));
                }
            }

            // Freeze alert
            if new_alerts.contains(DriftAlerts::FREEZE) {
                events.push(Self::create_freeze_event(
                    state.config.freeze_threshold_ms,
                    stream_id.as_deref(),
                ));
            }

            // A/V skew alert
            if new_alerts.contains(DriftAlerts::AV_SKEW) {
                events.push(Self::create_av_skew_event(
                    state.drift_metrics.current_av_skew_us / 1000,
                    state.config.av_skew_threshold_ms,
                ));
            }

            // Cadence alert
            if new_alerts.contains(DriftAlerts::CADENCE_UNSTABLE) {
                events.push(Self::create_cadence_event(
                    state.drift_metrics.cadence_cv(),
                    state.config.cadence_cv_threshold,
                ));
            }
        }

        // Check if we should emit a periodic health score event
        if current_time_us - state.last_health_emit_us >= state.health_emit_interval_us {
            let health_score = state.drift_metrics.health_score();
            events.push(Self::create_health_event(health_score, alerts_after));
            state.last_health_emit_us = current_time_us;
        }

        Ok(events)
    }
}

/// Factory for creating HealthEmitterNode instances
pub struct HealthEmitterNodeFactory;

impl StreamingNodeFactory for HealthEmitterNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: HealthEmitterConfig = serde_json::from_value(params.clone()).unwrap_or_default();

        Ok(Box::new(HealthEmitterNode::new(node_id, config)))
    }

    fn node_type(&self) -> &str {
        "HealthEmitterNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // This node can emit multiple events per input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = HealthEmitterConfig::default();
        assert_eq!(config.lead_threshold_ms, 50);
        assert_eq!(config.freeze_threshold_ms, 500);
        assert_eq!(config.health_emit_interval_ms, 1000);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "lead_threshold_ms": 100,
            "freeze_threshold_ms": 1000,
        });

        let config: HealthEmitterConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.lead_threshold_ms, 100);
        assert_eq!(config.freeze_threshold_ms, 1000);
        // Defaults for unspecified fields
        assert_eq!(config.health_emit_interval_ms, 1000);
    }

    #[test]
    fn test_drift_event_creation() {
        let event = HealthEmitterNode::create_drift_event(50_000, 50_000, Some("audio"));
        assert_eq!(event["type"], "drift");
        assert_eq!(event["lead_ms"], 50);
        assert_eq!(event["threshold_ms"], 50);
        assert_eq!(event["stream_id"], "audio");
    }

    #[test]
    fn test_freeze_event_creation() {
        let event = HealthEmitterNode::create_freeze_event(823, None);
        assert_eq!(event["type"], "freeze");
        assert_eq!(event["duration_ms"], 823);
        assert!(event.get("stream_id").is_none());
    }

    #[test]
    fn test_health_event_creation() {
        let event =
            HealthEmitterNode::create_health_event(0.72, DriftAlerts::DRIFT_SLOPE | DriftAlerts::FREEZE);
        assert_eq!(event["type"], "health");
        assert_eq!(event["score"], 0.72);

        let alerts = event["alerts"].as_array().unwrap();
        assert!(alerts.contains(&Value::String("DRIFT_SLOPE".to_string())));
        assert!(alerts.contains(&Value::String("FREEZE".to_string())));
    }

    #[tokio::test]
    async fn test_node_creation() {
        let node = HealthEmitterNode::new("test_health".to_string(), HealthEmitterConfig::default());
        assert_eq!(node.node_type(), "HealthEmitterNode");
        assert_eq!(node.node_id(), "test_health");
    }

    #[tokio::test]
    async fn test_process_audio_input() {
        let node = HealthEmitterNode::new("test".to_string(), HealthEmitterConfig::default());

        let input = RuntimeData::Audio {
            samples: vec![0.0; 1600], // 100ms at 16kHz
            sample_rate: 16000,
            channels: 1,
            stream_id: Some("audio".to_string()),
            timestamp_us: Some(100_000),
            arrival_ts_us: Some(100_000),
        };

        let result = node.process_async(input).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_factory_creation() {
        let factory = HealthEmitterNodeFactory;
        assert_eq!(factory.node_type(), "HealthEmitterNode");
        assert!(factory.is_multi_output_streaming());

        let params = serde_json::json!({
            "lead_threshold_ms": 75
        });

        let node = factory.create("test_node".to_string(), &params, None);
        assert!(node.is_ok());
    }
}
