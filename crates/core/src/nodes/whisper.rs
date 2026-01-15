//! Whisper transcription node using rwhisper
//!
//! This module provides a Rust-native Whisper transcription implementation
//! using the rwhisper crate for high-performance audio transcription.

use crate::capabilities::{
    AudioConstraints, AudioSampleFormat, ConstraintValue, MediaCapabilities, MediaConstraints,
    TextConstraints,
};
use crate::nodes::{NodeContext, NodeExecutor, NodeInfo};
use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

// Whisper dependencies (optional - stub implementations provided when unavailable)
use rodio;
use rwhisper::{WhisperBuilder, WhisperSource};

/// Whisper transcription node using rwhisper
///
/// This node provides real-time audio transcription using Whisper models
/// through the rwhisper Rust bindings.
///
/// Parameters:
/// - `model_path`: Path to the Whisper GGML model file (optional if model_source is provided)
/// - `model_source`: Pre-defined model to download (e.g., "tiny", "tiny.en", "base", "base.en", "small", "small.en")
/// - `language`: Language code (e.g., "en") or null for auto-detect
/// - `n_threads`: Number of threads for inference (default: 4)
/// - `translate`: Whether to translate to English (default: false)
/// - `accumulate_chunks`: Whether to accumulate chunks and transcribe all at once (default: true)
pub struct RustWhisperNode {
    context: Option<Arc<Mutex<rwhisper::Whisper>>>,
    model_path: Option<String>,
    model_source: Option<String>,
    language: Option<String>,
    n_threads: i32,
    translate: bool,
    accumulate_chunks: bool,
    accumulated_audio: Vec<f32>,
}

impl RustWhisperNode {
    pub fn new() -> Self {
        Self {
            context: None,
            model_path: None,
            model_source: None,
            language: None,
            n_threads: 4,
            translate: false,
            accumulate_chunks: true,
            accumulated_audio: Vec::new(),
        }
    }

    /// Returns the media capabilities for this node (spec 022).
    ///
    /// **Input requirements:**
    /// - Audio: 16kHz sample rate, mono (1 channel), f32 format
    ///
    /// **Output capabilities:**
    /// - Text: UTF-8 JSON format containing transcription results
    ///
    /// Whisper requires 16kHz mono audio. Multi-channel or different sample rate
    /// audio should be converted before being passed to this node.
    pub fn media_capabilities() -> MediaCapabilities {
        MediaCapabilities::with_input_output(
            MediaConstraints::Audio(AudioConstraints {
                sample_rate: Some(ConstraintValue::Exact(16000)),
                channels: Some(ConstraintValue::Exact(1)),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            }),
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
        )
    }

    /// Get reference to the Whisper context (for streaming node use)
    pub fn get_context(&self) -> Option<&Arc<Mutex<rwhisper::Whisper>>> {
        self.context.as_ref()
    }

