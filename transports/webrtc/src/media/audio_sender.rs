//! Audio sender with ring buffer and dedicated transmission thread
//!
//! This module provides a ring buffer-based audio transmission system that:
//! - Decouples audio production from transmission
//! - Maintains consistent real-time playback timing
//! - Handles backpressure gracefully via ring buffer
//! - Uses a dedicated OS thread for precise timing

// Phase 4 (US2) audio transmission infrastructure
#![allow(dead_code)]

use crate::{Error, Result};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, warn};
use webrtc::media::Sample;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

/// Audio frame ready for transmission
#[derive(Clone)]
struct AudioFrame {
    /// Encoded Opus data
    data: Vec<u8>,
    /// Number of samples this frame represents (for timestamp calculation)
    sample_count: u32,
    /// Frame duration
    duration: Duration,
}

/// Ring buffer for audio frames
struct AudioRingBuffer {
    /// Circular buffer of frames
    buffer: Vec<Option<AudioFrame>>,
    /// Write position
    write_pos: AtomicU32,
    /// Read position
    read_pos: AtomicU32,
    /// Buffer capacity
    capacity: usize,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with given capacity
    fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize_with(capacity, || None);

        Self {
            buffer,
            write_pos: AtomicU32::new(0),
            read_pos: AtomicU32::new(0),
            capacity,
        }
    }

    /// Try to push a frame into the buffer
    /// Returns Ok(()) if successful, Err if buffer is full
    fn try_push(&self, frame: AudioFrame) -> Result<()> {
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let read_pos = self.read_pos.load(Ordering::Acquire);

        // Calculate next write position
        let next_write = (write_pos + 1) % self.capacity as u32;

        // Check if buffer is full (next write would overlap read)
        if next_write == read_pos {
            return Err(Error::MediaTrackError(
                "Audio ring buffer full - dropping frame".to_string(),
            ));
        }

        // SAFETY: We've verified the position is valid and won't overlap
        // The buffer is pre-allocated and positions are always in bounds
        unsafe {
            let slot = self.buffer.as_ptr().add(write_pos as usize) as *mut Option<AudioFrame>;
            *slot = Some(frame);
        }

        // Update write position
        self.write_pos.store(next_write, Ordering::Release);
        Ok(())
    }

    /// Try to pop a frame from the buffer
    /// Returns None if buffer is empty
    fn try_pop(&self) -> Option<AudioFrame> {
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let read_pos = self.read_pos.load(Ordering::Acquire);

        // Check if buffer is empty
        if read_pos == write_pos {
            return None;
        }

        // SAFETY: We've verified there's data to read
        let frame = unsafe {
            let slot = self.buffer.as_ptr().add(read_pos as usize) as *mut Option<AudioFrame>;
            (*slot).take()
        };

        // Update read position
        let next_read = (read_pos + 1) % self.capacity as u32;
        self.read_pos.store(next_read, Ordering::Release);

        frame
    }

    /// Get the number of frames currently in the buffer
    fn len(&self) -> usize {
        let write_pos = self.write_pos.load(Ordering::Acquire) as usize;
        let read_pos = self.read_pos.load(Ordering::Acquire) as usize;

        if write_pos >= read_pos {
            write_pos - read_pos
        } else {
            self.capacity - read_pos + write_pos
        }
    }
}

