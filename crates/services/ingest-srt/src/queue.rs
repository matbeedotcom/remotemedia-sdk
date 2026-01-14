//! Bounded queue module for media data
//!
//! Implements drop-oldest policy for audio and video queues
//! to handle backpressure from slow consumers.

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use remotemedia_health_analyzer::HealthEvent;

/// Bounded queue for audio samples with drop-oldest policy
pub struct BoundedAudioQueue {
    /// Internal queue of audio chunks
    queue: RwLock<VecDeque<AudioChunk>>,

    /// Maximum queue size in milliseconds
    max_ms: u64,

    /// Current accumulated duration in milliseconds
    current_ms: RwLock<u64>,

    /// Sample rate for duration calculation
    sample_rate: u32,

    /// Event sender for overflow warnings
    event_tx: broadcast::Sender<HealthEvent>,

    /// Session ID for logging
    session_id: String,

    /// Total samples dropped
    dropped_samples: RwLock<u64>,
}

/// A chunk of audio samples
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Raw audio samples (f32)
    pub samples: Vec<f32>,

    /// Timestamp in microseconds
    pub timestamp_us: u64,

    /// Duration in milliseconds
    pub duration_ms: f32,
}

impl BoundedAudioQueue {
    /// Create a new bounded audio queue
    pub fn new(
        max_ms: u64,
        sample_rate: u32,
        session_id: String,
        event_tx: broadcast::Sender<HealthEvent>,
    ) -> Self {
        Self {
            queue: RwLock::new(VecDeque::new()),
            max_ms,
            current_ms: RwLock::new(0),
            sample_rate,
            event_tx,
            session_id,
            dropped_samples: RwLock::new(0),
        }
    }

    /// Push audio samples, dropping oldest if necessary
    pub async fn push(&self, samples: Vec<f32>, timestamp_us: u64) {
        let duration_ms = (samples.len() as f32 / self.sample_rate as f32) * 1000.0;
        let chunk = AudioChunk {
            samples,
            timestamp_us,
            duration_ms,
        };

        let mut queue = self.queue.write().await;
        let mut current_ms = self.current_ms.write().await;

        // Drop oldest chunks until we have room
        let mut dropped_count = 0u64;
        while *current_ms + duration_ms as u64 > self.max_ms && !queue.is_empty() {
            if let Some(old) = queue.pop_front() {
                *current_ms = current_ms.saturating_sub(old.duration_ms as u64);
                dropped_count += old.samples.len() as u64;
            }
        }

        // Track dropped samples
        if dropped_count > 0 {
            let mut dropped = self.dropped_samples.write().await;
            *dropped += dropped_count;

            tracing::warn!(
                session_id = %self.session_id,
                dropped_samples = dropped_count,
                queue_ms = *current_ms,
                max_ms = self.max_ms,
                "Audio queue overflow, dropped oldest samples"
            );

            // Emit overflow warning event
            let _ = self.event_tx.send(HealthEvent::dropouts(
                dropped_count as u32,
                Some(self.session_id.clone()),
            ));
        }

        // Push new chunk
        *current_ms += duration_ms as u64;
        queue.push_back(chunk);
    }

    /// Pop the oldest audio chunk
    pub async fn pop(&self) -> Option<AudioChunk> {
        let mut queue = self.queue.write().await;
        let mut current_ms = self.current_ms.write().await;

        if let Some(chunk) = queue.pop_front() {
            *current_ms = current_ms.saturating_sub(chunk.duration_ms as u64);
            Some(chunk)
        } else {
            None
        }
    }

    /// Get current queue duration in milliseconds
    pub async fn duration_ms(&self) -> u64 {
        *self.current_ms.read().await
    }

    /// Get queue length (number of chunks)
    pub async fn len(&self) -> usize {
        self.queue.read().await.len()
    }

    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        self.queue.read().await.is_empty()
    }

    /// Get total dropped samples
    pub async fn dropped_samples(&self) -> u64 {
        *self.dropped_samples.read().await
    }
}

/// Bounded queue for video frames with drop-oldest policy
pub struct BoundedVideoQueue {
    /// Internal queue of video frames
    queue: RwLock<VecDeque<VideoFrame>>,

    /// Maximum number of frames
    max_frames: usize,

