//! RuntimeData to Tensor conversion utilities
//!
//! Provides conversion between RemoteMedia RuntimeData types and Candle tensors.

use crate::error::{CandleNodeError, Result};
use remotemedia_core::data_compat::RuntimeData;

/// Extension trait for tensor operations
pub trait TensorExt {
    /// Convert to f32 vector
    fn to_f32_vec(&self) -> Result<Vec<f32>>;
}

/// Converter for RuntimeData to/from tensors
pub struct RuntimeDataConverter;

impl RuntimeDataConverter {
    /// Extract audio samples from RuntimeData
    pub fn extract_audio(data: &RuntimeData, node_id: &str) -> Result<AudioData> {
        match data {
            RuntimeData::Audio {
                samples,
                sample_rate,
                channels,
                ..
            } => Ok(AudioData {
                samples: samples.clone(),
                sample_rate: *sample_rate,
                channels: *channels,
            }),
            other => Err(CandleNodeError::invalid_input(
                node_id,
                "Audio",
                format!("{:?}", other.data_type()),
            )),
        }
    }

    /// Extract video frame from RuntimeData
    pub fn extract_video(data: &RuntimeData, node_id: &str) -> Result<VideoData> {
        match data {
            RuntimeData::Video {
                pixel_data,
                width,
                height,
                format,
                ..
            } => Ok(VideoData {
                pixel_data: pixel_data.clone(),
                width: *width,
                height: *height,
                format: format.clone(),
            }),
            other => Err(CandleNodeError::invalid_input(
                node_id,
                "Video",
                format!("{:?}", other.data_type()),
            )),
        }
    }

    /// Extract text from RuntimeData
    pub fn extract_text(data: &RuntimeData, node_id: &str) -> Result<String> {
        match data {
            RuntimeData::Text(text) => Ok(text.clone()),
            RuntimeData::Json(value) => {
                if let Some(text) = value.as_str() {
                    Ok(text.to_string())
                } else if let Some(prompt) = value.get("prompt").and_then(|v| v.as_str()) {
                    Ok(prompt.to_string())
                } else {
                    Ok(value.to_string())
                }
            }
            other => Err(CandleNodeError::invalid_input(
                node_id,
                "Text",
                format!("{:?}", other.data_type()),
            )),
        }
    }

    /// Create Text RuntimeData from string
    pub fn to_text(text: String) -> RuntimeData {
        RuntimeData::Text(text)
    }

    /// Create JSON RuntimeData from serializable value
    pub fn to_json<T: serde::Serialize>(value: &T) -> Result<RuntimeData> {
        let json = serde_json::to_value(value).map_err(|e| CandleNodeError::OutputConversion {
            node_id: "unknown".to_string(),
            message: e.to_string(),
        })?;
        Ok(RuntimeData::Json(json))
    }
}

/// Extracted audio data
#[derive(Debug, Clone)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
}

impl AudioData {
    /// Resample audio to target sample rate
    pub fn resample(&self, target_rate: u32) -> Result<Self> {
        if self.sample_rate == target_rate {
            return Ok(self.clone());
        }

        use rubato::{FftFixedIn, Resampler};

        let params = rubato::FftFixedInOut::<f32>::new(
            self.sample_rate as usize,
            target_rate as usize,
            1024,
            self.channels as usize,
        ).map_err(|e| CandleNodeError::InputConversion {
            node_id: "resampler".to_string(),
            message: format!("Failed to create resampler: {}", e),
        })?;

        // For simple cases, use linear interpolation as fallback
        let ratio = target_rate as f64 / self.sample_rate as f64;
        let new_len = (self.samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx0 = src_idx.floor() as usize;
            let idx1 = (idx0 + 1).min(self.samples.len() - 1);
            let frac = src_idx - idx0 as f64;
            
            let sample = self.samples[idx0] * (1.0 - frac as f32) + self.samples[idx1] * frac as f32;
            resampled.push(sample);
        }

        Ok(Self {
            samples: resampled,
            sample_rate: target_rate,
            channels: self.channels,
        })
    }

    /// Convert stereo to mono by averaging channels
    pub fn to_mono(&self) -> Self {
        if self.channels == 1 {
            return self.clone();
        }

        let mono_samples: Vec<f32> = self.samples
            .chunks(self.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect();

        Self {
            samples: mono_samples,
            sample_rate: self.sample_rate,
            channels: 1,
        }
    }

    /// Prepare audio for Whisper (16kHz mono)
    pub fn prepare_for_whisper(&self) -> Result<Self> {
        let mono = self.to_mono();
        mono.resample(16000)
    }
}

/// Extracted video data
#[derive(Debug, Clone)]
pub struct VideoData {
    pub pixel_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: remotemedia_core::data_compat::PixelFormat,
}

impl VideoData {
    /// Convert to RGB24 format if needed
    pub fn to_rgb24(&self) -> Result<Self> {
        use remotemedia_core::data_compat::PixelFormat;
        
        match self.format {
            PixelFormat::Rgb24 => Ok(self.clone()),
            PixelFormat::Yuv420p => {
                // YUV420P to RGB24 conversion
                let y_size = (self.width * self.height) as usize;
                let uv_size = y_size / 4;
                
                if self.pixel_data.len() < y_size + 2 * uv_size {
                    return Err(CandleNodeError::InputConversion {
                        node_id: "video".to_string(),
                        message: "Invalid YUV420P data size".to_string(),
                    });
                }

                let y_plane = &self.pixel_data[0..y_size];
                let u_plane = &self.pixel_data[y_size..y_size + uv_size];
                let v_plane = &self.pixel_data[y_size + uv_size..];

                let mut rgb = Vec::with_capacity(y_size * 3);

                for row in 0..self.height {
                    for col in 0..self.width {
                        let y_idx = (row * self.width + col) as usize;
                        let uv_idx = ((row / 2) * (self.width / 2) + (col / 2)) as usize;

                        let y = y_plane[y_idx] as f32;
                        let u = u_plane[uv_idx] as f32 - 128.0;
                        let v = v_plane[uv_idx] as f32 - 128.0;

                        let r = (y + 1.402 * v).clamp(0.0, 255.0) as u8;
                        let g = (y - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
                        let b = (y + 1.772 * u).clamp(0.0, 255.0) as u8;

                        rgb.push(r);
                        rgb.push(g);
                        rgb.push(b);
                    }
                }

                Ok(Self {
                    pixel_data: rgb,
                    width: self.width,
                    height: self.height,
                    format: PixelFormat::Rgb24,
                })
            }
            other => Err(CandleNodeError::InputConversion {
                node_id: "video".to_string(),
                message: format!("Unsupported pixel format: {:?}", other),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_to_mono() {
        let stereo = AudioData {
            samples: vec![1.0, 2.0, 3.0, 4.0], // 2 stereo samples
            sample_rate: 44100,
            channels: 2,
        };

        let mono = stereo.to_mono();
        assert_eq!(mono.channels, 1);
        assert_eq!(mono.samples.len(), 2);
        assert_eq!(mono.samples[0], 1.5); // (1+2)/2
        assert_eq!(mono.samples[1], 3.5); // (3+4)/2
    }

    #[test]
    fn test_extract_text() {
        let data = RuntimeData::Text("hello".to_string());
        let text = RuntimeDataConverter::extract_text(&data, "test").unwrap();
        assert_eq!(text, "hello");
    }
}
