//! Synthetic test data generation for pipeline testing

use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::schema::RuntimeDataType;
use std::f32::consts::PI;
use std::path::Path;
use tracing::info;

/// Factory for generating synthetic test data
pub struct SyntheticDataFactory;

impl SyntheticDataFactory {
    /// Generate synthetic data matching the expected input type
    pub fn generate(data_type: &RuntimeDataType) -> Vec<RuntimeData> {
        match data_type {
            RuntimeDataType::Audio => vec![Self::generate_audio()],
            RuntimeDataType::Video => vec![generate_video()],
            RuntimeDataType::Image => vec![generate_image()],
            RuntimeDataType::Text => vec![generate_text()],
            RuntimeDataType::Json => vec![generate_json()],
            RuntimeDataType::Binary => vec![generate_binary()],
            RuntimeDataType::Tensor => vec![generate_tensor()],
            RuntimeDataType::Numpy => vec![generate_numpy()],
            RuntimeDataType::ControlMessage => vec![], // Internal type, not generated
        }
    }

    /// Generate synthetic audio — sine wave at 440Hz, 1 second, 16kHz mono
    pub fn generate_audio() -> RuntimeData {
        Self::generate_audio_with_params(16000, 1, 1.0, 440.0)
    }

    /// Generate audio with custom parameters
    pub fn generate_audio_with_params(
        sample_rate: u32,
        channels: u32,
        duration_secs: f32,
        frequency: f32,
    ) -> RuntimeData {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * PI * frequency * t).sin() * 0.5
            })
            .collect();

        RuntimeData::Audio {
            samples: samples.into(),
            sample_rate,
            channels,
            stream_id: None,
            timestamp_us: Some(0),
            arrival_ts_us: None,
            metadata: None,
        }
    }

    /// Generate speech-like audio — alternating tone bursts and silence
    /// that may trigger VAD depending on model sensitivity
    pub fn generate_speech_like_audio(sample_rate: u32, duration_secs: f32) -> RuntimeData {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let burst_samples = (sample_rate as f32 * 0.3) as usize; // 300ms burst
        let silence_samples = (sample_rate as f32 * 0.2) as usize; // 200ms silence
        let cycle_len = burst_samples + silence_samples;

        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let pos_in_cycle = i % cycle_len;
                if pos_in_cycle < burst_samples {
                    // White noise shaped with speech-like envelope
                    let t = i as f32 / sample_rate as f32;
                    let noise = ((t * 12345.6789).sin() * 54321.0).sin();
                    let envelope = (pos_in_cycle as f32 / burst_samples as f32 * PI).sin();
                    noise * envelope * 0.3
                } else {
                    // Silence
                    0.0
                }
            })
            .collect();

        RuntimeData::Audio {
            samples: samples.into(),
            sample_rate,
            channels: 1,
            stream_id: None,
            timestamp_us: Some(0),
            arrival_ts_us: None,
            metadata: None,
        }
    }
}

/// Generate chunked audio for streaming pipelines
pub fn generate_audio_chunks(
    sample_rate: u32,
    chunk_size: usize,
    num_chunks: usize,
    frequency: f32,
) -> Vec<RuntimeData> {
    let mut chunks = Vec::with_capacity(num_chunks);
    for chunk_idx in 0..num_chunks {
        let offset = chunk_idx * chunk_size;
        let samples: Vec<f32> = (0..chunk_size)
            .map(|i| {
                let t = (offset + i) as f32 / sample_rate as f32;
                (2.0 * PI * frequency * t).sin() * 0.5
            })
            .collect();

        chunks.push(RuntimeData::Audio {
            samples: samples.into(),
            sample_rate,
            channels: 1,
            stream_id: None,
            timestamp_us: Some((offset as u64 * 1_000_000) / sample_rate as u64),
            arrival_ts_us: None,
            metadata: None,
        });
    }
    chunks
}

fn generate_video() -> RuntimeData {
    // 320x240 black frame, YUV420P
    let y_size = 320 * 240;
    let uv_size = (320 / 2) * (240 / 2);
    let mut pixel_data = vec![0u8; y_size + 2 * uv_size];
    // Set UV to 128 (neutral chroma)
    for byte in pixel_data[y_size..].iter_mut() {
        *byte = 128;
    }

    RuntimeData::Video {
        pixel_data,
        width: 320,
        height: 240,
        format: remotemedia_core::data::PixelFormat::Yuv420p,
        codec: None,
        frame_number: 0,
        timestamp_us: 0,
        is_keyframe: true,
        stream_id: None,
        arrival_ts_us: None,
    }
}

fn generate_image() -> RuntimeData {
    // Minimal valid 1×1 PNG (a single black pixel). Used by manifest
    // tests as a placeholder when a node's `accepts` includes Image —
    // pipeline shape-checks pass without depending on a real encoder.
    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    RuntimeData::Image {
        data: PIXEL_PNG.to_vec(),
        format: remotemedia_core::data::ImageFormat::Png,
        width: 1,
        height: 1,
        timestamp_us: None,
        stream_id: None,
        metadata: None,
    }
}