    fn extract_audio_data(&self, input: &Value) -> Result<Vec<f32>> {
        tracing::info!("extract_audio_data: input = {:?}", input);

        // Handle tuple/list format: [audio_array, sample_rate]
        // This comes from AudioBuffer which outputs (audio_data, sample_rate)
        // audio_data can be either:
        // - 1D array: [sample1, sample2, ...]
        // - 2D array: [[sample1, sample2, ...]] or [[ch1_samples...], [ch2_samples...]]
        // - __numpy__ format: {"__numpy__": true, "array": {...}}
        if let Some(arr) = input.as_array() {
            tracing::info!("Input is array with length: {}", arr.len());
            if arr.len() >= 2 {
                // First element is the audio data
                let audio_element = &arr[0];

                // Check if it's a __numpy__ format
                if let Some(obj) = audio_element.as_object() {
                    if obj
                        .get("__numpy__")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        tracing::info!("Found __numpy__ format, extracting audio data");
                        return self.extract_from_numpy_format(audio_element);
                    }
                }

                // Try to extract as a 2D array first (most common from AudioBuffer)
                if let Some(outer_array) = audio_element.as_array() {
                    tracing::info!("Audio element is array with {} channels", outer_array.len());

                    // Check if it's a 2D array (channels x samples)
                    if !outer_array.is_empty() {
                        // Check if first element is also an array (2D case)
                        if outer_array[0].is_array() {
                            // 2D array: [[ch1_samples], [ch2_samples], ...]
                            // Flatten all channels into a single array (mix to mono)
                            tracing::info!("Found 2D array with {} channels", outer_array.len());

                            let mut all_samples: Vec<Vec<f32>> = Vec::new();
                            let mut max_len = 0;

                            // Collect all channels
                            for channel in outer_array {
                                if let Some(channel_arr) = channel.as_array() {
                                    let samples: Vec<f32> = channel_arr
                                        .iter()
                                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                                        .collect();
                                    max_len = max_len.max(samples.len());
                                    all_samples.push(samples);
                                }
                            }

                            if !all_samples.is_empty() && max_len > 0 {
                                // Mix channels to mono by averaging
                                let mut mono_audio = vec![0.0f32; max_len];
                                for channel_samples in &all_samples {
                                    for (i, &sample) in channel_samples.iter().enumerate() {
                                        mono_audio[i] += sample;
                                    }
                                }
                                // Average across channels
                                let num_channels = all_samples.len() as f32;
                                for sample in mono_audio.iter_mut() {
                                    *sample /= num_channels;
                                }

                                tracing::info!(
                                    "Extracted and mixed {} channels into {} mono samples",
                                    all_samples.len(),
                                    mono_audio.len()
                                );
                                return Ok(mono_audio);
                            }
                        } else {
                            // 1D array directly: [sample1, sample2, ...]
                            tracing::info!("Found 1D array with {} samples", outer_array.len());
                            let audio: Vec<f32> = outer_array
                                .iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect();

                            if !audio.is_empty() {
                                tracing::info!(
                                    "Extracted {} audio samples from 1D array",
                                    audio.len()
                                );
                                return Ok(audio);
                            }
                        }
                    }
                }
            }
        }

        // Handle dict format: {"audio_data": [...], "sample_rate": 16000}
        if let Some(obj) = input.as_object() {
            tracing::info!(
                "Input is object with keys: {:?}",
                obj.keys().collect::<Vec<_>>()
            );
            if let Some(audio_val) = obj.get("audio_data").or_else(|| obj.get("audio")) {
                // Try 2D array first
                if let Some(outer_array) = audio_val.as_array() {
                    if !outer_array.is_empty() && outer_array[0].is_array() {
                        // 2D: [[ch1], [ch2], ...]
                        let mut all_samples: Vec<Vec<f32>> = Vec::new();
                        let mut max_len = 0;

                        for channel in outer_array {
                            if let Some(channel_arr) = channel.as_array() {
                                let samples: Vec<f32> = channel_arr
                                    .iter()
                                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                                    .collect();
                                max_len = max_len.max(samples.len());
                                all_samples.push(samples);
                            }
                        }

                        if !all_samples.is_empty() && max_len > 0 {
                            let mut mono_audio = vec![0.0f32; max_len];
                            for channel_samples in &all_samples {
                                for (i, &sample) in channel_samples.iter().enumerate() {
                                    mono_audio[i] += sample;
                                }
                            }
                            let num_channels = all_samples.len() as f32;
                            for sample in mono_audio.iter_mut() {
                                *sample /= num_channels;
                            }

                            tracing::info!(
                                "Extracted {} audio samples from dict (2D)",
                                mono_audio.len()
                            );
                            return Ok(mono_audio);
                        }
                    } else {
                        // 1D array
                        let audio: Vec<f32> = outer_array
                            .iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect();

                        if !audio.is_empty() {
                            tracing::info!(
                                "Extracted {} audio samples from dict (1D)",
                                audio.len()
                            );
                            return Ok(audio);
                        }
                    }
                }
            }
        }

