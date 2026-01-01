//! Pipeline module for running analysis pipelines
//!
//! This module provides the infrastructure for loading and running
//! analysis pipeline templates on incoming media streams.

mod registry;
mod runner;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use remotemedia_health_analyzer::HealthEvent;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::{PipelineExecutor, TransportData};

pub use registry::{PipelineRegistry, PipelineTemplate};
pub use runner::{PipelineRunner, PipelineOutput};

use crate::demuxer::{DecodedAudio, MpegTsDemuxer};
use crate::session::IngestSession;

/// Run a pipeline for a session, consuming raw MPEG-TS data from the input channel
///
/// This function:
/// 1. Demuxes MPEG-TS to extract audio
/// 2. Feeds decoded audio to PipelineExecutor
/// 3. Emits health events based on analysis results
pub async fn run_pipeline(
    session: Arc<IngestSession>,
    input_rx: mpsc::Receiver<Vec<u8>>,
) {
    let session_id = session.id.clone();
    info!(session_id = %session_id, "Pipeline processing task started");

    // Create channels for demuxer -> pipeline communication
    let (audio_tx, audio_rx) = mpsc::channel::<DecodedAudio>(100);

    // Spawn demuxer task
    let demuxer = MpegTsDemuxer::new(
        input_rx,
        audio_tx,
        16000,  // Target sample rate for analysis
        1,      // Mono
        session_id.clone(),
    );
    let demuxer_handle = tokio::spawn(demuxer.run());

    // Run pipeline processing with decoded audio
    let result = run_pipeline_with_audio(session.clone(), audio_rx).await;

    if let Err(e) = result {
        warn!(session_id = %session_id, error = %e, "Pipeline processing error");
    }

    // Wait for demuxer to finish
    let _ = demuxer_handle.await;

    info!(session_id = %session_id, "Pipeline processing task ended");
}

/// Run the analysis pipeline with decoded audio
async fn run_pipeline_with_audio(
    session: Arc<IngestSession>,
    mut audio_rx: mpsc::Receiver<DecodedAudio>,
) -> Result<(), PipelineError> {
    let session_id = session.id.clone();

    // Create PipelineExecutor
    let executor = PipelineExecutor::new()
        .map_err(|e| PipelineError::Execution(e.to_string()))?;

    // Get manifest for the pipeline template
    let registry = PipelineRegistry::with_defaults();
    let template = registry.get(&session.pipeline_id)
        .ok_or_else(|| PipelineError::TemplateNotFound(session.pipeline_id.clone()))?;

    // Parse manifest from template
    let manifest: Manifest = serde_yaml::from_str(&template.manifest)
        .map_err(|e| PipelineError::InvalidManifest(e.to_string()))?;
    let manifest = Arc::new(manifest);

    // Create streaming session
    let mut pipeline_session = executor.create_session(manifest).await
        .map_err(|e| PipelineError::Execution(e.to_string()))?;

    info!(
        session_id = %session_id,
        template = %template.id,
        "Pipeline session created"
    );

    let mut chunk_count: u64 = 0;
    let start_time = std::time::Instant::now();

    // Process decoded audio chunks
    while let Some(audio) = audio_rx.recv().await {
        chunk_count += 1;

        // Get arrival timestamp
        let arrival_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        // Create RuntimeData::Audio
        let runtime_data = RuntimeData::Audio {
            samples: audio.samples,
            sample_rate: audio.sample_rate,
            channels: audio.channels,
            stream_id: Some(session_id.clone()),
            timestamp_us: Some(audio.timestamp_us),
            arrival_ts_us: Some(arrival_ts),
        };

        // Send to pipeline
        let transport_data = TransportData::new(runtime_data);
        if let Err(e) = pipeline_session.send_input(transport_data).await {
            warn!(
                session_id = %session_id,
                error = %e,
                "Failed to send audio to pipeline"
            );
            break;
        }

        // Process pipeline outputs (non-blocking)
        let mut outputs_received = 0;
        while let Ok(Some(output)) = pipeline_session.try_recv_output() {
            outputs_received += 1;
            process_pipeline_output(&session, output)?;
        }
        if outputs_received > 0 && chunk_count % 50 == 0 {
            debug!(
                session_id = %session_id,
                outputs = outputs_received,
                "Received pipeline outputs"
            );
        }

        // Log progress periodically
        if chunk_count % 50 == 0 {
            let elapsed = start_time.elapsed();
            debug!(
                session_id = %session_id,
                chunks = chunk_count,
                elapsed_secs = elapsed.as_secs(),
                "Processing audio"
            );
        }
    }

    // Drain remaining outputs
    let drain_timeout = std::time::Duration::from_millis(500);
    let drain_start = std::time::Instant::now();
    while drain_start.elapsed() < drain_timeout {
        match pipeline_session.try_recv_output() {
            Ok(Some(output)) => {
                process_pipeline_output(&session, output)?;
            }
            _ => break,
        }
    }

    // Close pipeline session
    if let Err(e) = pipeline_session.close().await {
        warn!(session_id = %session_id, error = %e, "Error closing pipeline session");
    }

    info!(
        session_id = %session_id,
        chunks = chunk_count,
        elapsed_secs = start_time.elapsed().as_secs(),
        "Pipeline processing completed"
    );

    Ok(())
}