    /// Event sender for overflow warnings
    event_tx: broadcast::Sender<HealthEvent>,

    /// Session ID for logging
    session_id: String,

    /// Total frames dropped
    dropped_frames: RwLock<u64>,

    /// Whether video is degraded (reduced fps)
    degraded: RwLock<bool>,

    /// Whether video is disabled due to critical load
    disabled: RwLock<bool>,
}

/// A video frame
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Raw frame data
    pub data: Vec<u8>,

    /// Timestamp in microseconds
    pub timestamp_us: u64,

    /// Frame width
    pub width: u32,

    /// Frame height
    pub height: u32,

    /// Is keyframe
    pub is_keyframe: bool,
}

impl BoundedVideoQueue {
    /// Create a new bounded video queue
    pub fn new(
        max_frames: usize,
        session_id: String,
        event_tx: broadcast::Sender<HealthEvent>,
    ) -> Self {
        Self {
            queue: RwLock::new(VecDeque::new()),
            max_frames,
            event_tx,
            session_id,
            dropped_frames: RwLock::new(0),
            degraded: RwLock::new(false),
            disabled: RwLock::new(false),
        }
    }

    /// Push a video frame, dropping oldest if necessary
    pub async fn push(&self, frame: VideoFrame) -> bool {
        // Check if video is disabled
        if *self.disabled.read().await {
            return false;
        }

        let mut queue = self.queue.write().await;

        // Drop oldest frames if queue is full
        let mut dropped_count = 0u64;
        while queue.len() >= self.max_frames {
            if let Some(_old) = queue.pop_front() {
                dropped_count += 1;
            }
        }

        // Track dropped frames
        if dropped_count > 0 {
            let mut dropped = self.dropped_frames.write().await;
            *dropped += dropped_count;

            tracing::warn!(
                session_id = %self.session_id,
                dropped_frames = dropped_count,
                queue_len = queue.len(),
                max_frames = self.max_frames,
                "Video queue overflow, dropped oldest frames"
            );

            // Emit warning event
            let _ = self.event_tx.send(HealthEvent::freeze(
                dropped_count as u64 * 33, // ~33ms per frame at 30fps
                Some(self.session_id.clone()),
            ));
        }

        // Push new frame
        queue.push_back(frame);
        true
    }

    /// Pop the oldest video frame
    pub async fn pop(&self) -> Option<VideoFrame> {
        self.queue.write().await.pop_front()
    }

    /// Get queue length
    pub async fn len(&self) -> usize {
        self.queue.read().await.len()
    }

    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        self.queue.read().await.is_empty()
    }

    /// Get total dropped frames
    pub async fn dropped_frames(&self) -> u64 {
        *self.dropped_frames.read().await
    }

    /// Set degraded mode (reduces fps)
    pub async fn set_degraded(&self, degraded: bool) {
        let mut deg = self.degraded.write().await;
        if *deg != degraded {
            *deg = degraded;
            tracing::info!(
                session_id = %self.session_id,
                degraded = degraded,
                "Video degradation mode changed"
            );
        }
    }

    /// Check if video is degraded
    pub async fn is_degraded(&self) -> bool {
        *self.degraded.read().await
    }

    /// Disable video processing entirely
    pub async fn set_disabled(&self, disabled: bool) {
        let mut dis = self.disabled.write().await;
        if *dis != disabled {
            *dis = disabled;
            tracing::info!(
                session_id = %self.session_id,
                disabled = disabled,
                "Video processing disabled state changed"
            );
        }
    }

    /// Check if video is disabled
    pub async fn is_disabled(&self) -> bool {
        *self.disabled.read().await
    }
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub audio_queue_ms: u64,
    pub audio_dropped_samples: u64,
    pub video_queue_frames: usize,
    pub video_dropped_frames: u64,
    pub video_degraded: bool,
    pub video_disabled: bool,
}

/// Combined media queues for a session
pub struct MediaQueues {
    pub audio: Arc<BoundedAudioQueue>,
    pub video: Arc<BoundedVideoQueue>,
}