        tracing::error!("Unable to extract audio data from input: {:?}", input);
        Err(Error::Execution(
            format!("Unable to extract audio data from input. Expected array [audio_array, sample_rate] where audio_array is 1D [f32] or 2D [[f32]]. Got: {:?}", input)
        ))
    }

    fn extract_from_numpy_format(&self, numpy_obj: &Value) -> Result<Vec<f32>> {
        // Extract the array metadata and data
        let array_data = numpy_obj.get("array").ok_or_else(|| {
            Error::Execution("__numpy__ format missing 'array' field".to_string())
        })?;

        let meta = array_data
            .get("meta")
            .ok_or_else(|| Error::Execution("Numpy array missing 'meta' field".to_string()))?;

        let data_b64 = array_data
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Execution("Numpy array missing 'data' field".to_string()))?;

        let shape = meta
            .get("shape")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Execution("Numpy array missing 'shape' in meta".to_string()))?;

        let dtype = meta
            .get("dtype")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Execution("Numpy array missing 'dtype' in meta".to_string()))?;

        tracing::info!("Numpy array: dtype={}, shape={:?}", dtype, shape);

        // Decode base64 data
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .map_err(|e| Error::Execution(format!("Failed to decode base64: {}", e)))?;

        // Convert bytes to f32 array based on dtype
        let audio_vec = match dtype {
            "float32" => {
                // Interpret bytes as f32 array
                let f32_slice = unsafe {
                    std::slice::from_raw_parts(
                        bytes.as_ptr() as *const f32,
                        bytes.len() / std::mem::size_of::<f32>(),
                    )
                };
                f32_slice.to_vec()
            }
            "float64" => {
                // Convert f64 to f32
                let f64_slice = unsafe {
                    std::slice::from_raw_parts(
                        bytes.as_ptr() as *const f64,
                        bytes.len() / std::mem::size_of::<f64>(),
                    )
                };
                f64_slice.iter().map(|&x| x as f32).collect()
            }
            "int16" => {
                // Convert int16 to f32 (normalize to [-1, 1])
                let i16_slice = unsafe {
                    std::slice::from_raw_parts(
                        bytes.as_ptr() as *const i16,
                        bytes.len() / std::mem::size_of::<i16>(),
                    )
                };
                i16_slice.iter().map(|&x| x as f32 / 32768.0).collect()
            }
            _ => {
                return Err(Error::Execution(format!(
                    "Unsupported numpy dtype for audio: {}",
                    dtype
                )))
            }
        };

        // Handle shape - flatten if 2D
        // Note: numpy arrays in row-major (C) order store data as:
        // For shape (channels, samples): [ch0_s0, ch0_s1, ..., ch1_s0, ch1_s1, ...]
        if shape.len() == 2 {
            // 2D array (channels, samples)
            let channels = shape[0].as_u64().unwrap_or(1) as usize;
            let samples_per_channel = shape[1].as_u64().unwrap_or(0) as usize;

            tracing::info!(
                "Processing 2D numpy array: shape=({}, {}), total_elements={}",
                channels,
                samples_per_channel,
                audio_vec.len()
            );

            if channels > 1 && samples_per_channel > 0 {
                // Mix to mono by averaging channels
                tracing::info!(
                    "Mixing {} channels to mono, {} samples per channel",
                    channels,
                    samples_per_channel
                );

                // Verify we have enough data
                let expected_len = channels * samples_per_channel;
                if audio_vec.len() < expected_len {
                    tracing::warn!(
                        "Audio vector length {} < expected {}, using available data",
                        audio_vec.len(),
                        expected_len
                    );
                }

                let mut mono = vec![0.0f32; samples_per_channel];

                // Row-major order: data is [ch0_samples..., ch1_samples..., ...]
                for ch in 0..channels {
                    let ch_start = ch * samples_per_channel;
                    let ch_end = ((ch + 1) * samples_per_channel).min(audio_vec.len());

                    for (s, &sample) in audio_vec[ch_start..ch_end].iter().enumerate() {
                        if s < samples_per_channel {
                            mono[s] += sample;
                        }
                    }
                }

                // Average across channels
                let num_channels_f32 = channels as f32;
                for sample in mono.iter_mut() {
                    *sample /= num_channels_f32;
                }

                tracing::info!(
                    "Extracted {} mono samples from numpy array ({}x{})",
                    mono.len(),
                    channels,
                    samples_per_channel
                );
                Ok(mono)
            } else {
                tracing::info!(
                    "Extracted {} mono samples from numpy array (single channel or empty)",
                    audio_vec.len()
                );
                Ok(audio_vec)
            }
        } else if shape.len() == 1 {
            // 1D array
            tracing::info!("Extracted {} samples from 1D numpy array", audio_vec.len());
            Ok(audio_vec)
        } else {
            tracing::warn!(
                "Unexpected numpy array shape length: {}, treating as 1D",
                shape.len()
            );
            Ok(audio_vec)
        }
    }

    async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<Value> {
        use futures::StreamExt;

        tracing::info!("Starting transcription for {} audio samples", audio.len());

        let whisper = self
            .context
            .as_ref()
            .ok_or_else(|| Error::Execution("Whisper context not initialized".to_string()))?;

        tracing::info!("Acquiring whisper lock...");
        let whisper_guard = whisper.lock().await;
        tracing::info!("✓ Whisper lock acquired");

        // Create a rodio Source from our audio samples
        // SamplesBuffer::new(channels, sample_rate, samples)
        let audio_len = audio.len();
        tracing::info!(
            "Creating rodio SamplesBuffer: {} samples, 1 channel, 16000 Hz",
            audio_len
        );

        // Debug: Check audio values before passing to SamplesBuffer
        let non_zero = audio.iter().filter(|&&s| s.abs() > 0.0001).count();
        let max_val = audio.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
        let first_10: Vec<f32> = audio.iter().take(10).copied().collect();
        tracing::info!(
            "Before SamplesBuffer: non_zero={}/{}, max={:.6}, first_10={:?}",
            non_zero,
            audio_len,
            max_val,
            first_10
        );

        let source = rodio::buffer::SamplesBuffer::new(1, 16000, audio);
        tracing::info!("✓ SamplesBuffer created successfully");

        // Start transcription task
        tracing::info!("Calling whisper_guard.transcribe()...");
        let mut task = whisper_guard.transcribe(source);
        tracing::info!("✓ transcribe() returned a task");

        // Collect all segments from the stream
        let mut segments_list = Vec::new();
        let mut full_text = String::new();

        tracing::info!("About to poll task.next() for first segment (30s timeout)...");

        // Try to get at least one segment with a timeout-like approach
        let first_segment = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            tracing::info!("Inside timeout async block, calling task.next().await...");
            let result = task.next().await;
            tracing::info!("task.next().await returned: {:?}", result.is_some());
            result
        })
        .await
        .map_err(|_| {
            tracing::error!("✗ Transcription TIMED OUT after 30 seconds waiting for first segment");
            Error::Execution("Transcription timed out waiting for first segment".to_string())
        })?;

        if let Some(segment) = first_segment {
            let text = segment.text();
            tracing::info!("Received first segment: '{}'", text);
            let start = segment.start();
            let duration = segment.duration();
            let end = start + duration;

            segments_list.push(json!({
                "start": start,
                "end": end,
                "text": text.trim()
            }));

            full_text.push_str(text);
            full_text.push(' ');

            // Continue getting remaining segments
            tracing::info!("Waiting for remaining segments...");
            while let Some(segment) = task.next().await {
                let text = segment.text();
                tracing::info!("Received segment: {}", text);
                let start = segment.start();
                let duration = segment.duration();
                let end = start + duration;

                segments_list.push(json!({
                    "start": start,
                    "end": end,
                    "text": text.trim()
                }));

                full_text.push_str(text);
                full_text.push(' ');
            }
        }

        tracing::info!(
            "Transcription completed: {} segments, {} characters",
            segments_list.len(),
            full_text.len()
        );

        Ok(json!({
            "text": full_text.trim(),
            "segments": segments_list,
            "language": self.language.clone().unwrap_or_else(|| "auto".to_string()),
            "audio_duration": audio_len as f64 / 16000.0,
        }))
    }
}