/// Process output from the pipeline and emit health events
fn process_pipeline_output(
    session: &IngestSession,
    output: TransportData,
) -> Result<(), PipelineError> {
    let data = output.data;

    // Convert RuntimeData to HealthEvent if applicable
    if let Some(event) = convert_to_health_event(&data) {
        match session.event_tx.send(event) {
            Ok(subscriber_count) => {
                debug!(
                    session_id = %session.id,
                    subscriber_count = subscriber_count,
                    "Event sent to broadcast channel"
                );
            }
            Err(_) => {
                debug!(
                    session_id = %session.id,
                    "No subscribers for event broadcast"
                );
            }
        }
    }

    Ok(())
}

/// Convert RuntimeData output to HealthEvent
fn convert_to_health_event(data: &RuntimeData) -> Option<HealthEvent> {
    match data {
        RuntimeData::Json(value) => {
            // Try to parse as HealthEvent
            if let Ok(event) = serde_json::from_value::<HealthEvent>(value.clone()) {
                return Some(event);
            }

            if let Some(obj) = value.as_object() {
                // Check for _schema field from audio analysis nodes
                let schema = obj.get("_schema").and_then(|v| v.as_str());

                if let Some(schema) = schema {
                    match schema {
                        "silence_event" => {
                            // Only emit if sustained silence or dropouts detected
                            let is_sustained = obj.get("is_sustained_silence")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let has_dropouts = obj.get("has_intermittent_dropouts")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if is_sustained {
                                let duration_ms = obj.get("silence_duration_ms")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as f32;
                                let rms_db = obj.get("rms_db")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(-60.0) as f32;
                                debug!("Silence detected: {}ms at {}dB", duration_ms, rms_db);
                                return Some(HealthEvent::silence(duration_ms, rms_db, None));
                            }
                            if has_dropouts {
                                let count = obj.get("dropout_count")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32;
                                debug!("Dropouts detected: {} in window", count);
                                return Some(HealthEvent::dropouts(count, None));
                            }
                            return None;
                        }
                        "clipping_event" => {
                            // Only emit if clipping detected
                            let is_clipping = obj.get("is_clipping")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if is_clipping {
                                let ratio = obj.get("saturation_ratio")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as f32;
                                let crest_db = obj.get("crest_factor_db")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as f32;
                                debug!("Clipping detected: ratio={}, crest={}dB", ratio, crest_db);
                                return Some(HealthEvent::clipping(ratio, crest_db, None));
                            }
                            return None;
                        }
                        "audio_level_event" => {
                            // Only emit if low volume detected
                            let is_low = obj.get("is_low_volume")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if is_low {
                                let rms_db = obj.get("rms_db")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(-40.0) as f32;
                                let peak_db = obj.get("peak_db")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(-40.0) as f32;
                                debug!("Low volume detected: rms={}dB, peak={}dB", rms_db, peak_db);
                                return Some(HealthEvent::low_volume(rms_db, peak_db, None));
                            }
                            return None;
                        }
                        "channel_balance_event" => {
                            // Only emit if imbalance or dead channel detected
                            let is_imbalanced = obj.get("is_imbalanced")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let has_dead = obj.get("has_dead_channel")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if is_imbalanced || has_dead {
                                let imbalance_db = obj.get("imbalance_db")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as f32;
                                let dead_channel = obj.get("dead_channel")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("none")
                                    .to_string();
                                debug!("Channel imbalance: {}dB, dead={}", imbalance_db, dead_channel);
                                return Some(HealthEvent::channel_imbalance(imbalance_db, dead_channel, None));
                            }
                            return None;
                        }
                        _ => {
                            debug!("Unknown schema: {}", schema);
                        }
                    }
                }

                // Check for common event types in the JSON
                // HealthEmitterNode uses "type" field, some other sources use "event_type"
                let event_type = obj.get("type")
                    .or_else(|| obj.get("event_type"))
                    .and_then(|v| v.as_str());

                if let Some(event_type) = event_type {
                    match event_type {
                        "health" => {
                            let score = obj.get("score")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(1.0);
                            debug!("Converting health event with score: {}", score);
                            return Some(HealthEvent::health(score, vec![]));
                        }
                        "silence" => {
                            let duration_ms = obj.get("duration_ms")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;
                            let rms_db = obj.get("rms_db")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(-60.0) as f32;
                            return Some(HealthEvent::silence(duration_ms, rms_db, None));
                        }
                        "clipping" => {
                            let ratio = obj.get("clipping_ratio")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;
                            return Some(HealthEvent::clipping(ratio, 0.0, None));
                        }
                        "low_volume" => {
                            let rms_db = obj.get("rms_db")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(-40.0) as f32;
                            return Some(HealthEvent::low_volume(rms_db, rms_db, None));
                        }
                        "drift" | "freeze" | "cadence" | "av_skew" => {
                            // These are also valid health-related events from HealthEmitterNode
                            debug!("Converting {} event", event_type);
                            return Some(HealthEvent::health(1.0, vec![]));
                        }
                        _ => {
                            debug!("Unknown event type: {}", event_type);
                        }
                    }
                }
            }
            None
        }
        RuntimeData::Audio { .. } => {
            // For audio pass-through, we could analyze and emit events
            // But typically the pipeline nodes handle this
            None
        }
        _ => None,
    }
}

/// Pipeline execution errors
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Channel closed")]
    ChannelClosed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_health_event() {
        let json = serde_json::json!({
            "event_type": "health",
            "score": 0.95
        });
        let data = RuntimeData::Json(json);
        let event = convert_to_health_event(&data);
        assert!(event.is_some());
    }

    #[test]
    fn test_convert_silence_event() {
        let json = serde_json::json!({
            "event_type": "silence",
            "duration_ms": 5000
        });
        let data = RuntimeData::Json(json);
        let event = convert_to_health_event(&data);
        assert!(event.is_some());
    }
}
