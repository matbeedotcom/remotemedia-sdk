//! MPEG-TS demuxer for SRT ingest
//!
//! This module provides MPEG-TS demuxing and audio decoding for data
//! received via SRT. It uses ffmpeg-sys-next for raw FFmpeg FFI bindings
//! with custom AVIO for streaming input.

use std::collections::VecDeque;
use std::ffi::CString;
use std::ptr;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use tokio::sync::mpsc;

/// Decoded audio frame
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub timestamp_us: u64,
}

/// Video timing information (no pixel data - just PTS for A/V sync)
#[derive(Debug, Clone)]
pub struct VideoTiming {
    /// Presentation timestamp in microseconds
    pub timestamp_us: u64,
    /// Frame duration in microseconds (based on framerate)
    pub duration_us: u64,
}

/// Thread-safe ring buffer for feeding data to FFmpeg
struct RingBuffer {
    data: Mutex<RingBufferInner>,
    condvar: Condvar,
}

struct RingBufferInner {
    buffer: VecDeque<u8>,
    eof: bool,
    read_position: u64,
}

impl RingBuffer {
    fn new() -> Self {
        Self {
            data: Mutex::new(RingBufferInner {
                buffer: VecDeque::with_capacity(188 * 1000), // ~1000 MPEG-TS packets
                eof: false,
                read_position: 0,
            }),
            condvar: Condvar::new(),
        }
    }

    /// Write data to the ring buffer
    fn write(&self, data: &[u8]) {
        let mut inner = self.data.lock().unwrap();
        inner.buffer.extend(data);
        self.condvar.notify_one();
    }

    /// Signal EOF to the reader
    fn set_eof(&self) {
        let mut inner = self.data.lock().unwrap();
        inner.eof = true;
        self.condvar.notify_one();
    }

    /// Read data from the ring buffer (blocking until data available or EOF)
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut inner = self.data.lock().unwrap();

        // Wait for data or EOF
        while inner.buffer.is_empty() && !inner.eof {
            inner = self.condvar.wait(inner).unwrap();
        }

        if inner.buffer.is_empty() && inner.eof {
            return 0; // EOF
        }

        let to_read = buf.len().min(inner.buffer.len());
        for (i, byte) in inner.buffer.drain(..to_read).enumerate() {
            buf[i] = byte;
        }
        inner.read_position += to_read as u64;

        to_read
    }
}

/// Commands sent to the worker thread
enum WorkerCommand {
    /// Shutdown the worker
    Shutdown,
}

/// Responses from the worker thread
enum WorkerResponse {
    /// Successfully decoded audio samples
    Audio(DecodedAudio),
    /// Video timing info (for A/V sync - no pixel data)
    Video(VideoTiming),
    /// Stream ended
    EndOfStream,
    /// Error occurred
    Error(String),
}

/// MPEG-TS demuxer that reads from a byte buffer
pub struct MpegTsDemuxer {
    /// Channel to receive MPEG-TS bytes
    input_rx: mpsc::Receiver<Vec<u8>>,

    /// Channel to send decoded audio
    audio_tx: mpsc::Sender<DecodedAudio>,

    /// Channel to send video timing (optional - for A/V sync)
    video_tx: Option<mpsc::Sender<VideoTiming>>,

    /// Target sample rate for output
    pub target_sample_rate: u32,

    /// Target channels for output
    pub target_channels: u32,

    /// Session ID for logging
    session_id: String,
}

impl MpegTsDemuxer {
    /// Create a new demuxer (audio only, for backwards compatibility)
    pub fn new(
        input_rx: mpsc::Receiver<Vec<u8>>,
        audio_tx: mpsc::Sender<DecodedAudio>,
        target_sample_rate: u32,
        target_channels: u32,
        session_id: String,
    ) -> Self {
        Self {
            input_rx,
            audio_tx,
            video_tx: None,
            target_sample_rate,
            target_channels,
            session_id,
        }
    }

    /// Create a new demuxer with video timing output for A/V sync
    pub fn with_video(
        input_rx: mpsc::Receiver<Vec<u8>>,
        audio_tx: mpsc::Sender<DecodedAudio>,
        video_tx: mpsc::Sender<VideoTiming>,
        target_sample_rate: u32,
        target_channels: u32,
        session_id: String,
    ) -> Self {
        Self {
            input_rx,
            audio_tx,
            video_tx: Some(video_tx),
            target_sample_rate,
            target_channels,
            session_id,
        }
    }