/// Audio sender with ring buffer and dedicated transmission thread
pub struct AudioSender {
    /// Ring buffer for audio frames
    buffer: Arc<AudioRingBuffer>,
    /// WebRTC track for transmission
    track: Arc<TrackLocalStaticSample>,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    /// Current RTP timestamp
    timestamp: Arc<AtomicU32>,
    /// Thread handle (wrapped in Mutex for Drop)
    thread_handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl AudioSender {
    /// Create a new audio sender
    ///
    /// # Arguments
    ///
    /// * `track` - WebRTC track for transmission
    /// * `buffer_capacity` - Ring buffer capacity in frames
    ///   - Recommended: 1500 frames = 30 seconds @ 20ms (allows TTS burst generation)
    ///   - Minimum: 200 frames = 4 seconds @ 20ms
    pub fn new(track: Arc<TrackLocalStaticSample>, buffer_capacity: usize) -> Self {
        let buffer = Arc::new(AudioRingBuffer::new(buffer_capacity));
        let shutdown = Arc::new(AtomicBool::new(false));
        let timestamp = Arc::new(AtomicU32::new(0));

        let sender = Self {
            buffer: Arc::clone(&buffer),
            track: Arc::clone(&track),
            shutdown: Arc::clone(&shutdown),
            timestamp: Arc::clone(&timestamp),
            thread_handle: Arc::new(Mutex::new(None)),
        };

        // Start the transmission thread
        let thread_buffer = Arc::clone(&buffer);
        let thread_track = Arc::clone(&track);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_timestamp = Arc::clone(&timestamp);

        let handle = std::thread::Builder::new()
            .name("audio-sender".to_string())
            .spawn(move || {
                Self::transmission_thread(
                    thread_buffer,
                    thread_track,
                    thread_shutdown,
                    thread_timestamp,
                )
            })
            .expect("Failed to spawn audio sender thread");

        // Store the thread handle
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            *sender_clone.thread_handle.lock().await = Some(handle);
        });

        use tracing::info;
        info!(
            "AudioSender created with buffer capacity: {} frames",
            buffer_capacity
        );
        sender
    }

    /// Transmission thread - runs on dedicated OS thread for precise timing
    fn transmission_thread(
        buffer: Arc<AudioRingBuffer>,
        track: Arc<TrackLocalStaticSample>,
        shutdown: Arc<AtomicBool>,
        timestamp: Arc<AtomicU32>,
    ) {
        use tracing::info;
        // Use println as a backup in case tracing isn't working
        println!("[AUDIO-SENDER-THREAD] Thread spawned!");
        info!("Audio transmission thread started - creating tokio runtime");

        // Create a dedicated tokio runtime for this thread
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => {
                info!("Audio transmission thread: tokio runtime created successfully");
                rt
            }
            Err(e) => {
                warn!("Failed to create tokio runtime for audio sender: {}", e);
                return;
            }
        };

        let mut frame_count = 0u64;
        let mut loop_iterations = 0u64;
        let start_time = Instant::now();

        info!(
            "Audio transmission thread: entering main loop, shutdown={}",
            shutdown.load(Ordering::Acquire)
        );

        while !shutdown.load(Ordering::Acquire) {
            loop_iterations += 1;

            // Log first few iterations for debugging
            if loop_iterations <= 3 {
                let buffer_len = buffer.len();
                info!(
                    "Audio sender: Loop iteration {}, shutdown={}, buffer_len={}",
                    loop_iterations,
                    shutdown.load(Ordering::Acquire),
                    buffer_len
                );
            }

            // Try to get a frame from the buffer
            if let Some(frame) = buffer.try_pop() {
                // Update RTP timestamp
                let old_ts = timestamp.fetch_add(frame.sample_count, Ordering::AcqRel);

                // Create WebRTC sample
                let sample = Sample {
                    data: frame.data.into(),
                    duration: frame.duration,
                    timestamp: std::time::SystemTime::now(),
                    ..Default::default()
                };

                // Send the sample using the dedicated runtime
                let track_clone = Arc::clone(&track);
                let send_result = rt.block_on(async { track_clone.write_sample(&sample).await });

                if let Err(e) = send_result {
                    warn!("Failed to send audio frame: {}", e);
                } else {
                    frame_count += 1;

                    // Log first frame and periodically
                    if frame_count == 1 {
                        info!("Audio sender: FIRST frame sent successfully!");
                    } else if frame_count % 50 == 0 {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let buffer_len = buffer.len();
                        info!(
                            "Audio sender: {} frames sent in {:.2}s, buffer: {} frames, timestamp: {}",
                            frame_count, elapsed, buffer_len, old_ts
                        );
                    }
                }

                // PACING: Sleep to match real-time playback
                // Each frame is frame.duration (typically 20ms)
                std::thread::sleep(frame.duration);
            } else {
                // Buffer is empty, sleep briefly and retry
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        info!(
            "Audio transmission thread shutting down (sent {} frames, {} loop iterations, shutdown={})",
            frame_count,
            loop_iterations,
            shutdown.load(Ordering::Acquire)
        );
    }

    /// Enqueue an audio frame for transmission
    ///
    /// # Arguments
    ///
    /// * `encoded_data` - Opus-encoded audio data
    /// * `sample_count` - Number of audio samples this frame represents
    /// * `duration` - Frame duration (typically 20ms)
    pub async fn enqueue_frame(
        &self,
        encoded_data: Vec<u8>,
        sample_count: u32,
        duration: Duration,
    ) -> Result<()> {
        let frame = AudioFrame {
            data: encoded_data,
            sample_count,
            duration,
        };

        self.buffer.try_push(frame)?;
        Ok(())
    }

    /// Get the number of frames currently buffered
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Get the current RTP timestamp
    pub fn timestamp(&self) -> u32 {
        self.timestamp.load(Ordering::Acquire)
    }

    /// Shutdown the sender and wait for thread to complete
    pub async fn shutdown(&self) -> Result<()> {
        debug!("Shutting down AudioSender");
        self.shutdown.store(true, Ordering::Release);

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.lock().await.take() {
            handle.join().map_err(|_| {
                Error::MediaTrackError("Failed to join audio sender thread".to_string())
            })?;
        }

        Ok(())
    }
}

