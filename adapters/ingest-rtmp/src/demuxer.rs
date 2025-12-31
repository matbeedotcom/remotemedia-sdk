//! RTMP/RTSP demuxer implementation using direct FFmpeg FFI
//!
//! This module provides RTMP, RTMPS, and RTSP stream demuxing and decoding
//! using FFmpeg's native network protocol support via direct FFI calls.
//!
//! # Supported Protocols
//!
//! - `rtmp://` - Real-Time Messaging Protocol
//! - `rtmps://` - RTMP over TLS  
//! - `rtsp://` - Real-Time Streaming Protocol
//! - `rtsps://` - RTSP over TLS
//!
//! # Architecture
//!
//! The demuxer spawns a dedicated worker thread that owns all FFmpeg state.
//! This is required because FFmpeg contexts are not Send/Sync, but we need
//! to interact with them from async Rust code.

use std::sync::mpsc;
use std::thread;

use remotemedia_runtime_core::data::video::{PixelFormat, VideoCodec};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::ingestion::{
    AudioConfig, AudioTrackProperties, IngestMetadata, MediaType, TrackInfo, TrackProperties,
    TrackSelection, VideoConfig, VideoTrackProperties,
};
use remotemedia_runtime_core::Error;

/// Commands sent to the worker thread
enum WorkerCommand {
    /// Request the next decoded frame
    NextFrame,
    /// Stop the worker thread
    Stop,
}

/// Responses from the worker thread
enum WorkerResponse {
    /// Successfully decoded audio samples
    Audio {
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u16,
        timestamp_us: u64,
    },
    /// Successfully decoded video frame
    Video {
        pixel_data: Vec<u8>,
        width: u32,
        height: u32,
        format: PixelFormat,
        codec: Option<VideoCodec>,
        frame_number: u64,
        timestamp_us: u64,
        is_keyframe: bool,
    },
    /// Stream ended (EOF)
    EndOfStream,
    /// Error occurred
    Error(String),
}

/// RTMP/RTSP stream demuxer and decoder
///
/// Uses FFmpeg's native network protocol support to connect to and decode
/// live streams. All FFmpeg operations happen on a dedicated worker thread.
pub struct RtmpDemuxer {
    /// Channel to send commands to worker
    cmd_tx: mpsc::Sender<WorkerCommand>,

    /// Channel to receive responses from worker
    resp_rx: mpsc::Receiver<WorkerResponse>,

    /// Worker thread handle
    worker_handle: Option<thread::JoinHandle<()>>,

    /// Discovered metadata
    metadata: IngestMetadata,

    /// Target audio sample rate
    target_sample_rate: u32,

    /// Target audio channels  
    #[allow(dead_code)]
    target_channels: u16,

    /// Whether the stream has ended
    ended: bool,
}

impl RtmpDemuxer {
    /// Open an RTMP/RTSP stream
    pub async fn open(
        url: &str,
        audio_config: Option<&AudioConfig>,
        video_config: Option<&VideoConfig>,
        track_selection: &TrackSelection,
    ) -> Result<Self, Error> {
        let _ = (video_config, track_selection);

        let url = url.to_string();
        let target_sample_rate = audio_config.map(|c| c.sample_rate).unwrap_or(16000);
        let target_channels = audio_config.map(|c| c.channels).unwrap_or(1);

        tracing::info!(
            "Opening stream: {} (target: {}Hz {}ch)",
            url,
            target_sample_rate,
            target_channels
        );

        // Create channels for communication
        let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>();
        let (resp_tx, resp_rx) = mpsc::channel::<WorkerResponse>();
        let (init_tx, init_rx) = mpsc::channel::<Result<IngestMetadata, String>>();

        // Spawn worker thread
        let worker_url = url.clone();
        let worker_handle = thread::spawn(move || {
            worker_thread_main(
                worker_url,
                target_sample_rate,
                target_channels,
                cmd_rx,
                resp_tx,
                init_tx,
            );
        });

        // Wait for initialization
        let metadata = tokio::task::spawn_blocking(move || init_rx.recv())
            .await
            .map_err(|e| Error::Other(format!("Task error: {}", e)))?
            .map_err(|_| Error::Other("Worker thread died during init".to_string()))?
            .map_err(|e| Error::Other(e))?;

        tracing::info!("Stream opened successfully");

        Ok(Self {
            cmd_tx,
            resp_rx,
            worker_handle: Some(worker_handle),
            metadata,
            target_sample_rate,
            target_channels,
            ended: false,
        })
    }

