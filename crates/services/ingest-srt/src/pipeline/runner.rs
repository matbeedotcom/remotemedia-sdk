//! Pipeline runner for executing analysis pipelines
//!
//! This module handles running pipelines on incoming media data
//! and collecting the resulting alerts/events.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc};

use remotemedia_health_analyzer::HealthEvent;

use super::PipelineTemplate;

/// Output from a pipeline execution
#[derive(Debug, Clone)]
pub enum PipelineOutput {
    /// Health event from analysis
    Event(HealthEvent),

    /// Pipeline error
    Error(String),

    /// Pipeline completed (stream ended)
    Complete,
}

/// Pipeline runner for processing media through an analysis pipeline
pub struct PipelineRunner {
    /// Template being used
    template: Arc<PipelineTemplate>,

    /// Session ID for this runner
    session_id: String,

    /// Stream start time for relative_ms calculation
    stream_started_at: DateTime<Utc>,

    /// Input receiver for media data
    input_rx: mpsc::Receiver<Vec<u8>>,

    /// Output sender for events
    event_tx: broadcast::Sender<HealthEvent>,

    /// Shutdown signal
    shutdown_rx: Option<broadcast::Receiver<()>>,
}

impl PipelineRunner {
    /// Create a new pipeline runner
    pub fn new(
        template: Arc<PipelineTemplate>,
        session_id: String,
        input_rx: mpsc::Receiver<Vec<u8>>,
        event_tx: broadcast::Sender<HealthEvent>,
    ) -> Self {
        Self {
            template,
            session_id,
            stream_started_at: Utc::now(),
            input_rx,
            event_tx,
            shutdown_rx: None,
        }
    }

    /// Set shutdown signal receiver
    pub fn with_shutdown(mut self, shutdown_rx: broadcast::Receiver<()>) -> Self {
        self.shutdown_rx = Some(shutdown_rx);
        self
    }

    /// Get the template ID
    pub fn template_id(&self) -> &str {
        &self.template.id
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Calculate relative_ms from stream start
    pub fn relative_ms(&self) -> u64 {
        let now = Utc::now();
        (now - self.stream_started_at).num_milliseconds().max(0) as u64
    }

    /// Run the pipeline processing loop
    ///
    /// This processes incoming media data through the pipeline and
    /// emits events/alerts to the event channel.
    pub async fn run(mut self) -> Result<(), PipelineError> {
        tracing::info!(
            "Starting pipeline runner for session {} using template {}",
            self.session_id,
            self.template.id
        );

        // Emit stream started event
        let _ = self.event_tx.send(HealthEvent::stream_started(Some(self.session_id.clone())));

        // Main processing loop
        loop {
            // Check for shutdown
            if let Some(ref mut shutdown_rx) = self.shutdown_rx {
                match shutdown_rx.try_recv() {
                    Ok(()) => {
                        tracing::info!("Pipeline runner shutdown requested");
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Empty) => {}
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                }
            }

            // Receive input data
            match self.input_rx.recv().await {
                Some(data) => {
                    // Process the data through the pipeline
                    // For now, this is a placeholder - in a full implementation,
                    // this would parse the manifest and run actual analysis nodes
                    if let Err(e) = self.process_chunk(&data).await {
                        tracing::error!("Pipeline processing error: {}", e);
                    }
                }
                None => {
                    // Input channel closed - stream ended
                    tracing::info!("Pipeline input channel closed");
                    break;
                }
            }
        }

        // Emit stream ended event
        let relative_ms = self.relative_ms();
        let _ = self.event_tx.send(HealthEvent::stream_ended(
            relative_ms,
            "normal".to_string(),
            Some(self.session_id.clone()),
        ));

        tracing::info!("Pipeline runner completed for session {}", self.session_id);
        Ok(())
    }

    /// Process a single chunk of data through the pipeline
    async fn process_chunk(&mut self, data: &[u8]) -> Result<(), PipelineError> {
        // Decode the data format
        // The listener packs audio as: sample_rate (4) + channels (4) + samples (f32...)
        if data.len() < 8 {
            return Ok(()); // Too short, skip
        }

        let sample_rate = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let channels = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let samples_data = &data[8..];

        let num_samples = samples_data.len() / 4;
        if num_samples == 0 {
            return Ok(());
        }

        // Decode samples
        let mut samples = Vec::with_capacity(num_samples);
        for i in 0..num_samples {
            let offset = i * 4;
            if offset + 4 <= samples_data.len() {
                let bytes = [
                    samples_data[offset],
                    samples_data[offset + 1],
                    samples_data[offset + 2],
                    samples_data[offset + 3],
                ];
                samples.push(f32::from_le_bytes(bytes));
            }
        }

        // Run basic audio analysis
        let analysis = self.analyze_audio(&samples, sample_rate, channels);

        // Emit any detected issues as health events
        for event in analysis {
            let _ = self.event_tx.send(event);
        }

        Ok(())
    }

    /// Basic audio analysis
    ///
    /// In a full implementation, this would run the nodes from the manifest.
    /// For now, we implement some basic detectors inline.
    fn analyze_audio(&self, samples: &[f32], _sample_rate: u32, _channels: u32) -> Vec<HealthEvent> {
        let mut events = Vec::new();
        let _relative_ms = self.relative_ms();

        // Calculate RMS level
        let rms = if samples.is_empty() {
            0.0
        } else {
            let sum: f32 = samples.iter().map(|s| s * s).sum();
            (sum / samples.len() as f32).sqrt()
        };

        let db = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            -96.0
        };

        // Detect silence (< -40 dB)
        if db < -40.0 {
            // Note: A real implementation would track silence duration
            // before emitting an event. This is simplified.
            tracing::trace!("Low audio level detected: {:.1} dB", db);
        }

        // Detect clipping (samples near ±1.0)
        let clipping_threshold = 0.95;
        let clipping_samples = samples.iter().filter(|s| s.abs() > clipping_threshold).count();
        let clipping_ratio = clipping_samples as f32 / samples.len() as f32;

        if clipping_ratio > 0.01 {
            // More than 1% clipping
            // Calculate crest factor (peak to RMS ratio in dB)
            let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, |a, b| a.max(b));
            let crest_factor_db = if rms > 0.0 && peak > 0.0 {
                20.0 * (peak / rms).log10()
            } else {
                0.0
            };
            events.push(HealthEvent::clipping(
                clipping_ratio,
                crest_factor_db,
                Some(self.session_id.clone()),
            ));
        }

