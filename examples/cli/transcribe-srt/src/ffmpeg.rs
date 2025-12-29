//! FFmpeg-based audio decoding for any media format
//!
//! Uses ac-ffmpeg to decode audio streams from any media file (MP4, MP3, MKV, etc.)
//! and convert them to f32 PCM samples suitable for Whisper transcription.

use ac_ffmpeg::codec::audio::{AudioDecoder, AudioResampler, ChannelLayout, SampleFormat};
use ac_ffmpeg::codec::Decoder;
use ac_ffmpeg::format::demuxer::Demuxer;
use ac_ffmpeg::format::io::IO;
use anyhow::{Context, Result};
use std::fs::File;
use std::path::Path;
use std::str::FromStr;

/// Target sample rate for Whisper (16kHz)
const TARGET_SAMPLE_RATE: u32 = 16000;

/// Target channels: mono
const TARGET_CHANNELS: u32 = 1;

/// Decode audio from any media file to f32 PCM samples
///
/// # Arguments
/// * `path` - Path to the media file
///
/// # Returns
/// * `(samples, sample_rate, channels)` - Decoded audio data
pub fn decode_audio_file<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32, u32)> {
    let path = path.as_ref();
    
    tracing::info!("Decoding audio with FFmpeg: {:?}", path);
    
    // Open the file
    let file = File::open(path)
        .with_context(|| format!("Failed to open file: {:?}", path))?;
    
    // Create IO wrapper for FFmpeg
    let io = IO::from_seekable_read_stream(file);
    
    // Create demuxer
    let mut demuxer = Demuxer::builder()
        .build(io)
        .with_context(|| "Failed to create demuxer")?
        .find_stream_info(None)
        .map_err(|(_, e)| e)
        .with_context(|| "Failed to find stream info")?;
    
    // Find the best audio stream
    let (stream_index, stream) = demuxer
        .streams()
        .iter()
        .enumerate()
        .find(|(_, s)| s.codec_parameters().is_audio_codec())
        .with_context(|| "No audio stream found in file")?;
    
    // Get audio codec parameters
    let codec_params = stream
        .codec_parameters()
        .into_audio_codec_parameters()
        .with_context(|| "Stream is not an audio stream")?;
    
    tracing::debug!(
        "Found audio stream {}: sample_rate={}, channels={}",
        stream_index,
        codec_params.sample_rate(),
        codec_params.channel_layout().channels()
    );
    
    // Create audio decoder
    let mut decoder = AudioDecoder::from_stream(stream)
        .with_context(|| "Failed to create audio decoder")?
        .build()
        .with_context(|| "Failed to build audio decoder")?;
    
    // Get source parameters from decoder
    let decoder_params = decoder.codec_parameters();
    let source_sample_rate = decoder_params.sample_rate();
    let source_format = decoder_params.sample_format();
    let source_layout = decoder_params.channel_layout().to_owned();
    
    // Create target format - f32 (flt in FFmpeg terminology)
    let target_format = SampleFormat::from_str("flt")
        .map_err(|_| anyhow::anyhow!("Failed to parse target sample format 'flt'"))?;
    
    // Create target channel layout - mono
    let target_layout = ChannelLayout::from_channels(TARGET_CHANNELS)
        .with_context(|| "Failed to create target channel layout")?;
    
    // Create resampler to convert to target format (16kHz mono f32)
    let mut resampler = AudioResampler::builder()
        .source_channel_layout(source_layout)
        .source_sample_format(source_format)
        .source_sample_rate(source_sample_rate)
        .target_channel_layout(target_layout)
        .target_sample_format(target_format)
        .target_sample_rate(TARGET_SAMPLE_RATE)
        .build()
        .with_context(|| "Failed to create audio resampler")?;
    
    // Collect all decoded samples
    let mut all_samples: Vec<f32> = Vec::new();
    
    // Process all packets using take() method
    loop {
        match demuxer.take() {
            Ok(Some(packet)) => {
                // Skip non-audio packets
                if packet.stream_index() != stream_index {
                    continue;
                }
                
                // Decode packet
                decoder.push(packet)
                    .with_context(|| "Failed to push packet to decoder")?;
                
                // Get decoded frames
                while let Some(frame) = decoder.take()
                    .with_context(|| "Failed to take frame from decoder")? 
                {
                    // Resample frame
                    resampler.push(frame)
                        .with_context(|| "Failed to push frame to resampler")?;
                    
                    while let Some(resampled) = resampler.take()
                        .with_context(|| "Failed to take frame from resampler")?
                    {
                        // Get samples from the resampled frame
                        // For f32 format, the data is directly f32 samples
                        let planes = resampled.planes();
                        if !planes.is_empty() {
                            let data = planes[0].data();
                            // Convert bytes to f32 samples
                            let samples: Vec<f32> = data
                                .chunks_exact(4)
                                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                                .collect();
                            all_samples.extend(samples);
                        }
                    }
                }
            }
            Ok(None) => {
                // End of stream
                break;
            }
            Err(e) => {
                return Err(e).with_context(|| "Failed to read packet");
            }
        }
    }
    
    // Flush decoder
    decoder.flush()
        .with_context(|| "Failed to flush decoder")?;
    
    while let Some(frame) = decoder.take()
        .with_context(|| "Failed to take flushed frame from decoder")?
    {
        resampler.push(frame)
            .with_context(|| "Failed to push flushed frame to resampler")?;
        
        while let Some(resampled) = resampler.take()
            .with_context(|| "Failed to take flushed frame from resampler")?
        {
            let planes = resampled.planes();
            if !planes.is_empty() {
                let data = planes[0].data();
                let samples: Vec<f32> = data
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                all_samples.extend(samples);
            }
        }
    }
    
    // Flush resampler
    resampler.flush()
        .with_context(|| "Failed to flush resampler")?;
    
    while let Some(resampled) = resampler.take()
        .with_context(|| "Failed to take flushed frame from resampler")?
    {
        let planes = resampled.planes();
        if !planes.is_empty() {
            let data = planes[0].data();
            let samples: Vec<f32> = data
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            all_samples.extend(samples);
        }
    }
    
    let duration_secs = all_samples.len() as f32 / TARGET_SAMPLE_RATE as f32;
    tracing::info!(
        "Decoded {} samples ({:.2}s) at {}Hz mono",
        all_samples.len(),
        duration_secs,
        TARGET_SAMPLE_RATE
    );
    
    Ok((all_samples, TARGET_SAMPLE_RATE, TARGET_CHANNELS))
}

/// Check if a file path has a known audio/video extension that FFmpeg can handle
#[allow(dead_code)]
pub fn is_media_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    
    matches!(
        extension.as_deref(),
        Some("mp4") | Some("mkv") | Some("avi") | Some("mov") | Some("webm") |  // Video
        Some("mp3") | Some("aac") | Some("m4a") | Some("ogg") | Some("opus") |  // Audio
        Some("flac") | Some("wma") | Some("aiff") | Some("wav") |               // Lossless audio
        Some("ts") | Some("mts") | Some("m2ts") |                                // Transport stream
        Some("3gp") | Some("3g2") | Some("wmv") | Some("flv")                    // Other formats
    )
}
