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
use parking_lot::RwLock as SyncRwLock;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::transport::session_control::SessionControl;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, warn};
use webrtc::media::Sample;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

/// Late-bound clock-tap configuration for the avatar pipeline
/// (spec 2026-04-27 §3.6).
///
/// When a `ClockTap` is attached to an `AudioSender` (via
/// [`AudioSender::set_clock_tap`]), the transmission thread emits one
/// `RuntimeData::Json {kind: "audio_clock", pts_ms, stream_id?}` envelope
/// to `<node_id>.out.clock` per dequeued frame. The renderer subscribes
/// to this tap to know what `pts_ms` the listener is currently hearing.
///
/// Attaching is opt-in and late-bound on purpose: `AudioSender` is
/// constructed inside `AudioTrack::new`, which doesn't currently hold a
/// `SessionControl` reference. Whoever does (the renderer's session
/// bootstrap, or the manifest config glue) attaches via the setter.
#[derive(Clone)]
pub struct ClockTap {
    /// Session bus the publish goes through.
    pub control: Arc<SessionControl>,
    /// `node_id` segment of the tap address; the renderer subscribes
    /// to `node_out(node_id).with_port("clock")`. Conventionally
    /// `"audio"` per spec §3.6 example.
    pub node_id: String,
    /// Optional `stream_id` echoed in the envelope so multi-track
    /// renderers (one peer, multiple avatars) can disambiguate which
    /// track this clock belongs to. Spec §6.4.
    pub stream_id: Option<String>,
}

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
    /// Wakes any task awaiting space in the buffer. Notified on every
    /// successful pop (one slot freed) and on `clear()` (all slots
    /// freed at once). Producers use it to implement async
    /// backpressure rather than dropping frames on overflow.
    space_available: Notify,
    /// Shutdown signal. Producers awaiting space check this on each
    /// retry so they bail out instead of hanging when the sender is
    /// being torn down.
    shutdown: AtomicBool,
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
            space_available: Notify::new(),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Async-backpressuring push.
    ///
    /// If the buffer has room, push immediately. If it's full, wait
    /// for the consumer thread to free a slot (or for `clear()` to
    /// drain on barge-in) and retry. This is the path producers
    /// should use; the previous "drop frame on full" behaviour
    /// caused audible glitches under long TTS replies.
    ///
    /// Returns `Err` only on shutdown — the buffer being full is
    /// not an error here, just a wait condition.
    async fn push(&self, frame: AudioFrame) -> Result<()> {
        let mut frame = Some(frame);
        loop {
            if self.shutdown.load(Ordering::Acquire) {
                return Err(Error::MediaTrackError(
                    "audio sender shutdown — refusing to enqueue frame".to_string(),
                ));
            }
            // Register interest BEFORE the try_push attempt. If the
            // consumer drains a slot between try_push() and notified()
            // the permit is held by the Notify and the next
            // notified().await returns immediately — no missed wakeups.
            let waiter = self.space_available.notified();
            tokio::pin!(waiter);
            // Enable the waiter so it captures any notifications that
            // arrive between this point and the next .await.
            waiter.as_mut().enable();

            match self.try_push_internal(frame.take().expect("frame consumed only on success")) {
                Ok(()) => return Ok(()),
                Err(rejected) => {
                    frame = Some(rejected);
                    waiter.await;
                }
            }
        }
    }

    /// Try to push a frame into the buffer (non-blocking).
    /// Returns Ok(()) if successful, Err if buffer is full.
    ///
    /// Kept on the public surface for callers that genuinely want
    /// drop-on-full semantics (currently none in the production path,
    /// but we leave it because tests + the existing API contract
    /// reference it).
    fn try_push(&self, frame: AudioFrame) -> Result<()> {
        self.try_push_internal(frame).map_err(|_| {
            Error::MediaTrackError("Audio ring buffer full - dropping frame".to_string())
        })
    }

    /// Internal try-push that hands the frame back on rejection so
    /// the async `push` can retry without cloning. `Err(frame)` =
    /// "buffer was full, here's your frame back."
    fn try_push_internal(&self, frame: AudioFrame) -> std::result::Result<(), AudioFrame> {
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let read_pos = self.read_pos.load(Ordering::Acquire);

        let next_write = (write_pos + 1) % self.capacity as u32;
        if next_write == read_pos {
            return Err(frame);
        }

        // SAFETY: position is in bounds and won't overlap the reader.
        unsafe {
            let slot = self.buffer.as_ptr().add(write_pos as usize) as *mut Option<AudioFrame>;
            *slot = Some(frame);
        }
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

        // Wake any producer parked on `push().await` waiting for
        // space. Notify is `Send + Sync` and `notify_one` is callable
        // from a non-tokio thread, so this is safe from the std::thread
        // transmission loop. If no one is waiting, the permit is
        // queued and the next `push()` consumes it without waiting.
        self.space_available.notify_one();

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

    /// Drain the ring buffer without stopping the sender thread.
    ///
    /// Called on barge-in: the user started speaking while the
    /// assistant was still playing queued TTS audio. Advancing
    /// `read_pos` to `write_pos` discards all queued frames so the
    /// next frames the thread pops are the fresh ones. The `Option`
    /// slots are left as-is — the thread's `try_pop` tolerates both
    /// None and stale Some readings (it checks read_pos==write_pos
    /// first, and any stale Some is overwritten on the next push).
    fn clear(&self) -> usize {
        let write_pos = self.write_pos.load(Ordering::Acquire);
        let old_read_pos = self.read_pos.swap(write_pos, Ordering::AcqRel);
        let dropped = if write_pos >= old_read_pos {
            (write_pos - old_read_pos) as usize
        } else {
            self.capacity - old_read_pos as usize + write_pos as usize
        };
        // Proactively clear the Option slots we just skipped over so
        // we don't hold onto AudioFrame allocations until the next
        // wrap-around.
        let mut cursor = old_read_pos as usize;
        while cursor != write_pos as usize {
            // SAFETY: same reasoning as try_pop — positions are in
            // bounds and we just claimed this range by moving read_pos.
            unsafe {
                let slot =
                    self.buffer.as_ptr().add(cursor) as *mut Option<AudioFrame>;
                *slot = None;
            }
            cursor = (cursor + 1) % self.capacity;
        }
        // Wake every parked producer — clearing freed every slot at
        // once. A single `notify_one` would only wake one of them and
        // leave the rest sleeping despite the buffer being empty.
        self.space_available.notify_waiters();
        dropped
    }

    /// Mark the ring buffer as shutting down and wake every parked
    /// producer so they observe the flag and return promptly.
    fn signal_shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.space_available.notify_waiters();
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
    /// Optional avatar clock tap (spec 2026-04-27 §3.6). Read every
    /// dequeue by the transmission thread; written by external
    /// callers via `set_clock_tap`. Sync `parking_lot::RwLock` because
    /// the read happens on the transmission OS thread (no tokio
    /// runtime in scope) and is on the hot path. Uncontended reads
    /// are a single atomic.
    clock_tap: Arc<SyncRwLock<Option<ClockTap>>>,
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
        let clock_tap: Arc<SyncRwLock<Option<ClockTap>>> = Arc::new(SyncRwLock::new(None));

        let sender = Self {
            buffer: Arc::clone(&buffer),
            track: Arc::clone(&track),
            shutdown: Arc::clone(&shutdown),
            timestamp: Arc::clone(&timestamp),
            thread_handle: Arc::new(Mutex::new(None)),
            clock_tap: Arc::clone(&clock_tap),
        };

        // Start the transmission thread
        let thread_buffer = Arc::clone(&buffer);
        let thread_track = Arc::clone(&track);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_timestamp = Arc::clone(&timestamp);
        let thread_clock_tap = Arc::clone(&clock_tap);

        let handle = std::thread::Builder::new()
            .name("audio-sender".to_string())
            .spawn(move || {
                Self::transmission_thread(
                    thread_buffer,
                    thread_track,
                    thread_shutdown,
                    thread_timestamp,
                    thread_clock_tap,
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
        clock_tap: Arc<SyncRwLock<Option<ClockTap>>>,
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

        // Cumulative duration of frames the listener has heard. This is
        // a wall-of-played-audio clock — it never resets on barge-in
        // (spec §3.6: barge clears the ring; the renderer's stale-pts
        // eviction handles the rest).
        let mut cum_played_ms: u64 = 0;

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

                // Spec 2026-04-27 §3.6: publish the avatar audio clock
                // tap on each dequeued frame. We emit *after dequeue,
                // before write_sample / pacing sleep* — what the
                // renderer cares about is "this is the frame the
                // listener is about to start hearing", and tying the
                // publish to dequeue (not to write_sample success)
                // means a transiently-misconfigured peer connection
                // doesn't silently break the avatar's mouth sync.
                cum_played_ms = cum_played_ms.saturating_add(frame.duration.as_millis() as u64);
                if let Some(tap) = clock_tap.read().clone() {
                    let mut envelope = serde_json::json!({
                        "kind": "audio_clock",
                        "pts_ms": cum_played_ms,
                    });
                    if let Some(stream_id) = &tap.stream_id {
                        envelope["stream_id"] = serde_json::Value::String(stream_id.clone());
                    }
                    tap.control.publish_tap(
                        &tap.node_id,
                        Some("clock"),
                        RuntimeData::Json(envelope),
                    );
                }

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

    /// Enqueue an audio frame for transmission.
    ///
    /// Applies async backpressure: if the ring buffer is full, this
    /// awaits a slot opening up rather than dropping the frame. The
    /// wait cascades naturally through the calling drain task →
    /// session output channel → fan task → TTS callback, eventually
    /// stalling the producer instead of dropping audio.
    ///
    /// Returns `Err` only if the sender has been shut down — a full
    /// buffer is no longer an error condition.
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
        self.buffer.push(frame).await
    }

    /// Attach (or replace) the avatar clock tap. Spec 2026-04-27 §3.6.
    ///
    /// Late-binding; safe to call any time after `new()`. The
    /// transmission thread picks up the change on the next frame.
    pub fn set_clock_tap(&self, tap: ClockTap) {
        *self.clock_tap.write() = Some(tap);
    }

    /// Detach the clock tap so subsequent dequeued frames don't publish.
    pub fn clear_clock_tap(&self) {
        *self.clock_tap.write() = None;
    }

    /// Get the number of frames currently buffered
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Drain the ring buffer without stopping the sender thread.
    ///
    /// Used on barge-in: we want the assistant to stop speaking
    /// immediately, not after the already-queued ~10 s of TTS audio
    /// finishes playing. The sender thread keeps running, it just
    /// finds the buffer empty and waits for the next push.
    pub fn flush_buffer(&self) -> usize {
        self.buffer.clear()
    }

    /// Get the current RTP timestamp
    pub fn timestamp(&self) -> u32 {
        self.timestamp.load(Ordering::Acquire)
    }

    /// Shutdown the sender and wait for thread to complete
    pub async fn shutdown(&self) -> Result<()> {
        debug!("Shutting down AudioSender");
        self.shutdown.store(true, Ordering::Release);
        // Also flip the buffer's shutdown flag and wake every parked
        // producer; otherwise a producer awaiting space hangs forever
        // because the transmission thread has stopped popping.
        self.buffer.signal_shutdown();

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
            clock_tap: Arc::clone(&self.clock_tap),
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
