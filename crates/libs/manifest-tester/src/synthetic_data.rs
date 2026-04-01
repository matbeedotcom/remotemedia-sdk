//! Synthetic test data generation for pipeline testing

use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::schema::RuntimeDataType;
use std::f32::consts::PI;

/// Factory for generating synthetic test data
pub struct SyntheticDataFactory;

impl SyntheticDataFactory {
    /// Generate synthetic data matching the expected input type
    pub fn generate(data_type: &RuntimeDataType) -> Vec<RuntimeData> {
        match data_type {
            RuntimeDataType::Audio => vec![Self::generate_audio()],
            RuntimeDataType::Video => vec![Self::generate_video()],
            RuntimeDataType::Text => vec![Self::generate_text()],
            RuntimeDataType::Json => vec![Self::generate_json()],
            RuntimeDataType::Binary => vec![Self::generate_binary()],
            RuntimeDataType::Tensor => vec![Self::generate_tensor()],
            RuntimeDataType::Numpy => vec![Self::generate_numpy()],
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
            samples,
            sample_rate,
            channels,
            stream_id: None,
            timestamp_us: Some(0),
            arrival_ts_us: None,
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
            samples,
            sample_rate,
            channels: 1,
            stream_id: None,
            timestamp_us: Some(0),
            arrival_ts_us: None,
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
                samples,
                sample_rate,
                channels: 1,
                stream_id: None,
                timestamp_us: Some((offset as u64 * 1_000_000) / sample_rate as u64),
                arrival_ts_us: None,
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
}
