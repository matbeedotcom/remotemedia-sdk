//! Streaming Protocol Ingestion Adapter for RemoteMedia SDK
//!
//! This crate provides streaming protocol ingestion support for RemoteMedia pipelines
//! using FFmpeg's native URL handling.
//!
//! # Usage
//!
//! Register the plugin with the global ingest registry:
//!
//! ```ignore
//! use std::sync::Arc;
//! use remotemedia_ingest_rtmp::RtmpIngestPlugin;
//! use remotemedia_runtime_core::ingestion::global_ingest_registry;
//!
//! // Register the streaming plugin
//! global_ingest_registry().register(Arc::new(RtmpIngestPlugin))?;
//!
//! // Now streaming URLs can be used
//! let config = IngestConfig::from_url("rtmp://localhost:1935/live/stream");
//! let source = global_ingest_registry().create_from_uri(&config)?;
//! ```
//!
//! # Supported Protocols
//!
//! - `rtmp://` - Real-Time Messaging Protocol (unencrypted)
//! - `rtmps://` - RTMP over TLS
//! - `rtsp://` - Real-Time Streaming Protocol
//! - `rtsps://` - RTSP over TLS
//! - `udp://` - UDP (MPEG-TS, raw) - use `?listen=1` for server mode
//! - `rtp://` - RTP over UDP
//! - `srt://` - Secure Reliable Transport
//!
//! # UDP Side-Car Pattern
//!
//! For monitoring streams without affecting the primary output:
//!
//! ```bash
//! # FFmpeg tee: primary output + UDP side-car
//! ffmpeg -i rtmp://source/live/stream \
//!   -c copy -f tee \
//!   "[f=flv]rtmp://primary/live/stream|[f=mpegts]udp://127.0.0.1:5004"
//!
//! # Analyzer listens on UDP
//! remotemedia-demo --ingest "udp://127.0.0.1:5004?listen=1" --json
//! ```
//!

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, RwLock};

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::ingestion::{
    AudioConfig, IngestConfig, IngestMetadata, IngestPlugin, IngestSource, IngestStatus,
    IngestStream, ReconnectConfig, TrackSelection, VideoConfig,
};
use remotemedia_runtime_core::Error;

mod demuxer;
pub mod audio_samples;

pub use demuxer::RtmpDemuxer;
pub use audio_samples::{sample_formats, convert_packed_samples_to_f32, convert_planar_samples_to_f32, ffmpeg_error_string};

/// Streaming protocol ingest plugin
///
/// Handles various streaming URI schemes for live stream ingestion.
/// All protocols are handled via FFmpeg's native URL support.
///
/// Supported schemes:
/// - `rtmp://`, `rtmps://` - RTMP streaming
/// - `rtsp://`, `rtsps://` - RTSP streaming  
/// - `udp://` - UDP (MPEG-TS, raw) - use `?listen=1` for server mode
/// - `rtp://` - RTP over UDP
/// - `srt://` - Secure Reliable Transport
pub struct RtmpIngestPlugin;

impl IngestPlugin for RtmpIngestPlugin {
    fn name(&self) -> &'static str {
        "streaming"
    }

    fn schemes(&self) -> &'static [&'static str] {
        &["rtmp", "rtmps", "rtsp", "rtsps", "udp", "rtp", "srt"]
    }

    fn create(&self, config: &IngestConfig) -> Result<Box<dyn IngestSource>, Error> {
        Ok(Box::new(RtmpIngestSource::new(config.clone())?))
    }

    fn validate(&self, config: &IngestConfig) -> Result<(), Error> {
        // Check URL scheme
        let url = config.url.to_lowercase();
        let valid_schemes = [
            "rtmp://", "rtmps://", 
            "rtsp://", "rtsps://",
            "udp://", "rtp://", "srt://"
        ];
        
        if !valid_schemes.iter().any(|s| url.starts_with(s)) {
            return Err(Error::ConfigError(format!(
                "Invalid streaming URL: {}. Expected one of: {}",
                config.url,
                valid_schemes.join(", ")
            )));
        }

        // Validate URL can be parsed
        url::Url::parse(&config.url).map_err(|e| {
            Error::ConfigError(format!("Invalid streaming URL '{}': {}", config.url, e))
        })?;

        Ok(())
    }
}