impl Default for RustWhisperNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeExecutor for RustWhisperNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        // Extract parameters
        if let Some(model_path) = context.params.get("model_path").and_then(|v| v.as_str()) {
            self.model_path = Some(model_path.to_string());
        }

        if let Some(model_source) = context.params.get("model_source").and_then(|v| v.as_str()) {
            self.model_source = Some(model_source.to_string());
        }

        if let Some(language) = context.params.get("language").and_then(|v| v.as_str()) {
            self.language = Some(language.to_string());
        }

        if let Some(n_threads) = context.params.get("n_threads").and_then(|v| v.as_i64()) {
            self.n_threads = n_threads as i32;
        }

        if let Some(translate) = context.params.get("translate").and_then(|v| v.as_bool()) {
            self.translate = translate;
        }

        // Require either model_path or model_source
        if self.model_path.is_none() && self.model_source.is_none() {
        return Err(Error::Manifest(
            "Either model_path or model_source is required for RustWhisperNode".to_string(),
        ));
            }

            // Note: For now we only support model_source with WhisperBuilder
            // model_path support requires different rwhisper API
            let source_str = self
        .model_source
        .as_ref()
        .or(self.model_path.as_ref())
        .ok_or_else(|| {
            Error::Manifest("model_source or model_path required".to_string())
        })?;

            tracing::info!("Loading Whisper model from source: {}", source_str);

            // Prefer quantized models as they support word-level timestamps via DTW
            let source = match source_str.as_str() {
        // Quantized models (support word timestamps)
        "quantized_tiny" | "tiny_q" => WhisperSource::QuantizedTiny,
        "quantized_tiny_en" | "tiny.en_q" | "tiny_en_q" => WhisperSource::QuantizedTinyEn,
        // Non-quantized models (NO word timestamps - DTW alignment not available)
        "tiny" => WhisperSource::Tiny,
        "tiny.en" | "tiny_en" => WhisperSource::TinyEn,
        "base" => WhisperSource::Base,
        "base.en" | "base_en" => WhisperSource::BaseEn,
        "small" => WhisperSource::Small,
        "small.en" | "small_en" => WhisperSource::SmallEn,
        "medium" => WhisperSource::Medium,
        "medium.en" | "medium_en" => WhisperSource::MediumEn,
        "large-v2" | "large" => WhisperSource::LargeV2,
        // Large v3 turbo (quantized, supports word timestamps)
        "large-v3-turbo" | "large_v3_turbo" => WhisperSource::QuantizedLargeV3Turbo,
        _ => return Err(Error::Manifest(
            format!("Unknown model source: {}. Valid options: tiny, tiny.en, base, base.en, small, small.en, medium, medium.en, large, large-v2, large-v3-turbo. For word timestamps, use quantized_tiny, quantized_tiny_en, or large-v3-turbo", source_str)
        )),
            };

            let builder = WhisperBuilder::default().with_source(source);

            // Note: with_language() requires WhisperLanguage enum, not a string
            // For now, we'll skip this and let the model auto-detect
            // TODO: Map language strings to WhisperLanguage enum
            if self.language.is_some() {
        tracing::info!("Language parameter '{}' provided but not yet implemented (requires WhisperLanguage enum mapping)",
                     self.language.as_ref().unwrap());
            }

            let ctx = builder
        .build_with_loading_handler(|progress| {
            use rwhisper::ModelLoadingProgress;
            match progress {
                ModelLoadingProgress::Downloading { source, progress } => {
                    let pct = (progress.progress as f64 / progress.size as f64) * 100.0;
                    tracing::info!("Downloading model from {}: {:.1}%", source, pct);
                }
                ModelLoadingProgress::Loading { progress } => {
                    let pct = progress * 100.0;
                    tracing::info!("Loading model: {:.1}%", pct);
                }
            }
        })
        .await
        .map_err(|e| Error::Execution(format!("Failed to build Whisper model: {}", e)))?;

        self.context = Some(Arc::new(Mutex::new(ctx)));
        tracing::info!("Whisper model loaded successfully");

        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        tracing::info!("RustWhisperNode::process called");

        // Extract audio data
        tracing::info!("Extracting audio data from input");
        let audio = self.extract_audio_data(&input)?;

            // Skip empty audio
            if audio.is_empty() {
                tracing::warn!("Empty audio data, skipping transcription");
            return Ok(vec![]);
        }

        tracing::info!("Extracted {} audio samples", audio.len());

        if self.accumulate_chunks {
            // Accumulate audio chunks - don't transcribe yet
            tracing::info!(
                "Accumulating {} samples (total accumulated: {})",
                audio.len(),
                self.accumulated_audio.len()
            );
            self.accumulated_audio.extend(audio);
            tracing::info!(
                "Total accumulated audio: {} samples ({:.2}s at 16kHz)",
                self.accumulated_audio.len(),
                self.accumulated_audio.len() as f32 / 16000.0
            );
            Ok(vec![])
        } else {
            // Transcribe immediately (old behavior)
            let non_zero_count = audio.iter().filter(|&&s| s.abs() > 0.0001).count();
            let max_amplitude = audio.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
            tracing::info!(
                "Audio stats: non_zero={}/{}, max_amplitude={:.6}",
                non_zero_count,
                audio.len(),
                max_amplitude
            );

            let result = self.transcribe_audio(audio).await?;
            tracing::info!("Transcription result ready, returning");
            Ok(vec![result])
        }
    }

    fn is_streaming(&self) -> bool {
        self.accumulate_chunks
    }

    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        if self.accumulated_audio.is_empty() {
            tracing::info!("No accumulated audio to transcribe");
            return Ok(vec![]);
        }

            tracing::info!(
                "Transcribing accumulated audio: {} samples ({:.2}s at 16kHz)",
                self.accumulated_audio.len(),
            self.accumulated_audio.len() as f32 / 16000.0
        );

        // Take ownership of accumulated audio and clear the buffer
        let audio = std::mem::take(&mut self.accumulated_audio);
        let result = self.transcribe_audio(audio).await?;

        tracing::info!("Final transcription result: {:?}", result);
        Ok(vec![result])
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.context = None;
        self.accumulated_audio.clear();
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "RustWhisperTranscriber".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Whisper transcription using rwhisper (Rust)".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_creation() {
        let node = RustWhisperNode::new();
        assert_eq!(node.n_threads, 4);
        assert!(!node.translate);
    }

    #[tokio::test]
    async fn test_node_info() {
        let node = RustWhisperNode::new();
        let info = node.info();
        assert_eq!(info.name, "RustWhisperTranscriber");
    }

    #[test]
    fn test_media_capabilities() {
        let caps = RustWhisperNode::media_capabilities();

        // Check input constraints
        let input = caps.default_input().expect("Should have default input");
        match input {
            MediaConstraints::Audio(audio) => {
                assert_eq!(audio.sample_rate, Some(ConstraintValue::Exact(16000)));
                assert_eq!(audio.channels, Some(ConstraintValue::Exact(1)));
                assert_eq!(
                    audio.format,
                    Some(ConstraintValue::Exact(AudioSampleFormat::F32))
                );
            }
            _ => panic!("Expected Audio input constraints"),
        }

        // Check output constraints
        let output = caps.default_output().expect("Should have default output");
        match output {
            MediaConstraints::Text(text) => {
                assert_eq!(
                    text.encoding,
                    Some(ConstraintValue::Exact("utf-8".to_string()))
                );
                assert_eq!(
                    text.format,
                    Some(ConstraintValue::Exact("json".to_string()))
                );
            }
            _ => panic!("Expected Text output constraints"),
        }
    }
}