    /// Get discovered metadata
    pub fn metadata(&self) -> IngestMetadata {
        self.metadata.clone()
    }

    /// Read and decode the next audio frame
    pub async fn next_frame(&mut self) -> Result<Option<RuntimeData>, Error> {
        if self.ended {
            return Ok(None);
        }

        // Send command to worker
        if self.cmd_tx.send(WorkerCommand::NextFrame).is_err() {
            self.ended = true;
            return Ok(None);
        }

        // Wait for response - use block_in_place to allow blocking in async context
        let response = tokio::task::block_in_place(|| self.resp_rx.recv());

        match response {
            Ok(WorkerResponse::Audio {
                samples,
                sample_rate: _,
                channels,
                timestamp_us,
            }) => Ok(Some(RuntimeData::Audio {
                samples,
                sample_rate: self.target_sample_rate,
                channels: channels as u32,
                stream_id: Some("audio:0".to_string()),
                timestamp_us: Some(timestamp_us),
                arrival_ts_us: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_micros() as u64)
                        .unwrap_or(0),
                ),
            })),
            Ok(WorkerResponse::EndOfStream) => {
                self.ended = true;
                Ok(None)
            }
            Ok(WorkerResponse::Error(e)) => {
                self.ended = true;
                Err(Error::Other(e))
            }
            Err(_) => {
                self.ended = true;
                Ok(None)
            }
        }
    }

    /// Signal the demuxer to stop
    pub fn stop(&self) {
        let _ = self.cmd_tx.send(WorkerCommand::Stop);
    }
}