fn generate_text() -> RuntimeData {
    RuntimeData::Text("The quick brown fox jumps over the lazy dog.".to_string())
}

fn generate_json() -> RuntimeData {
    RuntimeData::Json(serde_json::json!({
        "test": true,
        "message": "synthetic test data"
    }))
}

fn generate_binary() -> RuntimeData {
    RuntimeData::Binary(vec![0u8; 256])
}

fn generate_tensor() -> RuntimeData {
    RuntimeData::Tensor {
        data: vec![0u8; 4 * 10], // 10 float32 zeros
        shape: vec![1, 10],
        dtype: 0, // float32
    }
}

fn generate_numpy() -> RuntimeData {
    RuntimeData::Numpy {
        data: vec![0u8; 4 * 10], // 10 float32 zeros
        shape: vec![1, 10],
        dtype: "float32".to_string(),
        strides: vec![40, 4],
        c_contiguous: true,
        f_contiguous: false,
    }
}

/// Load audio from a WAV file and return as RuntimeData::Audio
///
/// Supports 16-bit PCM and 32-bit float WAV files. Converts 16-bit
/// samples to f32 in the range [-1.0, 1.0].
pub fn load_wav(path: &Path) -> Result<RuntimeData, String> {
    let data =
        std::fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    // Parse WAV header (minimal RIFF/WAVE parser)
    if data.len() < 44 {
        return Err("WAV file too small".to_string());
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("Not a valid WAV file".to_string());
    }

    // Find fmt chunk
    let mut pos = 12;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut audio_format = 0u16;
    let mut data_start = 0usize;
    let mut data_size = 0usize;

    while pos + 8 <= data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;

        if chunk_id == b"fmt " && chunk_size >= 16 {
            audio_format = u16::from_le_bytes(data[pos + 8..pos + 10].try_into().unwrap());
            channels = u16::from_le_bytes(data[pos + 10..pos + 12].try_into().unwrap());
            sample_rate = u32::from_le_bytes(data[pos + 12..pos + 16].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(data[pos + 22..pos + 24].try_into().unwrap());
        } else if chunk_id == b"data" {
            data_start = pos + 8;
            data_size = chunk_size;
            break;
        }
        pos += 8 + chunk_size;
        // Chunks are word-aligned
        if chunk_size % 2 != 0 {
            pos += 1;
        }
    }

    if data_start == 0 || data_size == 0 {
        return Err("WAV file missing data chunk".to_string());
    }
    if audio_format != 1 && audio_format != 3 {
        return Err(format!(
            "Unsupported WAV format: {audio_format} (expected PCM=1 or float=3)"
        ));
    }

    let audio_data = &data[data_start..data_start + data_size.min(data.len() - data_start)];

    let samples: Vec<f32> = match (audio_format, bits_per_sample) {
        (1, 16) => {
            // 16-bit PCM → f32
            audio_data
                .chunks_exact(2)
                .map(|chunk| {
                    let sample = i16::from_le_bytes(chunk.try_into().unwrap());
                    sample as f32 / 32768.0
                })
                .collect()
        }
        (3, 32) => {
            // 32-bit float
            audio_data
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect()
        }
        _ => {
            return Err(format!(
                "Unsupported WAV: format={audio_format}, bits={bits_per_sample}"
            ));
        }
    };

    let duration_secs = samples.len() as f32 / (sample_rate as f32 * channels as f32);
    info!(
        "Loaded WAV: {}Hz, {} channels, {:.2}s, {} samples",
        sample_rate,
        channels,
        duration_secs,
        samples.len()
    );

    Ok(RuntimeData::Audio {
        samples: samples.into(),
        sample_rate,
        channels: channels as u32,
        stream_id: None,
        timestamp_us: Some(0),
        arrival_ts_us: None,
        metadata: None,
    })
}

/// Load audio from a WAV file and chunk it for streaming pipelines
pub fn load_wav_chunked(path: &Path, chunk_size: usize) -> Result<Vec<RuntimeData>, String> {
    let audio = load_wav(path)?;
    match audio {
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels,
            ..
        } => {
            let chunks: Vec<RuntimeData> = samples
                .chunks(chunk_size)
                .enumerate()
                .map(|(i, chunk)| {
                    let offset_samples = i * chunk_size;
                    let timestamp_us = (offset_samples as u64 * 1_000_000) / sample_rate as u64;
                    RuntimeData::Audio {
                        samples: chunk.to_vec().into(),
                        sample_rate,
                        channels,
                        stream_id: None,
                        timestamp_us: Some(timestamp_us),
                        arrival_ts_us: None,
                        metadata: None,
                    }
                })
                .collect();
            info!(
                "Chunked WAV into {} chunks of {chunk_size} samples",
                chunks.len()
            );
            Ok(chunks)
        }
        _ => Err("Expected audio data".to_string()),
    }
}
