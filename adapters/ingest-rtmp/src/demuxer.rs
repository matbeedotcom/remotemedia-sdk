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
#[allow(dead_code)]
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

    /// Read and decode the next frame (audio or video)
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

        let arrival_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

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
                arrival_ts_us: Some(arrival_ts),
            })),
            Ok(WorkerResponse::Video {
                pixel_data,
                width,
                height,
                format,
                codec,
                frame_number,
                timestamp_us,
                is_keyframe,
            }) => Ok(Some(RuntimeData::Video {
                pixel_data,
                width,
                height,
                format,
                codec,
                frame_number,
                timestamp_us,
                is_keyframe,
                stream_id: Some("video:0".to_string()),
                arrival_ts_us: Some(arrival_ts),
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

    // Pixel formats (from libavutil/pixfmt.h)
    pub const AV_PIX_FMT_NONE: c_int = -1;
    pub const AV_PIX_FMT_YUV420P: c_int = 0;
    pub const AV_PIX_FMT_YUYV422: c_int = 1;
    pub const AV_PIX_FMT_RGB24: c_int = 2;
    pub const AV_PIX_FMT_BGR24: c_int = 3;
    pub const AV_PIX_FMT_YUV422P: c_int = 4;
    pub const AV_PIX_FMT_YUV444P: c_int = 5;
    pub const AV_PIX_FMT_YUV410P: c_int = 6;
    pub const AV_PIX_FMT_YUV411P: c_int = 7;
    pub const AV_PIX_FMT_GRAY8: c_int = 8;
    pub const AV_PIX_FMT_NV12: c_int = 23;
    pub const AV_PIX_FMT_NV21: c_int = 24;
    pub const AV_PIX_FMT_ARGB: c_int = 25;
    pub const AV_PIX_FMT_RGBA: c_int = 26;

    // Codec IDs for common video codecs
    pub const AV_CODEC_ID_H264: c_int = 27;
    pub const AV_CODEC_ID_VP8: c_int = 139;
    pub const AV_CODEC_ID_VP9: c_int = 167;
    pub const AV_CODEC_ID_AV1: c_int = 226;

    // Picture types for keyframe detection
    pub const AV_PICTURE_TYPE_NONE: c_int = 0;
    pub const AV_PICTURE_TYPE_I: c_int = 1; // Intra (keyframe)

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
    pub enum SwsContext {}

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
        pub fn ffw_frame_get_width(frame: *const AVFrame) -> c_int;
        pub fn ffw_frame_get_height(frame: *const AVFrame) -> c_int;
        pub fn ffw_frame_get_line_size(frame: *const AVFrame, plane: c_int) -> c_int;
        pub fn ffw_frame_get_picture_type(frame: *const AVFrame) -> c_int;
        pub fn ffw_frame_get_pts(frame: *const AVFrame) -> i64;
        
        // Codec parameters accessors (these ARE available in libffwrapper.a)
        pub fn ffw_codec_parameters_get_sample_rate(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_get_channel_layout(params: *const AVCodecParameters) -> i64;
        pub fn ffw_codec_parameters_get_format(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_get_decoder_name(params: *const AVCodecParameters) -> *const c_char;
        pub fn ffw_codec_parameters_is_audio_codec(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_is_video_codec(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_get_width(params: *const AVCodecParameters) -> c_int;
        pub fn ffw_codec_parameters_get_height(params: *const AVCodecParameters) -> c_int;
    }

    // libswscale for video scaling/conversion
    #[link(name = "swscale")]
    extern "C" {
        pub fn sws_getContext(
            srcW: c_int,
            srcH: c_int,
            srcFormat: c_int,
            dstW: c_int,
            dstH: c_int,
            dstFormat: c_int,
            flags: c_int,
            srcFilter: *mut c_void,
            dstFilter: *mut c_void,
            param: *const f64,
        ) -> *mut SwsContext;
        pub fn sws_freeContext(context: *mut SwsContext);
        pub fn sws_scale(
            context: *mut SwsContext,
            srcSlice: *const *const u8,
            srcStride: *const c_int,
            srcSliceY: c_int,
            srcSliceH: c_int,
            dst: *const *mut u8,
            dstStride: *const c_int,
        ) -> c_int;
    }

    // SwsContext flags
    pub const SWS_BILINEAR: c_int = 2;
    pub const SWS_BICUBIC: c_int = 4;
    pub const SWS_FAST_BILINEAR: c_int = 1;

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
#[allow(dead_code)]
struct WorkerState {
    format_ctx: *mut ffi::AVFormatContext,
    // Audio state
    audio_codec_ctx: *mut ffi::AVCodecContext,
    audio_stream_index: i32,
    swr_ctx: *mut ffi::SwrContext,
    target_sample_rate: u32,
    target_channels: u16,
    source_format: i32,
    audio_timestamp_us: u64,
    // Video state
    video_codec_ctx: *mut ffi::AVCodecContext,
    video_stream_index: i32,
    sws_ctx: *mut ffi::SwsContext,
    video_width: u32,
    video_height: u32,
    video_pixel_format: i32,
    video_codec: Option<VideoCodec>,
    video_frame_number: u64,
    // Shared state
    packet: *mut ffi::AVPacket,
    frame: *mut ffi::AVFrame,
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
            if !self.sws_ctx.is_null() {
                ffi::sws_freeContext(self.sws_ctx);
            }
            if !self.audio_codec_ctx.is_null() {
                ffi::avcodec_free_context(&mut self.audio_codec_ctx);
            }
            if !self.video_codec_ctx.is_null() {
                ffi::avcodec_free_context(&mut self.video_codec_ctx);
            }
            if !self.format_ctx.is_null() {
                ffi::avformat_close_input(&mut self.format_ctx);
            }
        }
    }
}

/// Decoded frame result
enum DecodedFrame {
    Audio {
        samples: Vec<f32>,
        timestamp_us: u64,
    },
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
                    Ok(Some(DecodedFrame::Audio { samples, timestamp_us })) => {
                        let _ = resp_tx.send(WorkerResponse::Audio {
                            samples,
                            sample_rate: state.target_sample_rate,
                            channels: state.target_channels,
                            timestamp_us,
                        });
                    }
                    Ok(Some(DecodedFrame::Video {
                        pixel_data,
                        width,
                        height,
                        format,
                        codec,
                        frame_number,
                        timestamp_us,
                        is_keyframe,
                    })) => {
                        let _ = resp_tx.send(WorkerResponse::Video {
                            pixel_data,
                            width,
                            height,
                            format,
                            codec,
                            frame_number,
                            timestamp_us,
                            is_keyframe,
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

    let _ = (target_sample_rate, target_channels); // May be used for future resampling config

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

        let nb_streams = (*format_ctx).nb_streams;
        let mut tracks = Vec::new();

        // =========================================================================
        // AUDIO STREAM SETUP
        // =========================================================================
        let mut audio_decoder: *const ffi::AVCodec = ptr::null();
        let audio_stream_index = ffi::av_find_best_stream(
            format_ctx,
            ffi::AVMEDIA_TYPE_AUDIO,
            -1,
            -1,
            &mut audio_decoder,
            0,
        );

        let (audio_codec_ctx, source_sample_rate, source_channels, source_format, audio_codec_name) = 
            if audio_stream_index >= 0 && (audio_stream_index as u32) < nb_streams {
                let stream = *(*format_ctx).streams.offset(audio_stream_index as isize);
                if !stream.is_null() {
                    let codecpar = (*stream).codecpar;
                    if !codecpar.is_null() {
                        let sample_rate = ffi::ffw_codec_parameters_get_sample_rate(codecpar) as u32;
                        let channel_layout = ffi::ffw_codec_parameters_get_channel_layout(codecpar);
                        let channels = if channel_layout != 0 {
                            (channel_layout as u64).count_ones() as u16
                        } else {
                            2
                        };
                        let format = ffi::ffw_codec_parameters_get_format(codecpar);
                        
                        let codec_name_ptr = ffi::ffw_codec_parameters_get_decoder_name(codecpar);
                        let codec_name = if !codec_name_ptr.is_null() {
                            let name = std::ffi::CStr::from_ptr(codec_name_ptr).to_string_lossy().into_owned();
                            if name.is_empty() { None } else { Some(name) }
                        } else {
                            None
                        };

                        // Find decoder if not found
                        if audio_decoder.is_null() {
                            if let Some(ref name) = codec_name {
                                extern "C" {
                                    fn avcodec_find_decoder_by_name(name: *const std::os::raw::c_char) -> *const ffi::AVCodec;
                                }
                                let c_name = CString::new(name.as_str()).ok();
                                if let Some(ref c_name) = c_name {
                                    audio_decoder = avcodec_find_decoder_by_name(c_name.as_ptr());
                                }
                            }
                        }

                        if !audio_decoder.is_null() {
                            let ctx = ffi::avcodec_alloc_context3(audio_decoder);
                            if !ctx.is_null() {
                                if ffi::avcodec_parameters_to_context(ctx, codecpar) >= 0 {
                                    if ffi::avcodec_open2(ctx, audio_decoder, ptr::null_mut()) >= 0 {
                                        tracing::info!(
                                            "Audio stream: {}Hz {}ch, format {}, codec {:?}",
                                            sample_rate, channels, format, codec_name
                                        );
                                        tracks.push(TrackInfo {
                                            media_type: MediaType::Audio,
                                            index: 0,
                                            stream_id: "audio:0".to_string(),
                                            language: None,
                                            codec: codec_name.clone(),
                                            properties: TrackProperties::Audio(AudioTrackProperties {
                                                sample_rate,
                                                channels,
                                                bit_depth: None,
                                            }),
                                        });
                                        (ctx, sample_rate, channels, format, codec_name)
                                    } else {
                                        ffi::avcodec_free_context(&mut (ctx as *mut _));
                                        (ptr::null_mut(), 0, 0, 0, None)
                                    }
                                } else {
                                    ffi::avcodec_free_context(&mut (ctx as *mut _));
                                    (ptr::null_mut(), 0, 0, 0, None)
                                }
                            } else {
                                (ptr::null_mut(), 0, 0, 0, None)
                            }
                        } else {
                            (ptr::null_mut(), 0, 0, 0, None)
                        }
                    } else {
                        (ptr::null_mut(), 0, 0, 0, None)
                    }
                } else {
                    (ptr::null_mut(), 0, 0, 0, None)
                }
            } else {
                tracing::warn!("No audio stream found");
                (ptr::null_mut(), 0, 0, 0, None)
            };

        let _ = audio_codec_name;

        // =========================================================================
        // VIDEO STREAM SETUP
        // =========================================================================
        let mut video_decoder: *const ffi::AVCodec = ptr::null();
        let video_stream_index = ffi::av_find_best_stream(
            format_ctx,
            ffi::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            &mut video_decoder,
            0,
        );

        let (video_codec_ctx, video_width, video_height, video_pixel_format, video_codec, video_codec_name) = 
            if video_stream_index >= 0 && (video_stream_index as u32) < nb_streams {
                let stream = *(*format_ctx).streams.offset(video_stream_index as isize);
                if !stream.is_null() {
                    let codecpar = (*stream).codecpar;
                    if !codecpar.is_null() {
                        let width = ffi::ffw_codec_parameters_get_width(codecpar) as u32;
                        let height = ffi::ffw_codec_parameters_get_height(codecpar) as u32;
                        let pix_fmt = ffi::ffw_codec_parameters_get_format(codecpar);
                        
                        let codec_name_ptr = ffi::ffw_codec_parameters_get_decoder_name(codecpar);
                        let codec_name = if !codec_name_ptr.is_null() {
                            let name = std::ffi::CStr::from_ptr(codec_name_ptr).to_string_lossy().into_owned();
                            if name.is_empty() { None } else { Some(name) }
                        } else {
                            None
                        };

                        // Map codec name to VideoCodec enum
                        let video_codec = match codec_name.as_deref() {
                            Some(name) if name.contains("264") || name.contains("avc") => Some(VideoCodec::H264),
                            Some(name) if name.contains("vp8") => Some(VideoCodec::Vp8),
                            Some(name) if name.contains("av1") || name.contains("av01") => Some(VideoCodec::Av1),
                            _ => None,
                        };

                        // Find decoder if not found
                        if video_decoder.is_null() {
                            if let Some(ref name) = codec_name {
                                extern "C" {
                                    fn avcodec_find_decoder_by_name(name: *const std::os::raw::c_char) -> *const ffi::AVCodec;
                                }
                                let c_name = CString::new(name.as_str()).ok();
                                if let Some(ref c_name) = c_name {
                                    video_decoder = avcodec_find_decoder_by_name(c_name.as_ptr());
                                }
                            }
                        }

                        if !video_decoder.is_null() && width > 0 && height > 0 {
                            let ctx = ffi::avcodec_alloc_context3(video_decoder);
                            if !ctx.is_null() {
                                if ffi::avcodec_parameters_to_context(ctx, codecpar) >= 0 {
                                    if ffi::avcodec_open2(ctx, video_decoder, ptr::null_mut()) >= 0 {
                                        tracing::info!(
                                            "Video stream: {}x{}, format {}, codec {:?}",
                                            width, height, pix_fmt, codec_name
                                        );
                                        tracks.push(TrackInfo {
                                            media_type: MediaType::Video,
                                            index: 0,
                                            stream_id: "video:0".to_string(),
                                            language: None,
                                            codec: codec_name.clone(),
                                            properties: TrackProperties::Video(VideoTrackProperties {
                                                width,
                                                height,
                                                framerate: 0.0, // Will be determined from stream or estimated
                                                pixel_format: None,
                                            }),
                                        });
                                        (ctx, width, height, pix_fmt, video_codec, codec_name)
                                    } else {
                                        ffi::avcodec_free_context(&mut (ctx as *mut _));
                                        (ptr::null_mut(), 0, 0, 0, None, None)
                                    }
                                } else {
                                    ffi::avcodec_free_context(&mut (ctx as *mut _));
                                    (ptr::null_mut(), 0, 0, 0, None, None)
                                }
                            } else {
                                (ptr::null_mut(), 0, 0, 0, None, None)
                            }
                        } else {
                            (ptr::null_mut(), 0, 0, 0, None, None)
                        }
                    } else {
                        (ptr::null_mut(), 0, 0, 0, None, None)
                    }
                } else {
                    (ptr::null_mut(), 0, 0, 0, None, None)
                }
            } else {
                tracing::info!("No video stream found");
                (ptr::null_mut(), 0, 0, 0, None, None)
            };

        let _ = video_codec_name;

        // Must have at least one stream
        if audio_codec_ctx.is_null() && video_codec_ctx.is_null() {
            ffi::avformat_close_input(&mut format_ctx);
            return Err("No audio or video streams found".to_string());
        }

        // Allocate packet and frame
        let packet = ffi::av_packet_alloc();
        let frame = ffi::av_frame_alloc();

        if packet.is_null() || frame.is_null() {
            if !audio_codec_ctx.is_null() {
                ffi::avcodec_free_context(&mut (audio_codec_ctx as *mut _));
            }
            if !video_codec_ctx.is_null() {
                ffi::avcodec_free_context(&mut (video_codec_ctx as *mut _));
            }
            ffi::avformat_close_input(&mut format_ctx);
            return Err("Failed to allocate packet/frame".to_string());
        }

        // Build metadata
        let metadata = IngestMetadata {
            tracks,
            format: Some("streaming".to_string()),
            duration_ms: None,
            bitrate: None,
        };

        let state = WorkerState {
            format_ctx,
            // Audio
            audio_codec_ctx,
            audio_stream_index,
            swr_ctx: ptr::null_mut(),
            target_sample_rate: source_sample_rate,
            target_channels: source_channels,
            source_format,
            audio_timestamp_us: 0,
            // Video
            video_codec_ctx,
            video_stream_index,
            sws_ctx: ptr::null_mut(), // May be used for format conversion later
            video_width,
            video_height,
            video_pixel_format,
            video_codec,
            video_frame_number: 0,
            // Shared
            packet,
            frame,
        };

        Ok((state, metadata))
    }
}

/// Decode next frame (audio or video)
fn decode_next_frame(state: &mut WorkerState) -> Result<Option<DecodedFrame>, String> {
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

            let stream_index = (*state.packet).stream_index;

            // =========================================================================
            // AUDIO PACKET
            // =========================================================================
            if stream_index == state.audio_stream_index && !state.audio_codec_ctx.is_null() {
                let ret = ffi::avcodec_send_packet(state.audio_codec_ctx, state.packet);
                ffi::av_packet_unref(state.packet);

                if ret < 0 {
                    continue;
                }

                let ret = ffi::avcodec_receive_frame(state.audio_codec_ctx, state.frame);
                if ret == ffi::AVERROR_EAGAIN {
                    continue;
                }
                if ret < 0 {
                    continue;
                }

                let nb_samples = ffi::ffw_frame_get_nb_samples(state.frame);
                let format = ffi::ffw_frame_get_format(state.frame);
                
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

                let data = ffi::ffw_frame_get_plane_data(state.frame, 0);
                if data.is_null() {
                    ffi::av_frame_unref(state.frame);
                    continue;
                }
                
                let total_samples = nb_samples as usize * channels;
                if total_samples == 0 || total_samples > 1_000_000 {
                    ffi::av_frame_unref(state.frame);
                    continue;
                }

                let samples = {
                    use crate::audio_samples::{
                        sample_formats, convert_packed_samples_to_f32, convert_planar_samples_to_f32,
                    };
                    
                    if sample_formats::is_planar(format) {
                        // Planar format - collect plane pointers  
                        let plane_data: Vec<*const u8> = (0..channels)
                            .map(|ch| ffi::ffw_frame_get_plane_data(state.frame, ch as i32) as *const u8)
                            .collect();
                        convert_planar_samples_to_f32(&plane_data, format, nb_samples as usize)
                    } else {
                        // Packed format
                        convert_packed_samples_to_f32(data, format, total_samples)
                    }
                };

                ffi::av_frame_unref(state.frame);

                let chunk_duration_us = if state.target_sample_rate > 0 {
                    (samples.len() as u64 * 1_000_000)
                        / (state.target_sample_rate as u64 * state.target_channels.max(1) as u64)
                } else {
                    0
                };

                let timestamp_us = state.audio_timestamp_us;
                state.audio_timestamp_us += chunk_duration_us;

                return Ok(Some(DecodedFrame::Audio { samples, timestamp_us }));
            }

            // =========================================================================
            // VIDEO PACKET
            // =========================================================================
            if stream_index == state.video_stream_index && !state.video_codec_ctx.is_null() {
                let ret = ffi::avcodec_send_packet(state.video_codec_ctx, state.packet);
                ffi::av_packet_unref(state.packet);

                if ret < 0 {
                    continue;
                }

                let ret = ffi::avcodec_receive_frame(state.video_codec_ctx, state.frame);
                if ret == ffi::AVERROR_EAGAIN {
                    continue;
                }
                if ret < 0 {
                    continue;
                }

                let width = ffi::ffw_frame_get_width(state.frame) as u32;
                let height = ffi::ffw_frame_get_height(state.frame) as u32;
                let pix_fmt = ffi::ffw_frame_get_format(state.frame);
                let picture_type = ffi::ffw_frame_get_picture_type(state.frame);
                let is_keyframe = picture_type == ffi::AV_PICTURE_TYPE_I;
                let pts = ffi::ffw_frame_get_pts(state.frame);

                if width == 0 || height == 0 {
                    ffi::av_frame_unref(state.frame);
                    continue;
                }

                // Map FFmpeg pixel format to our PixelFormat
                let format = match pix_fmt {
                    f if f == ffi::AV_PIX_FMT_YUV420P => PixelFormat::Yuv420p,
                    f if f == ffi::AV_PIX_FMT_NV12 => PixelFormat::NV12,
                    f if f == ffi::AV_PIX_FMT_RGB24 => PixelFormat::Rgb24,
                    f if f == ffi::AV_PIX_FMT_RGBA => PixelFormat::Rgba32,
                    _ => {
                        // For unsupported formats, output as YUV420P (most common)
                        // In future, we could use sws_scale to convert
                        PixelFormat::Yuv420p
                    }
                };

                // Extract pixel data based on format
                let pixel_data = extract_video_frame_data(state.frame, width, height, pix_fmt);

                ffi::av_frame_unref(state.frame);

                if pixel_data.is_empty() {
                    continue;
                }

                // Calculate timestamp in microseconds
                // FFmpeg streams have time_base, but we use a simple frame-based approach
                let timestamp_us = if pts >= 0 {
                    // Use PTS if available (convert from stream time_base to microseconds)
                    // Assuming 1/90000 time_base for video (common), or estimate from frame rate
                    (pts as u64 * 1_000_000) / 90000
                } else {
                    // Estimate from frame number (assume 30fps)
                    (state.video_frame_number * 1_000_000) / 30
                };

                let frame_number = state.video_frame_number;
                state.video_frame_number += 1;

                return Ok(Some(DecodedFrame::Video {
                    pixel_data,
                    width,
                    height,
                    format,
                    codec: state.video_codec,
                    frame_number,
                    timestamp_us,
                    is_keyframe,
                }));
            }

            // Packet doesn't match any stream we're tracking
            ffi::av_packet_unref(state.packet);
        }
    }
}

/// Extract raw pixel data from an AVFrame
unsafe fn extract_video_frame_data(
    frame: *mut ffi::AVFrame,
    width: u32,
    height: u32,
    pix_fmt: i32,
) -> Vec<u8> {
    // For YUV420P (most common), we need Y, U, V planes
    if pix_fmt == ffi::AV_PIX_FMT_YUV420P {
        let y_size = (width * height) as usize;
        let uv_size = (width * height / 4) as usize;
        let total_size = y_size + uv_size * 2;

        let mut data = Vec::with_capacity(total_size);

        // Y plane
        let y_ptr = ffi::ffw_frame_get_plane_data(frame, 0);
        let y_linesize = ffi::ffw_frame_get_line_size(frame, 0) as usize;
        if y_ptr.is_null() {
            return Vec::new();
        }

        // Copy Y plane row by row (handles stride)
        for row in 0..height as usize {
            let src = y_ptr.add(row * y_linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, width as usize));
        }

        // U plane
        let u_ptr = ffi::ffw_frame_get_plane_data(frame, 1);
        let u_linesize = ffi::ffw_frame_get_line_size(frame, 1) as usize;
        if u_ptr.is_null() {
            return Vec::new();
        }

        for row in 0..(height / 2) as usize {
            let src = u_ptr.add(row * u_linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, (width / 2) as usize));
        }

        // V plane
        let v_ptr = ffi::ffw_frame_get_plane_data(frame, 2);
        let v_linesize = ffi::ffw_frame_get_line_size(frame, 2) as usize;
        if v_ptr.is_null() {
            return Vec::new();
        }

        for row in 0..(height / 2) as usize {
            let src = v_ptr.add(row * v_linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, (width / 2) as usize));
        }

        data
    } else if pix_fmt == ffi::AV_PIX_FMT_NV12 {
        // NV12: Y plane + interleaved UV plane
        let y_size = (width * height) as usize;
        let uv_size = (width * height / 2) as usize;
        let total_size = y_size + uv_size;

        let mut data = Vec::with_capacity(total_size);

        // Y plane
        let y_ptr = ffi::ffw_frame_get_plane_data(frame, 0);
        let y_linesize = ffi::ffw_frame_get_line_size(frame, 0) as usize;
        if y_ptr.is_null() {
            return Vec::new();
        }

        for row in 0..height as usize {
            let src = y_ptr.add(row * y_linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, width as usize));
        }

        // UV plane (interleaved)
        let uv_ptr = ffi::ffw_frame_get_plane_data(frame, 1);
        let uv_linesize = ffi::ffw_frame_get_line_size(frame, 1) as usize;
        if uv_ptr.is_null() {
            return Vec::new();
        }

        for row in 0..(height / 2) as usize {
            let src = uv_ptr.add(row * uv_linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, width as usize));
        }

        data
    } else if pix_fmt == ffi::AV_PIX_FMT_RGB24 {
        // Packed RGB24
        let total_size = (width * height * 3) as usize;
        let mut data = Vec::with_capacity(total_size);

        let ptr = ffi::ffw_frame_get_plane_data(frame, 0);
        let linesize = ffi::ffw_frame_get_line_size(frame, 0) as usize;
        if ptr.is_null() {
            return Vec::new();
        }

        for row in 0..height as usize {
            let src = ptr.add(row * linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, (width * 3) as usize));
        }

        data
    } else if pix_fmt == ffi::AV_PIX_FMT_RGBA {
        // Packed RGBA32
        let total_size = (width * height * 4) as usize;
        let mut data = Vec::with_capacity(total_size);

        let ptr = ffi::ffw_frame_get_plane_data(frame, 0);
        let linesize = ffi::ffw_frame_get_line_size(frame, 0) as usize;
        if ptr.is_null() {
            return Vec::new();
        }

        for row in 0..height as usize {
            let src = ptr.add(row * linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, (width * 4) as usize));
        }

        data
    } else {
        // For other formats, try to extract as YUV420P equivalent
        // This is a fallback that may not work correctly for all formats
        let y_size = (width * height) as usize;
        let uv_size = (width * height / 4) as usize;

        let y_ptr = ffi::ffw_frame_get_plane_data(frame, 0);
        if y_ptr.is_null() {
            return Vec::new();
        }

        let y_linesize = ffi::ffw_frame_get_line_size(frame, 0) as usize;
        let mut data = Vec::with_capacity(y_size + uv_size * 2);

        for row in 0..height as usize {
            let src = y_ptr.add(row * y_linesize);
            data.extend_from_slice(std::slice::from_raw_parts(src, width as usize));
        }

        // Fill U and V with neutral gray (128) if not available
        data.resize(y_size + uv_size * 2, 128);

        data
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