impl Drop for RtmpDemuxer {
    fn drop(&mut self) {
        self.stop();
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

// ============================================================================
// Worker thread implementation using direct FFmpeg FFI
// ============================================================================

/// FFmpeg FFI - Direct bindings to FFmpeg libraries
/// We define minimal struct layouts for the fields we need
mod ffi {
    #![allow(dead_code)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    
    use std::os::raw::{c_char, c_int, c_uint, c_void};

    // AVMediaType enum values
    pub const AVMEDIA_TYPE_VIDEO: c_int = 0;
    pub const AVMEDIA_TYPE_AUDIO: c_int = 1;

    // Sample formats (from libavutil/samplefmt.h)
    pub const AV_SAMPLE_FMT_NONE: c_int = -1;
    pub const AV_SAMPLE_FMT_U8: c_int = 0;
    pub const AV_SAMPLE_FMT_S16: c_int = 1;
    pub const AV_SAMPLE_FMT_S32: c_int = 2;
    pub const AV_SAMPLE_FMT_FLT: c_int = 3;
    pub const AV_SAMPLE_FMT_DBL: c_int = 4;
    pub const AV_SAMPLE_FMT_U8P: c_int = 5;
    pub const AV_SAMPLE_FMT_S16P: c_int = 6;
    pub const AV_SAMPLE_FMT_S32P: c_int = 7;
    pub const AV_SAMPLE_FMT_FLTP: c_int = 8;
    pub const AV_SAMPLE_FMT_DBLP: c_int = 9;

    // Channel layouts (from libavutil/channel_layout.h)
    pub const AV_CH_LAYOUT_MONO: i64 = 0x4;
    pub const AV_CH_LAYOUT_STEREO: i64 = 0x3;

    // Error codes
    pub const AVERROR_EOF: c_int = fferrtag(b'E', b'O', b'F', b' ');
    pub const AVERROR_EAGAIN: c_int = -11;

    const fn fferrtag(a: u8, b: u8, c: u8, d: u8) -> c_int {
        -((a as c_int) | ((b as c_int) << 8) | ((c as c_int) << 16) | ((d as c_int) << 24))
    }

    // Opaque types - we only access via functions
    pub enum AVCodec {}
    pub enum AVCodecParameters {}
    pub enum AVDictionary {}
    pub enum SwrContext {}

    // AVFormatContext - we need to access nb_streams and streams
    // This is a partial repr(C) struct matching FFmpeg's layout for these fields
    #[repr(C)]
    pub struct AVFormatContext {
        pub av_class: *const c_void,         // const AVClass*
        pub iformat: *const c_void,          // const AVInputFormat*
        pub oformat: *const c_void,          // const AVOutputFormat*
        pub priv_data: *mut c_void,          // void*
        pub pb: *mut c_void,                 // AVIOContext*
        pub ctx_flags: c_int,                // int
        pub nb_streams: c_uint,              // unsigned int
        pub streams: *mut *mut AVStream,     // AVStream**
        // ... more fields we don't need
    }

    // AVStream - we need codecpar
    #[repr(C)]
    pub struct AVStream {
        pub av_class: *const c_void,         // const AVClass*
        pub index: c_int,                    // int
        pub id: c_int,                       // int
        pub codecpar: *mut AVCodecParameters, // AVCodecParameters*
        // ... more fields we don't need
    }

    // AVPacket - we need stream_index
    #[repr(C)]
    pub struct AVPacket {
        pub buf: *mut c_void,                // AVBufferRef*
        pub pts: i64,                        // int64_t
        pub dts: i64,                        // int64_t
        pub data: *mut u8,                   // uint8_t*
        pub size: c_int,                     // int
        pub stream_index: c_int,             // int
        // ... more fields we don't need
    }

    // AVFrame - complex struct, use ac-ffmpeg wrappers for access
    pub enum AVFrame {}

    // AVCodecContext - we'll read values using av_opt_get after opening
    pub enum AVCodecContext {}

    // libavformat
    #[link(name = "avformat")]
    extern "C" {
        pub fn avformat_open_input(
            ps: *mut *mut AVFormatContext,
            url: *const c_char,
            fmt: *const c_void,
            options: *mut *mut AVDictionary,
        ) -> c_int;
        pub fn avformat_close_input(s: *mut *mut AVFormatContext);
        pub fn avformat_find_stream_info(ic: *mut AVFormatContext, options: *mut *mut AVDictionary) -> c_int;
        pub fn av_find_best_stream(
            ic: *mut AVFormatContext,
            media_type: c_int,
            wanted_stream: c_int,
            related_stream: c_int,
            decoder: *mut *const AVCodec,
            flags: c_int,
        ) -> c_int;
        pub fn av_read_frame(s: *mut AVFormatContext, pkt: *mut AVPacket) -> c_int;
    }

    // libavcodec
    #[link(name = "avcodec")]
    extern "C" {
        pub fn avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
        pub fn avcodec_free_context(avctx: *mut *mut AVCodecContext);
        pub fn avcodec_parameters_to_context(codec: *mut AVCodecContext, par: *const AVCodecParameters) -> c_int;
        pub fn avcodec_open2(avctx: *mut AVCodecContext, codec: *const AVCodec, options: *mut *mut AVDictionary) -> c_int;
        pub fn avcodec_send_packet(avctx: *mut AVCodecContext, pkt: *const AVPacket) -> c_int;
        pub fn avcodec_receive_frame(avctx: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
        pub fn avcodec_find_decoder(id: c_int) -> *const AVCodec;
        pub fn avcodec_get_name(id: c_int) -> *const c_char;
    }

    // libavutil
    #[link(name = "avutil")]
    extern "C" {
        pub fn av_strerror(errnum: c_int, errbuf: *mut c_char, errbuf_size: usize) -> c_int;
        pub fn av_dict_set(pm: *mut *mut AVDictionary, key: *const c_char, value: *const c_char, flags: c_int) -> c_int;
        pub fn av_dict_free(m: *mut *mut AVDictionary);
        pub fn av_packet_alloc() -> *mut AVPacket;
        pub fn av_packet_free(pkt: *mut *mut AVPacket);
        pub fn av_packet_unref(pkt: *mut AVPacket);
        pub fn av_frame_alloc() -> *mut AVFrame;
        pub fn av_frame_free(frame: *mut *mut AVFrame);
        pub fn av_frame_unref(frame: *mut AVFrame);
        pub fn av_opt_get_int(obj: *const c_void, name: *const c_char, search_flags: c_int, out_val: *mut i64) -> c_int;
    }

    // ac-ffmpeg wrapper functions (from libffwrapper.a)
    // These provide safe access to complex structs like AVFrame
    extern "C" {
        // Frame accessors (these ARE available in libffwrapper.a)
        pub fn ffw_frame_get_nb_samples(frame: *const AVFrame) -> c_int;
        pub fn ffw_frame_get_sample_rate(frame: *const AVFrame) -> c_int;
        pub fn ffw_frame_get_format(frame: *const AVFrame) -> c_int;
        pub fn ffw_frame_get_plane_data(frame: *const AVFrame, plane: c_int) -> *const u8;
        pub fn ffw_frame_get_channel_layout(frame: *const AVFrame) -> i64;
        
        // Codec parameters accessors (these ARE available in libffwrapper.a)
        pub fn ffw_codec_parameters_get_sample_rate(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_get_channel_layout(params: *const AVCodecParameters) -> i64;
        pub fn ffw_codec_parameters_get_format(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_get_decoder_name(params: *const AVCodecParameters) -> *const c_char;
        pub fn ffw_codec_parameters_is_audio_codec(params: *const AVCodecParameters) -> c_int;
    }

    // libswresample  
    #[link(name = "swresample")]
    extern "C" {
        pub fn swr_alloc() -> *mut SwrContext;
        pub fn swr_free(s: *mut *mut SwrContext);
        pub fn swr_init(s: *mut SwrContext) -> c_int;
        pub fn swr_convert(
            s: *mut SwrContext,
            out: *mut *mut u8,
            out_count: c_int,
            inp: *const *const u8,
            in_count: c_int,
        ) -> c_int;
        pub fn av_opt_set_int(obj: *mut c_void, name: *const c_char, val: i64, search_flags: c_int) -> c_int;
        pub fn av_opt_set_sample_fmt(obj: *mut c_void, name: *const c_char, fmt: c_int, search_flags: c_int) -> c_int;
    }

    pub fn av_err_str(errnum: c_int) -> String {
        let mut buf = [0i8; 256];
        unsafe {
            av_strerror(errnum, buf.as_mut_ptr(), buf.len());
            std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}

/// Worker thread state
struct WorkerState {
    format_ctx: *mut ffi::AVFormatContext,
    audio_codec_ctx: *mut ffi::AVCodecContext,
    audio_stream_index: i32,
    packet: *mut ffi::AVPacket,
    frame: *mut ffi::AVFrame,
    swr_ctx: *mut ffi::SwrContext,
    target_sample_rate: u32,
    target_channels: u16,
    source_format: i32,
    timestamp_us: u64,
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        unsafe {
            if !self.packet.is_null() {
                ffi::av_packet_free(&mut self.packet);
            }
            if !self.frame.is_null() {
                ffi::av_frame_free(&mut self.frame);
            }
            if !self.swr_ctx.is_null() {
                ffi::swr_free(&mut self.swr_ctx);
            }
            if !self.audio_codec_ctx.is_null() {
                ffi::avcodec_free_context(&mut self.audio_codec_ctx);
            }
            if !self.format_ctx.is_null() {
                ffi::avformat_close_input(&mut self.format_ctx);
            }
        }
    }
}

/// Main function for the worker thread
fn worker_thread_main(
    url: String,
    target_sample_rate: u32,
    target_channels: u16,
    cmd_rx: mpsc::Receiver<WorkerCommand>,
    resp_tx: mpsc::Sender<WorkerResponse>,
    init_tx: mpsc::Sender<Result<IngestMetadata, String>>,
) {
    // Initialize FFmpeg state
    let (mut state, metadata) = match init_ffmpeg(&url, target_sample_rate, target_channels) {
        Ok(result) => {
            let _ = init_tx.send(Ok(result.1.clone()));
            result
        }
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    drop(init_tx);

    // Process commands
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            WorkerCommand::NextFrame => {
                match decode_next_frame(&mut state) {
                    Ok(Some((samples, ts))) => {
                        let _ = resp_tx.send(WorkerResponse::Audio {
                            samples,
                            sample_rate: state.target_sample_rate,
                            channels: state.target_channels,
                            timestamp_us: ts,
                        });
                    }
                    Ok(None) => {
                        let _ = resp_tx.send(WorkerResponse::EndOfStream);
                        break;
                    }
                    Err(e) => {
                        let _ = resp_tx.send(WorkerResponse::Error(e));
                        break;
                    }
                }
            }
            WorkerCommand::Stop => break,
        }
    }

    // State is automatically cleaned up via Drop
    let _ = metadata;
}

/// Initialize FFmpeg for URL-based streaming
fn init_ffmpeg(
    url: &str,
    target_sample_rate: u32,
    target_channels: u16,
) -> Result<(WorkerState, IngestMetadata), String> {
    use std::ffi::CString;
    use std::ptr;

    let c_url = CString::new(url).map_err(|_| "Invalid URL encoding".to_string())?;

    unsafe {
        // Set up options for network streams
        let mut options: *mut ffi::AVDictionary = ptr::null_mut();
        let rtsp_transport = CString::new("rtsp_transport").unwrap();
        let tcp = CString::new("tcp").unwrap();
        let stimeout = CString::new("stimeout").unwrap();
        let timeout_val = CString::new("5000000").unwrap();
        let listen = CString::new("listen").unwrap();
        let listen_val = CString::new("0").unwrap();

        ffi::av_dict_set(&mut options, rtsp_transport.as_ptr(), tcp.as_ptr(), 0);
        ffi::av_dict_set(&mut options, stimeout.as_ptr(), timeout_val.as_ptr(), 0);
        ffi::av_dict_set(&mut options, listen.as_ptr(), listen_val.as_ptr(), 0);

        // Open input
        let mut format_ctx: *mut ffi::AVFormatContext = ptr::null_mut();
        let ret = ffi::avformat_open_input(&mut format_ctx, c_url.as_ptr(), ptr::null(), &mut options);

        ffi::av_dict_free(&mut options);

        if ret < 0 {
            return Err(format!("Failed to open {}: {}", url, ffi::av_err_str(ret)));
        }

        // Find stream info
        let ret = ffi::avformat_find_stream_info(format_ctx, ptr::null_mut());
        if ret < 0 {
            ffi::avformat_close_input(&mut format_ctx);
            return Err(format!("Failed to find stream info: {}", ffi::av_err_str(ret)));
        }

        // Find best audio stream - this also gives us the decoder
        let mut decoder: *const ffi::AVCodec = ptr::null();
        let audio_stream_index = ffi::av_find_best_stream(
            format_ctx,
            ffi::AVMEDIA_TYPE_AUDIO,
            -1,
            -1,
            &mut decoder,
            0,
        );

        if audio_stream_index < 0 {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("No audio stream found".to_string());
        }

        // Access stream via our defined struct layout
        let nb_streams = (*format_ctx).nb_streams;
        if audio_stream_index as u32 >= nb_streams {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("Audio stream index out of bounds".to_string());
        }

        let stream = *(*format_ctx).streams.offset(audio_stream_index as isize);
        if stream.is_null() {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("Failed to get audio stream".to_string());
        }

        let codecpar = (*stream).codecpar;
        if codecpar.is_null() {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("Failed to get codec parameters".to_string());
        }

        // Use ac-ffmpeg wrappers to safely read codec parameters
        let source_sample_rate = ffi::ffw_codec_parameters_get_sample_rate(codecpar) as u32;
        let source_channel_layout = ffi::ffw_codec_parameters_get_channel_layout(codecpar);
        let source_channels = if source_channel_layout != 0 {
            (source_channel_layout as u64).count_ones() as u16
        } else {
            2 // Default to stereo if unknown
        };
        let source_format = ffi::ffw_codec_parameters_get_format(codecpar);
        
        // Get codec name
        let codec_name_ptr = ffi::ffw_codec_parameters_get_decoder_name(codecpar);
        let codec_name = if codec_name_ptr.is_null() {
            None
        } else {
            let name = std::ffi::CStr::from_ptr(codec_name_ptr).to_string_lossy().into_owned();
            if name.is_empty() { None } else { Some(name) }
        };

        tracing::info!(
            "Audio stream: {}Hz {} channels, format {}, codec {:?}",
            source_sample_rate,
            source_channels,
            source_format,
            codec_name
        );

        // Create codec context
        if decoder.is_null() {
            // If av_find_best_stream didn't find a decoder, try by codec name
            if let Some(ref name) = codec_name {
                let c_name = CString::new(name.as_str()).ok();
                if let Some(ref c_name) = c_name {
                    // Try to find decoder by name
                    extern "C" {
                        fn avcodec_find_decoder_by_name(name: *const std::os::raw::c_char) -> *const ffi::AVCodec;
                    }
                    decoder = avcodec_find_decoder_by_name(c_name.as_ptr());
                }
            }
        }
        
        if decoder.is_null() {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("No decoder found for audio stream".to_string());
        }

        let audio_codec_ctx = ffi::avcodec_alloc_context3(decoder);
        if audio_codec_ctx.is_null() {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("Failed to allocate codec context".to_string());
        }

        let ret = ffi::avcodec_parameters_to_context(audio_codec_ctx, codecpar);
        if ret < 0 {
            ffi::avcodec_free_context(&mut (audio_codec_ctx as *mut _));
            ffi::avformat_close_input(&mut format_ctx);
            return Err(format!("Failed to copy codec params: {}", ffi::av_err_str(ret)));
        }

        let ret = ffi::avcodec_open2(audio_codec_ctx, decoder, ptr::null_mut());
        if ret < 0 {
            ffi::avcodec_free_context(&mut (audio_codec_ctx as *mut _));
            ffi::avformat_close_input(&mut format_ctx);
            return Err(format!("Failed to open codec: {}", ffi::av_err_str(ret)));
        }

        // NOTE: We intentionally DO NOT resample here.
        // The pipeline's AutoResampleStreamingNode handles resampling more robustly.
        // This adapter just decodes and outputs raw audio at the source format.
        let swr_ctx: *mut ffi::SwrContext = ptr::null_mut();
        
        tracing::info!(
            "Audio decoder ready: {}Hz {}ch, format {}. Pipeline will handle resampling.",
            source_sample_rate,
            source_channels,
            source_format
        );

        // Allocate packet and frame
        let packet = ffi::av_packet_alloc();
        let frame = ffi::av_frame_alloc();

        if packet.is_null() || frame.is_null() {
            if !swr_ctx.is_null() {
                ffi::swr_free(&mut (swr_ctx as *mut _));
            }
            ffi::avcodec_free_context(&mut (audio_codec_ctx as *mut _));
            ffi::avformat_close_input(&mut format_ctx);
            return Err("Failed to allocate packet/frame".to_string());
        }

        // Build metadata - report actual source format (pipeline handles resampling)
        let metadata = IngestMetadata {
            tracks: vec![TrackInfo {
                media_type: MediaType::Audio,
                index: 0,
                stream_id: "audio:0".to_string(),
                language: None,
                codec: codec_name,
                properties: TrackProperties::Audio(AudioTrackProperties {
                    sample_rate: source_sample_rate,
                    channels: source_channels,
                    bit_depth: None,
                }),
            }],
            format: Some("streaming".to_string()),
            duration_ms: None,
            bitrate: None,
        };

        let state = WorkerState {
            format_ctx,
            audio_codec_ctx,
            audio_stream_index,
            packet,
            frame,
            swr_ctx,
            target_sample_rate: source_sample_rate,  // Use source rate (no resampling)
            target_channels: source_channels,        // Use source channels (no resampling)
            source_format,
            timestamp_us: 0,
        };

        Ok((state, metadata))
    }
}

/// Decode next audio frame
fn decode_next_frame(state: &mut WorkerState) -> Result<Option<(Vec<f32>, u64)>, String> {
    unsafe {
        loop {
            // Read packet
            let ret = ffi::av_read_frame(state.format_ctx, state.packet);
            if ret < 0 {
                if ret == ffi::AVERROR_EOF {
                    return Ok(None);
                }
                return Err(format!("Read error: {}", ffi::av_err_str(ret)));
            }

            // Check if it's our audio stream - use direct struct access
            let stream_index = (*state.packet).stream_index;
            if stream_index != state.audio_stream_index {
                ffi::av_packet_unref(state.packet);
                continue;
            }

            // Send packet to decoder
            let ret = ffi::avcodec_send_packet(state.audio_codec_ctx, state.packet);
            ffi::av_packet_unref(state.packet);

            if ret < 0 {
                continue; // Try next packet
            }

            // Receive decoded frame
            let ret = ffi::avcodec_receive_frame(state.audio_codec_ctx, state.frame);
            if ret == ffi::AVERROR_EAGAIN {
                continue; // Need more packets
            }
            if ret < 0 {
                continue;
            }

            // Extract samples using ac-ffmpeg wrapper functions for AVFrame
            let nb_samples = ffi::ffw_frame_get_nb_samples(state.frame);
            let format = ffi::ffw_frame_get_format(state.frame);
            
            // Get channels from frame's channel_layout or fall back to stored value
            let channel_layout = ffi::ffw_frame_get_channel_layout(state.frame);
            let channels = if channel_layout != 0 {
                (channel_layout as u64).count_ones() as usize
            } else {
                state.target_channels as usize
            };

            if nb_samples <= 0 {
                ffi::av_frame_unref(state.frame);
                continue;
            }

            let samples = if !state.swr_ctx.is_null() {
                // Resample
                let frame_sample_rate = ffi::ffw_frame_get_sample_rate(state.frame);
                
                // Validate frame data before resampling
                let plane0 = ffi::ffw_frame_get_plane_data(state.frame, 0);
                if plane0.is_null() {
                    tracing::warn!("Frame has null plane data, skipping");
                    ffi::av_frame_unref(state.frame);
                    continue;
                }
                
                let out_samples = if frame_sample_rate > 0 {
                    (nb_samples as u64 * state.target_sample_rate as u64 / frame_sample_rate as u64 + 256) as usize
                } else {
                    (nb_samples as usize) + 256
                };
                
                // Sanity check output size
                if out_samples > 1_000_000 {
                    tracing::warn!("Unreasonable output sample count: {}, skipping", out_samples);
                    ffi::av_frame_unref(state.frame);
                    continue;
                }
                
                let mut out_buffer: Vec<f32> = vec![0.0; out_samples * state.target_channels as usize];

                // For swr_convert, we need to pass array of plane pointers
                // For interleaved: single pointer to all data
                // For planar: array of pointers, one per channel
                let is_planar = format == ffi::AV_SAMPLE_FMT_U8P 
                    || format == ffi::AV_SAMPLE_FMT_S16P 
                    || format == ffi::AV_SAMPLE_FMT_S32P
                    || format == ffi::AV_SAMPLE_FMT_FLTP 
                    || format == ffi::AV_SAMPLE_FMT_DBLP;

                let mut out_ptr = out_buffer.as_mut_ptr() as *mut u8;
                
                let converted = if is_planar && channels > 1 {
                    // Planar format - collect pointers for each plane
                    let mut in_ptrs: Vec<*const u8> = Vec::with_capacity(channels);
                    for ch in 0..channels {
                        let plane = ffi::ffw_frame_get_plane_data(state.frame, ch as i32);
                        if plane.is_null() {
                            tracing::warn!("Planar frame has null plane {} data, skipping", ch);
                            ffi::av_frame_unref(state.frame);
                            continue;
                        }
                        in_ptrs.push(plane);
                    }
                    
                    ffi::swr_convert(
                        state.swr_ctx,
                        &mut out_ptr,
                        out_samples as i32,
                        in_ptrs.as_ptr(),
                        nb_samples,
                    )
                } else {
                    // Interleaved format - single pointer (plane0 already validated)
                    ffi::swr_convert(
                        state.swr_ctx,
                        &mut out_ptr,
                        out_samples as i32,
                        &plane0,
                        nb_samples,
                    )
                };

                ffi::av_frame_unref(state.frame);

                if converted > 0 {
                    out_buffer.truncate(converted as usize * state.target_channels as usize);
                    out_buffer
                } else {
                    continue;
                }
            } else {
                // No resampling needed - extract samples directly
                let data = ffi::ffw_frame_get_plane_data(state.frame, 0);
                
                // Validate pointer before use
                if data.is_null() {
                    tracing::warn!("Frame has null data pointer, skipping");
                    ffi::av_frame_unref(state.frame);
                    continue;
                }
                
                let total_samples = nb_samples as usize * channels;
                
                // Sanity check
                if total_samples == 0 || total_samples > 1_000_000 {
                    tracing::warn!("Invalid sample count: {} (nb_samples={}, channels={}), skipping", 
                        total_samples, nb_samples, channels);
                    ffi::av_frame_unref(state.frame);
                    continue;
                }

                // For planar formats, only use the first channel (convert to mono)
                // This avoids complex interleaving issues and the pipeline can handle channel conversion
                let samples = match format {
                    f if f == ffi::AV_SAMPLE_FMT_FLT => {
                        // Interleaved float - use as-is
                        std::slice::from_raw_parts(data as *const f32, total_samples).to_vec()
                    }
                    f if f == ffi::AV_SAMPLE_FMT_FLTP => {
                        // Planar float - just use first channel (mono)
                        // Pipeline's AutoResampleNode will handle channel conversion if needed
                        std::slice::from_raw_parts(data as *const f32, nb_samples as usize).to_vec()
                    }
                    f if f == ffi::AV_SAMPLE_FMT_S16 => {
                        // Interleaved s16
                        std::slice::from_raw_parts(data as *const i16, total_samples)
                            .iter()
                            .map(|&s| s as f32 / 32768.0)
                            .collect()
                    }
                    f if f == ffi::AV_SAMPLE_FMT_S16P => {
                        // Planar s16 - just use first channel (mono)
                        std::slice::from_raw_parts(data as *const i16, nb_samples as usize)
                            .iter()
                            .map(|&s| s as f32 / 32768.0)
                            .collect()
                    }
                    f if f == ffi::AV_SAMPLE_FMT_S32 => {
                        // Interleaved s32
                        std::slice::from_raw_parts(data as *const i32, total_samples)
                            .iter()
                            .map(|&s| s as f32 / 2147483648.0)
                            .collect()
                    }
                    f if f == ffi::AV_SAMPLE_FMT_S32P => {
                        // Planar s32 - just use first channel (mono)
                        std::slice::from_raw_parts(data as *const i32, nb_samples as usize)
                            .iter()
                            .map(|&s| s as f32 / 2147483648.0)
                            .collect()
                    }
                    _ => {
                        // Unknown format - try to interpret as f32
                        tracing::warn!("Unknown audio format {}, treating as f32", format);
                        std::slice::from_raw_parts(data as *const f32, nb_samples as usize).to_vec()
                    }
                };

                ffi::av_frame_unref(state.frame);
                samples
            };

            // Calculate new timestamp
            let chunk_duration_us = if state.target_sample_rate > 0 {
                (samples.len() as u64 * 1_000_000)
                    / (state.target_sample_rate as u64 * state.target_channels as u64)
            } else {
                0
            };

            let current_ts = state.timestamp_us;
            state.timestamp_us += chunk_duration_us;

            return Ok(Some((samples, current_ts)));
        }
    }
}

/// Check if a track should be selected based on TrackSelection
#[allow(dead_code)]
fn should_select_track(
    selection: &TrackSelection,
    media_type: MediaType,
    index: u32,
    language: Option<&str>,
) -> bool {
    match selection {
        TrackSelection::All => true,
        TrackSelection::FirstAudioVideo => matches!(media_type, MediaType::Audio | MediaType::Video),
        TrackSelection::Specific(selectors) => selectors.iter().any(|sel| {
            sel.media_type == media_type
                && sel.index.map_or(true, |i| i == index)
                && sel.language.as_deref().map_or(true, |l| Some(l) == language)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remotemedia_runtime_core::ingestion::TrackSelector;

    #[test]
    fn test_should_select_track_all() {
        let selection = TrackSelection::All;
        assert!(should_select_track(&selection, MediaType::Audio, 0, None));
        assert!(should_select_track(&selection, MediaType::Video, 0, None));
    }

    #[test]
    fn test_should_select_track_first_audio_video() {
        let selection = TrackSelection::FirstAudioVideo;
        assert!(should_select_track(&selection, MediaType::Audio, 0, None));
        assert!(should_select_track(&selection, MediaType::Video, 0, None));
        assert!(!should_select_track(&selection, MediaType::Subtitle, 0, None));
    }

    #[test]
    fn test_should_select_track_specific() {
        let selection = TrackSelection::Specific(vec![TrackSelector {
            media_type: MediaType::Audio,
            index: Some(1),
            language: None,
        }]);

        assert!(should_select_track(&selection, MediaType::Audio, 1, None));
        assert!(!should_select_track(&selection, MediaType::Audio, 0, None));
    }
}