impl MediaQueues {
    /// Create new media queues with configuration
    pub fn new(
        audio_max_ms: u64,
        audio_sample_rate: u32,
        video_max_frames: usize,
        session_id: String,
        event_tx: broadcast::Sender<HealthEvent>,
    ) -> Self {
        Self {
            audio: Arc::new(BoundedAudioQueue::new(
                audio_max_ms,
                audio_sample_rate,
                session_id.clone(),
                event_tx.clone(),
            )),
            video: Arc::new(BoundedVideoQueue::new(
                video_max_frames,
                session_id,
                event_tx,
            )),
        }
    }

    /// Get queue statistics
    pub async fn stats(&self) -> QueueStats {
        QueueStats {
            audio_queue_ms: self.audio.duration_ms().await,
            audio_dropped_samples: self.audio.dropped_samples().await,
            video_queue_frames: self.video.len().await,
            video_dropped_frames: self.video.dropped_frames().await,
            video_degraded: self.video.is_degraded().await,
            video_disabled: self.video.is_disabled().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audio_queue_basic() {
        let (tx, _rx) = broadcast::channel(10);
        let queue = BoundedAudioQueue::new(500, 16000, "test".to_string(), tx);

        // Push some samples
        let samples = vec![0.0f32; 1600]; // 100ms at 16kHz
        queue.push(samples.clone(), 0).await;

        assert_eq!(queue.len().await, 1);
        assert_eq!(queue.duration_ms().await, 100);

        // Pop and verify
        let chunk = queue.pop().await.unwrap();
        assert_eq!(chunk.samples.len(), 1600);
    }

    #[tokio::test]
    async fn test_audio_queue_overflow() {
        let (tx, _rx) = broadcast::channel(10);
        let queue = BoundedAudioQueue::new(200, 16000, "test".to_string(), tx);

        // Push more than max_ms worth of samples
        for i in 0..5 {
            let samples = vec![0.0f32; 1600]; // 100ms each
            queue.push(samples, i * 100_000).await;
        }

        // Should have dropped oldest to stay under 200ms
        assert!(queue.duration_ms().await <= 200);
        assert!(queue.dropped_samples().await > 0);
    }

    #[tokio::test]
    async fn test_video_queue_basic() {
        let (tx, _rx) = broadcast::channel(10);
        let queue = BoundedVideoQueue::new(5, "test".to_string(), tx);

        let frame = VideoFrame {
            data: vec![0u8; 1000],
            timestamp_us: 0,
            width: 1920,
            height: 1080,
            is_keyframe: true,
        };

        assert!(queue.push(frame.clone()).await);
        assert_eq!(queue.len().await, 1);

        let popped = queue.pop().await.unwrap();
        assert_eq!(popped.data.len(), 1000);
    }

    #[tokio::test]
    async fn test_video_queue_overflow() {
        let (tx, _rx) = broadcast::channel(10);
        let queue = BoundedVideoQueue::new(3, "test".to_string(), tx);

        // Push more than max frames
        for i in 0..5 {
            let frame = VideoFrame {
                data: vec![0u8; 100],
                timestamp_us: i * 33_333,
                width: 640,
                height: 480,
                is_keyframe: i == 0,
            };
            queue.push(frame).await;
        }

        assert_eq!(queue.len().await, 3);
        assert!(queue.dropped_frames().await >= 2);
    }

    #[tokio::test]
    async fn test_video_queue_disabled() {
        let (tx, _rx) = broadcast::channel(10);
        let queue = BoundedVideoQueue::new(5, "test".to_string(), tx);

        queue.set_disabled(true).await;
        assert!(queue.is_disabled().await);

        let frame = VideoFrame {
            data: vec![0u8; 100],
            timestamp_us: 0,
            width: 640,
            height: 480,
            is_keyframe: true,
        };

        // Push should return false when disabled
        assert!(!queue.push(frame).await);
        assert_eq!(queue.len().await, 0);
    }

    #[tokio::test]
    async fn test_media_queues_stats() {
        let (tx, _rx) = broadcast::channel(10);
        let queues = MediaQueues::new(500, 16000, 5, "test".to_string(), tx);

        let stats = queues.stats().await;
        assert_eq!(stats.audio_queue_ms, 0);
        assert_eq!(stats.video_queue_frames, 0);
        assert!(!stats.video_degraded);
        assert!(!stats.video_disabled);
    }
}