impl Clone for AudioSender {
    fn clone(&self) -> Self {
        Self {
            buffer: Arc::clone(&self.buffer),
            track: Arc::clone(&self.track),
            shutdown: Arc::clone(&self.shutdown),
            timestamp: Arc::clone(&self.timestamp),
            thread_handle: Arc::clone(&self.thread_handle),
        }
    }
}

// NOTE: We do NOT implement Drop to set shutdown=true because AudioSender
// is cloned internally (for storing thread handle), and dropping the clone
// would kill the thread prematurely. Instead, shutdown must be called explicitly
// or the AudioTrack that owns this sender will clean up when it's dropped.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let buffer = AudioRingBuffer::new(4);

        // Push some frames
        let frame = AudioFrame {
            data: vec![1, 2, 3],
            sample_count: 480,
            duration: Duration::from_millis(20),
        };

        assert!(buffer.try_push(frame.clone()).is_ok());
        assert_eq!(buffer.len(), 1);

        // Pop frame
        let popped = buffer.try_pop();
        assert!(popped.is_some());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_ring_buffer_full() {
        let buffer = AudioRingBuffer::new(3);

        let frame = AudioFrame {
            data: vec![1, 2, 3],
            sample_count: 480,
            duration: Duration::from_millis(20),
        };

        // Fill buffer (capacity - 1 due to ring buffer semantics)
        assert!(buffer.try_push(frame.clone()).is_ok());
        assert!(buffer.try_push(frame.clone()).is_ok());

        // Should be full now
        assert!(buffer.try_push(frame.clone()).is_err());
    }

    #[test]
    fn test_ring_buffer_wraparound() {
        let buffer = AudioRingBuffer::new(4);

        let frame = AudioFrame {
            data: vec![1, 2, 3],
            sample_count: 480,
            duration: Duration::from_millis(20),
        };

        // Push and pop multiple times to test wraparound
        for _ in 0..10 {
            assert!(buffer.try_push(frame.clone()).is_ok());
            assert!(buffer.try_pop().is_some());
        }
    }
}