    /// Run the demuxer, processing incoming MPEG-TS data
    pub async fn run(mut self) {
        tracing::info!(
            session_id = %self.session_id,
            "Starting MPEG-TS demuxer (target: {}Hz {}ch)",
            self.target_sample_rate,
            self.target_channels
        );

        // Create shared ring buffer
        let ring_buffer = Arc::new(RingBuffer::new());
        let ring_buffer_writer = Arc::clone(&ring_buffer);

        // Create channels for worker communication
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCommand>();
        let (resp_tx, resp_rx) = std::sync::mpsc::channel::<WorkerResponse>();

        // Spawn worker thread for FFmpeg processing
        let session_id = self.session_id.clone();

        let worker_handle = thread::spawn(move || {
            worker_thread_main(ring_buffer, cmd_rx, resp_tx, session_id);
        });

        // Feed data to ring buffer and forward decoded audio
        let mut packet_count: u64 = 0;
        let mut frame_count: u64 = 0;

        loop {
            tokio::select! {
                // Receive MPEG-TS data from SRT
                data = self.input_rx.recv() => {
                    match data {
                        Some(bytes) => {
                            packet_count += 1;
                            ring_buffer_writer.write(&bytes);
                        }
                        None => {
                            // Input channel closed, signal EOF
                            ring_buffer_writer.set_eof();
                            break;
                        }
                    }
                }

                // Check for decoded frames (non-blocking via try_recv)
                _ = tokio::task::yield_now() => {
                    while let Ok(resp) = resp_rx.try_recv() {
                        match resp {
                            WorkerResponse::Audio(audio) => {
                                frame_count += 1;
                                if self.audio_tx.send(audio).await.is_err() {
                                    tracing::warn!(
                                        session_id = %self.session_id,
                                        "Audio output channel closed"
                                    );
                                    let _ = cmd_tx.send(WorkerCommand::Shutdown);
                                    break;
                                }
                            }
                            WorkerResponse::Video(video) => {
                                if let Some(ref video_tx) = self.video_tx {
                                    if video_tx.send(video).await.is_err() {
                                        tracing::debug!(
                                            session_id = %self.session_id,
                                            "Video output channel closed"
                                        );
                                    }
                                }
                            }
                            WorkerResponse::EndOfStream => {
                                tracing::debug!(
                                    session_id = %self.session_id,
                                    "Demuxer reached end of stream"
                                );
                            }
                            WorkerResponse::Error(e) => {
                                tracing::warn!(
                                    session_id = %self.session_id,
                                    error = %e,
                                    "Demuxer error"
                                );
                            }
                        }
                    }
                }
            }
        }

        // Signal shutdown and wait for worker
        let _ = cmd_tx.send(WorkerCommand::Shutdown);

        // Drain remaining frames
        while let Ok(resp) = resp_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            match resp {
                WorkerResponse::Audio(audio) => {
                    frame_count += 1;
                    let _ = self.audio_tx.send(audio).await;
                }
                WorkerResponse::Video(video) => {
                    if let Some(ref video_tx) = self.video_tx {
                        let _ = video_tx.send(video).await;
                    }
                }
                _ => {}
            }
        }

        let _ = worker_handle.join();

        tracing::info!(
            session_id = %self.session_id,
            packets = packet_count,
            frames = frame_count,
            "MPEG-TS demuxer stopped"
        );
    }
}

// ============================================================================
// Worker thread implementation using raw ffmpeg-sys-next FFI
// ============================================================================

use ffmpeg_next::ffi;

/// Custom read callback for AVIO
/// Returns number of bytes read, or AVERROR_EOF on end of stream
unsafe extern "C" fn read_packet(
    opaque: *mut std::ffi::c_void,
    buf: *mut u8,
    buf_size: i32,
) -> i32 {
    let ring_buffer = &*(opaque as *const RingBuffer);
    let slice = std::slice::from_raw_parts_mut(buf, buf_size as usize);
    let bytes_read = ring_buffer.read(slice);

    // Log read activity at trace level to avoid log spam
    static READ_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = READ_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if count < 10 || count % 100 == 0 {
        tracing::trace!(bytes_read = bytes_read, requested = buf_size, "AVIO read_packet");
    }

    if bytes_read == 0 {
        tracing::debug!("AVIO read_packet returning EOF");
        ffi::AVERROR_EOF
    } else {
        bytes_read as i32
    }
}