        // Detect low volume (< -35 dB sustained)
        if db < -35.0 && db > -60.0 {
            // peak_db is the peak level
            let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, |a, b| a.max(b));
            let peak_db = if peak > 0.0 { 20.0 * peak.log10() } else { -96.0 };
            events.push(HealthEvent::low_volume(
                db,       // rms_db
                peak_db,  // peak_db
                Some(self.session_id.clone()),
            ));
        }

        events
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

    fn test_template() -> PipelineTemplate {
        PipelineTemplate::new("test", "Test", "manifest: test")
    }

    #[tokio::test]
    async fn test_runner_creation() {
        let (input_tx, input_rx) = mpsc::channel(10);
        let (event_tx, _) = broadcast::channel(10);

        let runner = PipelineRunner::new(
            Arc::new(test_template()),
            "sess_123".to_string(),
            input_rx,
            event_tx,
        );

        assert_eq!(runner.template_id(), "test");
        assert_eq!(runner.session_id(), "sess_123");
        drop(input_tx); // Suppress warning
    }

    #[tokio::test]
    async fn test_runner_relative_ms() {
        let (_, input_rx) = mpsc::channel(10);
        let (event_tx, _) = broadcast::channel(10);

        let runner = PipelineRunner::new(
            Arc::new(test_template()),
            "sess_123".to_string(),
            input_rx,
            event_tx,
        );

        // relative_ms should be close to 0 right after creation
        assert!(runner.relative_ms() < 100);

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should have increased
        assert!(runner.relative_ms() >= 50);
    }

    #[tokio::test]
    async fn test_runner_emits_stream_events() {
        let (input_tx, input_rx) = mpsc::channel(10);
        let (event_tx, mut event_rx) = broadcast::channel(10);

        let runner = PipelineRunner::new(
            Arc::new(test_template()),
            "sess_123".to_string(),
            input_rx,
            event_tx,
        );

        // Drop sender to close input channel
        drop(input_tx);

        // Run the pipeline (will exit immediately due to closed channel)
        runner.run().await.unwrap();

        // Should receive stream_started and stream_ended
        let event1 = event_rx.recv().await.unwrap();
        assert!(event1.is_system());
        assert_eq!(event1.event_type(), "stream_started");

        let event2 = event_rx.recv().await.unwrap();
        assert!(event2.is_system());
        assert_eq!(event2.event_type(), "stream_ended");
    }

    #[test]
    fn test_analyze_audio_clipping() {
        let (_, input_rx) = mpsc::channel::<Vec<u8>>(10);
        let (event_tx, _) = broadcast::channel(10);

        let runner = PipelineRunner::new(
            Arc::new(test_template()),
            "sess_123".to_string(),
            input_rx,
            event_tx,
        );

        // Samples with clipping
        let samples: Vec<f32> = vec![0.99, 0.98, 0.97, -0.99, -0.98];
        let events = runner.analyze_audio(&samples, 16000, 1);

        // Should detect clipping
        assert!(events.iter().any(|e| e.event_type() == "clipping"));
    }
}