/// RTMP/RTMPS ingest source
///
/// Active connection to an RTMP stream, producing decoded audio/video data.
pub struct RtmpIngestSource {
    /// Configuration for this source
    config: IngestConfig,

    /// Current connection status
    status: Arc<RwLock<IngestStatus>>,

    /// Discovered stream metadata
    metadata: Arc<RwLock<Option<IngestMetadata>>>,

    /// Stop signal sender (None if not started)
    stop_tx: Option<oneshot::Sender<()>>,

    /// Flag to track if we've been stopped
    stopped: Arc<AtomicBool>,
}

impl RtmpIngestSource {
    /// Create a new RTMP ingest source
    pub fn new(config: IngestConfig) -> Result<Self, Error> {
        Ok(Self {
            config,
            status: Arc::new(RwLock::new(IngestStatus::Idle)),
            metadata: Arc::new(RwLock::new(None)),
            stop_tx: None,
            stopped: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Get the configured URL
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Get the audio configuration
    pub fn audio_config(&self) -> Option<&AudioConfig> {
        self.config.audio.as_ref()
    }

    /// Get the video configuration
    pub fn video_config(&self) -> Option<&VideoConfig> {
        self.config.video.as_ref()
    }

    /// Get the reconnect configuration
    pub fn reconnect_config(&self) -> &ReconnectConfig {
        &self.config.reconnect
    }

    /// Get the track selection
    pub fn track_selection(&self) -> &TrackSelection {
        &self.config.track_selection
    }

    /// Internal: Set status
    async fn set_status(&self, status: IngestStatus) {
        let mut guard = self.status.write().await;
        *guard = status;
    }
}

#[async_trait]
impl IngestSource for RtmpIngestSource {
    async fn start(&mut self) -> Result<IngestStream, Error> {
        // Check if already started
        if self.stop_tx.is_some() {
            return Err(Error::ConfigError(
                "Source already started. Call stop() first.".to_string(),
            ));
        }

        // Reset stopped flag
        self.stopped.store(false, Ordering::SeqCst);

        // Create stop signal channel
        let (stop_tx, stop_rx) = oneshot::channel();
        self.stop_tx = Some(stop_tx);

        // Create data channel
        let (data_tx, data_rx) = mpsc::channel::<RuntimeData>(32);

        // Update status to Connecting
        self.set_status(IngestStatus::Connecting).await;

        // Clone values for the decode task
        let url = self.config.url.clone();
        let audio_config = self.config.audio.clone();
        let video_config = self.config.video.clone();
        let track_selection = self.config.track_selection.clone();
        let reconnect_config = self.config.reconnect.clone();
        let status = Arc::clone(&self.status);
        let metadata_store = Arc::clone(&self.metadata);
        let stopped = Arc::clone(&self.stopped);

        // Spawn the decode task
        tokio::spawn(async move {
            let result = ingest_rtmp_stream(
                url,
                audio_config,
                video_config,
                track_selection,
                reconnect_config,
                data_tx,
                stop_rx,
                status.clone(),
                metadata_store.clone(),
                stopped,
            )
            .await;

            // Update final status
            let final_status = match result {
                Ok(()) => IngestStatus::Disconnected,
                Err(e) => IngestStatus::error(e.to_string()),
            };
            let mut guard = status.write().await;
            *guard = final_status;
        });

        // Wait briefly for connection to establish
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Return stream with placeholder metadata (will be updated by decode task)
        let metadata = {
            let guard = self.metadata.read().await;
            guard.clone().unwrap_or_default()
        };

        Ok(IngestStream::new(data_rx, metadata))
    }

    async fn stop(&mut self) -> Result<(), Error> {
        // Set stopped flag
        self.stopped.store(true, Ordering::SeqCst);

        // Send stop signal if we have one
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()); // Ignore error if receiver dropped
        }

        // Update status
        self.set_status(IngestStatus::Disconnected).await;

        Ok(())
    }

    fn status(&self) -> IngestStatus {
        // Use try_read to avoid blocking
        self.status
            .try_read()
            .map(|guard| guard.clone())
            .unwrap_or(IngestStatus::Idle)
    }

    fn metadata(&self) -> Option<&IngestMetadata> {
        // Can't return reference due to RwLock, return None for now
        // Callers should use IngestStream::metadata() instead
        None
    }
}

/// Main RTMP ingestion loop
///
/// Handles connection, reconnection, demuxing, and decoding.
#[allow(clippy::too_many_arguments)]
async fn ingest_rtmp_stream(
    url: String,
    audio_config: Option<AudioConfig>,
    video_config: Option<VideoConfig>,
    track_selection: TrackSelection,
    reconnect_config: ReconnectConfig,
    data_tx: mpsc::Sender<RuntimeData>,
    stop_rx: oneshot::Receiver<()>,
    status: Arc<RwLock<IngestStatus>>,
    metadata_store: Arc<RwLock<Option<IngestMetadata>>>,
    stopped: Arc<AtomicBool>,
) -> Result<(), Error> {
    let mut attempt = 0u32;

    // Wrap stop_rx in an Option so we can take it once
    let mut stop_rx = Some(stop_rx);

    loop {
        // Check if we should stop
        if stopped.load(Ordering::SeqCst) {
            tracing::info!("RTMP ingest stopped by request");
            return Ok(());
        }

        // Update status
        if attempt > 0 {
            let max_attempts = reconnect_config.max_attempts;
            let mut guard = status.write().await;
            *guard = IngestStatus::Reconnecting {
                attempt,
                max_attempts,
            };
        }

        tracing::info!(
            "Connecting to RTMP stream: {} (attempt {})",
            url,
            attempt + 1
        );

        // Try to connect and decode
        // Take stop_rx only on first attempt, then use None
        let stop_rx_for_decode = if attempt == 0 { stop_rx.take() } else { None };

        match connect_and_decode(
            &url,
            audio_config.as_ref(),
            video_config.as_ref(),
            &track_selection,
            &data_tx,
            stop_rx_for_decode,
            &status,
            &metadata_store,
            &stopped,
        )
        .await
        {
            Ok(()) => {
                tracing::info!("RTMP stream ended gracefully");
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("RTMP stream error: {}", e);

                // Check if we should reconnect
                if !reconnect_config.enabled {
                    return Err(e);
                }

                // Check max attempts
                if reconnect_config.max_attempts > 0 && attempt >= reconnect_config.max_attempts {
                    tracing::error!(
                        "Max reconnection attempts ({}) reached",
                        reconnect_config.max_attempts
                    );
                    return Err(e);
                }

                // Wait before reconnecting (check stopped flag during wait)
                let delay_ms = reconnect_config.delay_for_attempt(attempt);
                tracing::info!("Reconnecting in {}ms...", delay_ms);

                let start = tokio::time::Instant::now();
                let delay = tokio::time::Duration::from_millis(delay_ms);
                while start.elapsed() < delay {
                    if stopped.load(Ordering::SeqCst) {
                        tracing::info!("Stop received during reconnect delay");
                        return Ok(());
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }

                attempt += 1;
            }
        }
    }
}

/// Connect to RTMP stream and decode packets
#[allow(clippy::too_many_arguments)]
async fn connect_and_decode(
    url: &str,
    audio_config: Option<&AudioConfig>,
    video_config: Option<&VideoConfig>,
    track_selection: &TrackSelection,
    data_tx: &mpsc::Sender<RuntimeData>,
    mut stop_rx: Option<oneshot::Receiver<()>>,
    status: &Arc<RwLock<IngestStatus>>,
    metadata_store: &Arc<RwLock<Option<IngestMetadata>>>,
    stopped: &Arc<AtomicBool>,
) -> Result<(), Error> {
    // Create demuxer
    let mut demuxer =
        RtmpDemuxer::open(url, audio_config, video_config, track_selection).await?;

    // Store metadata
    let metadata = demuxer.metadata();
    {
        let mut guard = metadata_store.write().await;
        *guard = Some(metadata);
    }

    // Update status to connected
    {
        let mut guard = status.write().await;
        *guard = IngestStatus::Connected;
    }

    tracing::info!("Connected to RTMP stream: {}", url);

    // Decode loop
    loop {
        // Check stop signal
        if stopped.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Check oneshot stop signal if available
        if let Some(ref mut rx) = stop_rx {
            match rx.try_recv() {
                Ok(()) => {
                    tracing::info!("Stop signal received");
                    return Ok(());
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    // Sender dropped - treat as stop
                    tracing::info!("Stop signal sender dropped");
                    return Ok(());
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // No signal yet, continue
                }
            }
        }

        // Get next frame
        match demuxer.next_frame().await {
            Ok(Some(data)) => {
                // Send data
                if data_tx.send(data).await.is_err() {
                    tracing::warn!("Data channel closed");
                    return Ok(());
                }
            }
            Ok(None) => {
                // End of stream
                tracing::info!("End of RTMP stream");
                return Ok(());
            }
            Err(e) => {
                tracing::error!("Decode error: {}", e);
                return Err(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtmp_plugin_validates_streaming_urls() {
        let plugin = RtmpIngestPlugin;

        // Valid RTMP URLs
        let config = IngestConfig::from_url("rtmp://localhost:1935/live/stream");
        assert!(plugin.validate(&config).is_ok());

        let config = IngestConfig::from_url("rtmps://secure.server.com/app/key");
        assert!(plugin.validate(&config).is_ok());

        // Valid UDP URLs (side-car pattern)
        let config = IngestConfig::from_url("udp://127.0.0.1:5004?listen=1");
        assert!(plugin.validate(&config).is_ok());
        
        let config = IngestConfig::from_url("rtp://127.0.0.1:5004");
        assert!(plugin.validate(&config).is_ok());
        
        let config = IngestConfig::from_url("srt://localhost:4000?mode=listener");
        assert!(plugin.validate(&config).is_ok());

        // Valid RTSP URLs
        let config = IngestConfig::from_url("rtsp://localhost:8554/test");
        assert!(plugin.validate(&config).is_ok());

        let config = IngestConfig::from_url("rtsps://secure.server.com/stream");
        assert!(plugin.validate(&config).is_ok());

        // Invalid URLs
        let config = IngestConfig::from_url("http://localhost/stream");
        assert!(plugin.validate(&config).is_err());

        let config = IngestConfig::from_url("file:///path/to/file.mp4");
        assert!(plugin.validate(&config).is_err());

        let config = IngestConfig::from_url("./local.wav");
        assert!(plugin.validate(&config).is_err());
    }

    #[test]
    fn test_rtmp_plugin_rejects_non_streaming_urls() {
        let plugin = RtmpIngestPlugin;

        let non_streaming_urls = [
            "http://localhost/stream",
            "https://secure.server.com/video",
            "file:///path/to/file.mp4",
            "./local.wav",
            "/absolute/path.mp4",
        ];

        for url in non_streaming_urls {
            let config = IngestConfig::from_url(url);
            let result = plugin.validate(&config);
            assert!(result.is_err(), "Should reject non-streaming URL: {}", url);
        }
    }

    #[test]
    fn test_rtmp_plugin_schemes() {
        let plugin = RtmpIngestPlugin;
        let schemes = plugin.schemes();
        assert!(schemes.contains(&"rtmp"));
        assert!(schemes.contains(&"rtmps"));
        assert!(schemes.contains(&"rtsp"));
        assert!(schemes.contains(&"rtsps"));
        assert!(schemes.contains(&"udp"));
        assert!(schemes.contains(&"rtp"));
        assert!(schemes.contains(&"srt"));
        assert_eq!(schemes.len(), 7);
    }

    #[test]
    fn test_rtmp_plugin_name() {
        let plugin = RtmpIngestPlugin;
        assert_eq!(plugin.name(), "streaming");
    }
}