/// Main function for the worker thread
fn worker_thread_main(
    ring_buffer: Arc<RingBuffer>,
    cmd_rx: std::sync::mpsc::Receiver<WorkerCommand>,
    resp_tx: std::sync::mpsc::Sender<WorkerResponse>,
    session_id: String,
) {
    tracing::info!(session_id = %session_id, "FFmpeg worker thread starting");

    unsafe {
        // Initialize ffmpeg (safe to call multiple times)
        tracing::debug!(session_id = %session_id, "Initializing FFmpeg");
        ffmpeg_next::init().unwrap();
        tracing::debug!(session_id = %session_id, "FFmpeg initialized");

        // Allocate AVIO buffer (FFmpeg takes ownership)
        const AVIO_BUFFER_SIZE: usize = 188 * 7; // 7 MPEG-TS packets
        let avio_buffer = ffi::av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
        if avio_buffer.is_null() {
            let _ = resp_tx.send(WorkerResponse::Error("Failed to allocate AVIO buffer".to_string()));
            return;
        }

        // Create custom AVIO context
        let ring_buffer_ptr = Arc::into_raw(ring_buffer);
        let avio_ctx = ffi::avio_alloc_context(
            avio_buffer,
            AVIO_BUFFER_SIZE as i32,
            0, // read-only
            ring_buffer_ptr as *mut std::ffi::c_void,
            Some(read_packet),
            None, // no write
            None, // no seek
        );

        if avio_ctx.is_null() {
            ffi::av_free(avio_buffer as *mut std::ffi::c_void);
            let _ = Arc::from_raw(ring_buffer_ptr); // Reclaim ownership
            let _ = resp_tx.send(WorkerResponse::Error("Failed to create AVIO context".to_string()));
            return;
        }

        // Allocate format context
        let format_ctx = ffi::avformat_alloc_context();
        if format_ctx.is_null() {
            ffi::avio_context_free(&mut (avio_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error("Failed to allocate format context".to_string()));
            return;
        }

        // Set custom IO
        (*format_ctx).pb = avio_ctx;
        (*format_ctx).flags |= ffi::AVFMT_FLAG_CUSTOM_IO as i32;

        // Probe settings for MPEG-TS with AAC audio
        // AAC in MPEG-TS uses ADTS headers which need more probing
        // Use 256KB probesize and 1 second analyze duration
        (*format_ctx).probesize = 256 * 1024;
        (*format_ctx).max_analyze_duration = 1_000_000; // 1 second in microseconds

        // Open input with MPEG-TS format hint
        let format_name = CString::new("mpegts").unwrap();
        let input_format = ffi::av_find_input_format(format_name.as_ptr());

        tracing::debug!(session_id = %session_id, "Opening input with MPEG-TS format hint");

        let ret = ffi::avformat_open_input(
            &mut (format_ctx as *mut _),
            ptr::null(),
            input_format,
            ptr::null_mut(),
        );

        if ret < 0 {
            let err_str = ffmpeg_error_string(ret);
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error(format!("Failed to open input: {}", err_str)));
            return;
        }

        tracing::debug!(session_id = %session_id, "Input opened, finding stream info...");

        // Find stream info
        let ret = ffi::avformat_find_stream_info(format_ctx, ptr::null_mut());
        if ret < 0 {
            let err_str = ffmpeg_error_string(ret);
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error(format!("Failed to find stream info: {}", err_str)));
            return;
        }

        tracing::debug!(session_id = %session_id, "Stream info found");

        // Find best audio stream
        let mut audio_stream_index = ffi::av_find_best_stream(
            format_ctx,
            ffi::AVMediaType::AVMEDIA_TYPE_AUDIO,
            -1,
            -1,
            ptr::null_mut(),
            0,
        );

        // If av_find_best_stream fails, manually search for audio stream
        // This can happen with AAC streams where codec parameters aren't fully probed
        if audio_stream_index < 0 {
            tracing::debug!(
                session_id = %session_id,
                "av_find_best_stream failed, searching manually for audio stream"
            );

            let nb_streams = (*format_ctx).nb_streams as usize;
            for i in 0..nb_streams {
                let stream = *(*format_ctx).streams.add(i);
                let codec_type = (*(*stream).codecpar).codec_type;
                if codec_type == ffi::AVMediaType::AVMEDIA_TYPE_AUDIO {
                    audio_stream_index = i as i32;
                    tracing::debug!(
                        session_id = %session_id,
                        stream_index = i,
                        "Found audio stream manually"
                    );
                    break;
                }
            }
        }

        if audio_stream_index < 0 {
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error("No audio stream found".to_string()));
            return;
        }

        // Find video stream (optional - for A/V sync)
        let mut video_stream_index = ffi::av_find_best_stream(
            format_ctx,
            ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            ptr::null_mut(),
            0,
        );

        // Try manual search if needed
        if video_stream_index < 0 {
            let nb_streams = (*format_ctx).nb_streams as usize;
            for i in 0..nb_streams {
                let stream = *(*format_ctx).streams.add(i);
                let codec_type = (*(*stream).codecpar).codec_type;
                if codec_type == ffi::AVMediaType::AVMEDIA_TYPE_VIDEO {
                    video_stream_index = i as i32;
                    break;
                }
            }
        }

        // Get video time base for timestamp conversion (if video found)
        let video_time_base = if video_stream_index >= 0 {
            let video_stream = *(*format_ctx).streams.offset(video_stream_index as isize);
            let tb = (*video_stream).time_base;
            tracing::info!(
                session_id = %session_id,
                video_stream_index = video_stream_index,
                time_base = ?format!("{}/{}", tb.num, tb.den),
                "Video stream found for A/V sync"
            );
            Some((tb.num as i64, tb.den as i64))
        } else {
            tracing::debug!(session_id = %session_id, "No video stream found (audio-only mode)");
            None
        };

        let audio_stream = *(*format_ctx).streams.offset(audio_stream_index as isize);
        let codec_params = (*audio_stream).codecpar;

        // Get channel count using new ch_layout API (FFmpeg 5.1+)
        // Use defaults if not available (will be updated from first decoded frame)
        let mut channels = (*codec_params).ch_layout.nb_channels as u32;
        let mut sample_rate = (*codec_params).sample_rate as u32;

        // If channels or sample_rate is 0, use defaults (will be corrected from decoder)
        if channels == 0 {
            tracing::debug!(session_id = %session_id, "Channel count unknown, defaulting to 2");
            channels = 2;
        }
        if sample_rate == 0 {
            tracing::debug!(session_id = %session_id, "Sample rate unknown, defaulting to 48000");
            sample_rate = 48000;
        }

        tracing::info!(
            session_id = %session_id,
            sample_rate = sample_rate,
            channels = channels,
            codec_id = ?(*codec_params).codec_id,
            "Audio stream found"
        );

        // Find decoder
        let codec = ffi::avcodec_find_decoder((*codec_params).codec_id);
        if codec.is_null() {
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error("No decoder found for audio codec".to_string()));
            return;
        }

        // Allocate codec context
        let codec_ctx = ffi::avcodec_alloc_context3(codec);
        if codec_ctx.is_null() {
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error("Failed to allocate codec context".to_string()));
            return;
        }

        // Copy codec parameters
        let ret = ffi::avcodec_parameters_to_context(codec_ctx, codec_params);
        if ret < 0 {
            let err_str = ffmpeg_error_string(ret);
            ffi::avcodec_free_context(&mut (codec_ctx as *mut _));
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error(format!("Failed to copy codec params: {}", err_str)));
            return;
        }

        // Open codec
        let ret = ffi::avcodec_open2(codec_ctx, codec, ptr::null_mut());
        if ret < 0 {
            let err_str = ffmpeg_error_string(ret);
            ffi::avcodec_free_context(&mut (codec_ctx as *mut _));
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error(format!("Failed to open codec: {}", err_str)));
            return;
        }

        tracing::info!(
            session_id = %session_id,
            sample_rate = sample_rate,
            channels = channels,
            "FFmpeg initialized for MPEG-TS demuxing"
        );

        // Allocate packet and frame
        let packet = ffi::av_packet_alloc();
        let frame = ffi::av_frame_alloc();

        if packet.is_null() || frame.is_null() {
            if !packet.is_null() {
                ffi::av_packet_free(&mut (packet as *mut _));
            }
            if !frame.is_null() {
                ffi::av_frame_free(&mut (frame as *mut _));
            }
            ffi::avcodec_free_context(&mut (codec_ctx as *mut _));
            ffi::avformat_close_input(&mut (format_ctx as *mut _));
            let _ = Arc::from_raw(ring_buffer_ptr);
            let _ = resp_tx.send(WorkerResponse::Error("Failed to allocate packet/frame".to_string()));
            return;
        }

        // Process packets
        let mut audio_timestamp_us: u64 = 0;
        let mut audio_packets_processed: u64 = 0;
        let mut audio_frames_decoded: u64 = 0;
        let mut video_packets_processed: u64 = 0;
        let mut read_errors: u64 = 0;

        tracing::debug!(session_id = %session_id, "Starting packet processing loop");

        loop {
            // Check for shutdown command (non-blocking)
            if let Ok(WorkerCommand::Shutdown) = cmd_rx.try_recv() {
                tracing::debug!(session_id = %session_id, "Shutdown command received");
                break;
            }

            // Read packet
            let ret = ffi::av_read_frame(format_ctx, packet);
            if ret < 0 {
                if ret == ffi::AVERROR_EOF {
                    tracing::debug!(session_id = %session_id, "EOF reached");
                    break;
                }
                read_errors += 1;
                if read_errors % 100 == 1 {
                    tracing::trace!(session_id = %session_id, error = ret, "av_read_frame error");
                }
                // Other errors - might be temporary, continue
                continue;
            }

            let stream_index = (*packet).stream_index;

            // Handle video packets (just extract PTS, no decoding)
            if video_stream_index >= 0 && stream_index == video_stream_index {
                video_packets_processed += 1;

                // Extract PTS and convert to microseconds
                let pts = (*packet).pts;
                let duration = (*packet).duration;

                if pts != ffi::AV_NOPTS_VALUE && video_time_base.is_some() {
                    let (num, den) = video_time_base.unwrap();
                    // Convert PTS from stream time base to microseconds
                    // pts * (num / den) * 1_000_000
                    let timestamp_us = if den != 0 {
                        ((pts as i128 * num as i128 * 1_000_000) / den as i128) as u64
                    } else {
                        0
                    };
                    let duration_us = if den != 0 && duration != ffi::AV_NOPTS_VALUE {
                        ((duration as i128 * num as i128 * 1_000_000) / den as i128) as u64
                    } else {
                        // Default to ~33ms (30fps) if unknown
                        33333
                    };

                    let video_timing = VideoTiming {
                        timestamp_us,
                        duration_us,
                    };

                    // Log first video packet
                    if video_packets_processed == 1 {
                        tracing::info!(
                            session_id = %session_id,
                            pts = pts,
                            timestamp_us = timestamp_us,
                            duration_us = duration_us,
                            "First video packet PTS extracted"
                        );
                    }

                    let _ = resp_tx.send(WorkerResponse::Video(video_timing));
                }

                ffi::av_packet_unref(packet);
                continue;
            }

            // Only process audio packets
            if stream_index != audio_stream_index {
                ffi::av_packet_unref(packet);
                continue;
            }

            audio_packets_processed += 1;

            // Send packet to decoder
            let ret = ffi::avcodec_send_packet(codec_ctx, packet);
            ffi::av_packet_unref(packet);

            if ret < 0 {
                tracing::trace!(session_id = %session_id, error = ret, "Send packet error");
                continue;
            }

            // Receive decoded frames
            loop {
                let ret = ffi::avcodec_receive_frame(codec_ctx, frame);
                if ret < 0 {
                    break; // Need more data or error
                }

                let nb_samples = (*frame).nb_samples as usize;
                let frame_channels = (*frame).ch_layout.nb_channels as usize;

                if nb_samples == 0 {
                    continue;
                }

                audio_frames_decoded += 1;

                // Log first frame details
                if audio_frames_decoded == 1 {
                    tracing::info!(
                        session_id = %session_id,
                        nb_samples = nb_samples,
                        frame_channels = frame_channels,
                        format = (*frame).format,
                        "First audio frame decoded"
                    );
                }

                // Convert to f32 samples
                let samples = extract_audio_samples_raw(frame, nb_samples, frame_channels);

                if samples.is_empty() {
                    tracing::warn!(
                        session_id = %session_id,
                        format = (*frame).format,
                        "Failed to extract samples (unsupported format)"
                    );
                    continue;
                }

                // Calculate timestamp
                let chunk_duration_us = (samples.len() as u64 * 1_000_000)
                    / (sample_rate as u64 * frame_channels.max(1) as u64);

                let timestamp_us = audio_timestamp_us;
                audio_timestamp_us += chunk_duration_us;

                let audio = DecodedAudio {
                    samples,
                    sample_rate,
                    channels: frame_channels as u32,
                    timestamp_us,
                };

                if resp_tx.send(WorkerResponse::Audio(audio)).is_err() {
                    tracing::debug!(session_id = %session_id, "Response channel closed");
                    break;
                }

                ffi::av_frame_unref(frame);
            }

            // Log progress periodically
            if audio_packets_processed % 100 == 0 {
                tracing::debug!(
                    session_id = %session_id,
                    packets = audio_packets_processed,
                    frames = audio_frames_decoded,
                    "Processing progress"
                );
            }
        }

        tracing::info!(
            session_id = %session_id,
            audio_packets = audio_packets_processed,
            audio_frames = audio_frames_decoded,
            video_packets = video_packets_processed,
            "Packet processing loop ended"
        );

        // Flush decoder
        let _ = ffi::avcodec_send_packet(codec_ctx, ptr::null());
        loop {
            let ret = ffi::avcodec_receive_frame(codec_ctx, frame);
            if ret < 0 {
                break;
            }

            let nb_samples = (*frame).nb_samples as usize;
            let frame_channels = (*frame).ch_layout.nb_channels as usize;

            if nb_samples > 0 {
                let samples = extract_audio_samples_raw(frame, nb_samples, frame_channels);
                if !samples.is_empty() {
                    let chunk_duration_us = (samples.len() as u64 * 1_000_000)
                        / (sample_rate as u64 * frame_channels.max(1) as u64);
                    let timestamp_us = audio_timestamp_us;
                    audio_timestamp_us += chunk_duration_us;

                    let audio = DecodedAudio {
                        samples,
                        sample_rate,
                        channels: frame_channels as u32,
                        timestamp_us,
                    };

                    let _ = resp_tx.send(WorkerResponse::Audio(audio));
                }
            }
            ffi::av_frame_unref(frame);
        }

        // Cleanup
        ffi::av_packet_free(&mut (packet as *mut _));
        ffi::av_frame_free(&mut (frame as *mut _));
        ffi::avcodec_free_context(&mut (codec_ctx as *mut _));

        // Note: avformat_close_input frees the AVIO context and buffer when AVFMT_FLAG_CUSTOM_IO is set
        ffi::avformat_close_input(&mut (format_ctx as *mut _));

        // Reclaim ring buffer Arc
        let _ = Arc::from_raw(ring_buffer_ptr);

        let _ = resp_tx.send(WorkerResponse::EndOfStream);
        tracing::debug!(session_id = %session_id, "FFmpeg worker thread stopped");
    }
}

