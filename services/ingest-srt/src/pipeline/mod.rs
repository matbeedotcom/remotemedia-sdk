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

use remotemedia_pipeline_runner::convert_output_to_health_event;
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::{PipelineExecutor, TransportData};

pub use registry::{PipelineRegistry, PipelineTemplate};
pub use runner::{PipelineRunner, PipelineOutput};

use crate::demuxer::{DecodedAudio, MpegTsDemuxer, VideoTiming};
use crate::session::IngestSession;

/// Run a pipeline for a session, consuming raw MPEG-TS data from the input channel
///
/// This function:
/// 1. Demuxes MPEG-TS to extract audio and video timing
/// 2. Feeds decoded audio and video timing to PipelineExecutor
/// 3. Emits health events based on analysis results (including A/V drift)
pub async fn run_pipeline(
    session: Arc<IngestSession>,
    input_rx: mpsc::Receiver<Vec<u8>>,
) {
    let session_id = session.id.clone();
    info!(session_id = %session_id, "Pipeline processing task started");

    // Create channels for demuxer -> pipeline communication
    let (audio_tx, audio_rx) = mpsc::channel::<DecodedAudio>(100);
    let (video_tx, video_rx) = mpsc::channel::<VideoTiming>(100);

    // Spawn demuxer task with video support for A/V sync
    let demuxer = MpegTsDemuxer::with_video(
        input_rx,
        audio_tx,
        video_tx,
        16000,  // Target sample rate for analysis
        1,      // Mono
        session_id.clone(),
    );
    let demuxer_handle = tokio::spawn(demuxer.run());

    // Run pipeline processing with decoded audio and video timing
    let result = run_pipeline_with_av(session.clone(), audio_rx, video_rx).await;

    if let Err(e) = result {
        warn!(session_id = %session_id, error = %e, "Pipeline processing error");
    }

    // Wait for demuxer to finish
    let _ = demuxer_handle.await;

    info!(session_id = %session_id, "Pipeline processing task ended");
}

/// Run the analysis pipeline with decoded audio and video timing
async fn run_pipeline_with_av(
    session: Arc<IngestSession>,
    mut audio_rx: mpsc::Receiver<DecodedAudio>,
    mut video_rx: mpsc::Receiver<VideoTiming>,
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

    let mut audio_chunk_count: u64 = 0;
    let mut video_frame_count: u64 = 0;
    let start_time = std::time::Instant::now();
    let mut audio_done = false;
    let mut video_done = false;

    // Process decoded audio and video timing concurrently
    loop {
        if audio_done && video_done {
            break;
        }

        tokio::select! {
            // Handle audio
            audio_opt = audio_rx.recv(), if !audio_done => {
                match audio_opt {
                    Some(audio) => {
                        audio_chunk_count += 1;

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
                    }
                    None => {
                        audio_done = true;
                    }
                }
            }

            // Handle video timing (for A/V drift detection)
            video_opt = video_rx.recv(), if !video_done => {
                match video_opt {
                    Some(video) => {
                        video_frame_count += 1;

                        // Get arrival timestamp
                        let arrival_ts = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_micros() as u64)
                            .unwrap_or(0);

                        // Create RuntimeData::Video with just timing info
                        // Using minimal placeholder data since we only need PTS for drift detection
                        use remotemedia_runtime_core::data::video::PixelFormat;
                        let runtime_data = RuntimeData::Video {
                            pixel_data: vec![],  // No pixel data - just timing
                            width: 1,  // Minimal valid dimensions
                            height: 1,
                            format: PixelFormat::Unspecified,
                            codec: None,
                            frame_number: video_frame_count as u64,
                            timestamp_us: video.timestamp_us,
                            is_keyframe: false,
                            stream_id: Some(session_id.clone()),
                            arrival_ts_us: Some(arrival_ts),
                        };

                        // Send to pipeline for drift detection
                        let transport_data = TransportData::new(runtime_data);
                        if let Err(e) = pipeline_session.send_input(transport_data).await {
                            debug!(
                                session_id = %session_id,
                                error = %e,
                                "Failed to send video timing to pipeline"
                            );
                            // Don't break on video send failure - continue with audio
                        }

                        // Log first video frame
                        if video_frame_count == 1 {
                            info!(
                                session_id = %session_id,
                                video_ts_us = video.timestamp_us,
                                "First video timing sent to pipeline for A/V sync"
                            );
                        }
                    }
                    None => {
                        video_done = true;
                    }
                }
            }
        }

        // Process pipeline outputs (non-blocking)
        let mut outputs_received = 0;
        while let Ok(Some(output)) = pipeline_session.try_recv_output() {
            outputs_received += 1;
            process_pipeline_output(&session, output)?;
        }

        let total_chunks = audio_chunk_count + video_frame_count;
        if outputs_received > 0 && total_chunks % 50 == 0 {
            debug!(
                session_id = %session_id,
                outputs = outputs_received,
                "Received pipeline outputs"
            );
        }

        // Log progress periodically
        if total_chunks % 100 == 0 && total_chunks > 0 {
            let elapsed = start_time.elapsed();
            debug!(
                session_id = %session_id,
                audio_chunks = audio_chunk_count,
                video_frames = video_frame_count,
                elapsed_secs = elapsed.as_secs(),
                "Processing A/V"
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
        audio_chunks = audio_chunk_count,
        video_frames = video_frame_count,
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

    // Use the shared library's conversion function
    if let Some(event) = convert_output_to_health_event(&data) {
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
        // Uses "type" field, not "event_type", and requires "alerts" field
        let json = serde_json::json!({
            "type": "health",
            "score": 0.95,
            "alerts": []
        });
        let data = RuntimeData::Json(json);
        let event = convert_output_to_health_event(&data);
        assert!(event.is_some());
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
    }
}