// Use shared ffmpeg_error_string from ingest-rtmp
use remotemedia_ingest_rtmp::ffmpeg_error_string;

/// Extract audio samples from a decoded frame as f32
///
/// Uses the shared audio_samples module from remotemedia-ingest-rtmp
/// for consistent audio format conversion across all demuxers.
unsafe fn extract_audio_samples_raw(
    frame: *mut ffi::AVFrame,
    nb_samples: usize,
    channels: usize,
) -> Vec<f32> {
    use remotemedia_ingest_rtmp::audio_samples::{
        sample_formats, convert_packed_samples_to_f32, convert_planar_samples_to_f32,
    };

    let format = (*frame).format;

    // Check if planar format using our shared constants
    if sample_formats::is_planar(format) {
        // Collect plane pointers for each channel
        let plane_data: Vec<*const u8> = (0..channels)
            .map(|ch| (*frame).data[ch] as *const u8)
            .collect();
        
        convert_planar_samples_to_f32(&plane_data, format, nb_samples)
    } else {
        // Packed format - all data in first plane
        let data = (*frame).data[0] as *const u8;
        let total_samples = nb_samples * channels;
        convert_packed_samples_to_f32(data, format, total_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_demuxer_creation() {
        let (_input_tx, input_rx) = mpsc::channel(10);
        let (output_tx, _output_rx) = mpsc::channel(10);

        let demuxer = MpegTsDemuxer::new(
            input_rx,
            output_tx,
            16000,
            1,
            "test_session".to_string(),
        );

        // Just verify we can create the demuxer
        assert_eq!(demuxer.target_sample_rate, 16000);
        assert_eq!(demuxer.target_channels, 1);
    }

    #[test]
    fn test_ring_buffer() {
        let buffer = RingBuffer::new();

        // Write some data
        buffer.write(&[1, 2, 3, 4, 5]);

        // Read it back using the reader
        let reader = RingBufferReader {
            buffer: Arc::new(RingBuffer::new()),
        };

        // Just test the buffer internals
        let inner = buffer.data.lock().unwrap();
        assert_eq!(inner.buffer.len(), 5);
    }
}

/// Wrapper to make RingBuffer implement Read for testing
struct RingBufferReader {
    buffer: Arc<RingBuffer>,
}
