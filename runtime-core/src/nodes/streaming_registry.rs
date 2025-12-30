//! Default streaming node registry with built-in node factories

use crate::capabilities::{
    AudioConstraints, AudioSampleFormat, CapabilityBehavior, ConstraintValue, MediaCapabilities,
    MediaConstraints,
};
use crate::nodes::calculator::CalculatorNode;
use crate::nodes::passthrough::PassThroughNode;
use crate::nodes::python_streaming::PythonStreamingNode;
use crate::nodes::remote_pipeline::RemotePipelineNodeFactory;
use crate::nodes::whisper::RustWhisperNode;
// Temporarily disabled - incomplete implementation
// use crate::nodes::sync_av::SynchronizedAudioVideoNode;
use crate::nodes::video_flip::VideoFlipNode;
// use crate::nodes::video_processor::VideoProcessorNode;
#[cfg(feature = "video")]
use crate::nodes::video::{VideoEncoderNode, VideoEncoderConfig, VideoDecoderNode, VideoDecoderConfig};
use crate::nodes::{
    AsyncNodeWrapper, StreamingNode, StreamingNodeFactory, StreamingNodeRegistry, SyncNodeWrapper,
};
use crate::data::RuntimeData;
use crate::nodes::{NodeContext, NodeExecutor};
use crate::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// Factory implementations for built-in streaming nodes

struct CalculatorNodeFactory;
impl StreamingNodeFactory for CalculatorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = CalculatorNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "CalculatorNode"
    }
}

/// Streaming Whisper node that properly handles RuntimeData::Audio
struct WhisperStreamingNode {
    whisper: Mutex<RustWhisperNode>,
}

impl WhisperStreamingNode {
    fn new(node: RustWhisperNode) -> Self {
        Self {
            whisper: Mutex::new(node),
        }
    }
}

#[async_trait::async_trait]
impl crate::nodes::AsyncStreamingNode for WhisperStreamingNode {
    fn node_type(&self) -> &str {
        "RustWhisperNode"
    }

    async fn initialize(&self) -> Result<(), Error> {
        // Initialization happens in the factory
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let whisper = self.whisper.lock().await;

        // Extract audio samples directly from RuntimeData
        let audio_samples = match &data {
            RuntimeData::Audio { samples, sample_rate, channels, .. } => {
                tracing::debug!(
                    "WhisperStreamingNode received audio: {} samples, {}Hz, {} ch",
                    samples.len(),
                    sample_rate,
                    channels
                );
                
                // Resample to 16kHz if needed (Whisper requires 16kHz)
                let target_rate = 16000u32;
                if *sample_rate != target_rate {
                    tracing::debug!("Resampling from {}Hz to {}Hz", sample_rate, target_rate);
                    resample_audio(samples, *sample_rate, target_rate)
                } else {
                    samples.clone()
                }
            }
            RuntimeData::Json(json) => {
                // Fallback: try to extract from JSON format for backwards compatibility
                if let Some(arr) = json.as_array() {
                    if arr.len() >= 2 {
                        if let Some(audio_arr) = arr[0].as_array() {
                            audio_arr
                                .iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect()
                        } else {
                            return Err(Error::Execution(
                                "JSON input must have audio array as first element".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::Execution(
                            "JSON input must be [audio_array, sample_rate]".to_string(),
                        ));
                    }
                } else {
                    return Err(Error::Execution(format!(
                        "Expected RuntimeData::Audio or JSON array, got: {:?}",
                        json
                    )));
                }
            }
            _ => {
                return Err(Error::Execution(format!(
                    "WhisperStreamingNode expects RuntimeData::Audio, got: {}",
                    data.data_type()
                )));
            }
        };

        // Check if we have enough audio
        if audio_samples.is_empty() {
            tracing::warn!("Received empty audio, returning empty result");
            return Ok(RuntimeData::Json(serde_json::json!({
                "text": "",
                "segments": []
            })));
        }

        // Get whisper context
        let ctx = whisper.get_context().ok_or_else(|| {
            Error::Execution("Whisper model not initialized".to_string())
        })?;

        // Sample rate is 16000 Hz (Whisper requirement)
        const SAMPLE_RATE: usize = 16000;
        
        // Pre-chunk audio into ~10 second segments for fresh DTW alignment each time
        // This prevents timestamp compression that happens with long audio
        const CHUNK_DURATION_SECS: usize = 10;
        const CHUNK_SIZE: usize = SAMPLE_RATE * CHUNK_DURATION_SECS;
        
        let total_samples = audio_samples.len();
        let num_chunks = (total_samples + CHUNK_SIZE - 1) / CHUNK_SIZE;
        
        tracing::info!(
            "Running Whisper transcription on {} samples ({:.1}s) in {} chunks of {}s",
            total_samples,
            total_samples as f64 / SAMPLE_RATE as f64,
            num_chunks,
            CHUNK_DURATION_SECS
        );
        
        use futures::StreamExt;
        let mut full_text = String::new();
        let mut word_chunks: Vec<(f64, f64, String)> = Vec::new();
        
        // Process each chunk separately for fresh DTW alignment
        for chunk_idx in 0..num_chunks {
            let chunk_start_sample = chunk_idx * CHUNK_SIZE;
            let chunk_end_sample = (chunk_start_sample + CHUNK_SIZE).min(total_samples);
            let chunk_samples: Vec<f32> = audio_samples[chunk_start_sample..chunk_end_sample].to_vec();
            
            // Time offset for this chunk in the original audio
            let chunk_time_offset = chunk_start_sample as f64 / SAMPLE_RATE as f64;
            
            tracing::debug!(
                "Processing chunk {}/{}: {:.2}s-{:.2}s ({} samples)", 
                chunk_idx + 1, num_chunks,
                chunk_time_offset,
                chunk_end_sample as f64 / SAMPLE_RATE as f64,
                chunk_samples.len()
            );
            
            let ctx_guard = ctx.lock().await;
            let source = rodio::buffer::SamplesBuffer::new(1, 16000, chunk_samples);
            let mut task = ctx_guard.transcribe(source).timestamped();
            
            while let Ok(Some(segment)) = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                task.next()
            ).await {
                let segment_range = segment.sample_range();
                // Segment times are relative to chunk start, add chunk offset for absolute time
                let segment_start = chunk_time_offset + segment_range.start as f64 / SAMPLE_RATE as f64;
                let segment_end = chunk_time_offset + segment_range.end as f64 / SAMPLE_RATE as f64;
                let text = segment.text().trim();
                
                if text.is_empty() {
                    continue;
                }
                
                if !full_text.is_empty() {
                    full_text.push(' ');
                }
                full_text.push_str(text);
                
                // Try to get word-level timestamps from chunks
                // BPE tokens need to be merged: continuation tokens don't start with space
                let mut got_word_timestamps = false;
                let chunks: Vec<_> = segment.chunks().collect();
                tracing::trace!("Segment '{:.50}...' has {} chunks", text, chunks.len());
                
                // Words with zero duration get prepended to the next valid word
                let mut pending_prefix = String::new();
                
                for chunk in chunks {
                    let chunk_text_raw = chunk.text(); // DON'T trim - leading space matters!
                    let ts = chunk.timestamp();
                    
                    if let Some(ts) = ts {
                        got_word_timestamps = true;
                        let chunk_text = chunk_text_raw.trim();
                        if !chunk_text.is_empty() {
                            // timestamp() returns Range<f32> in seconds RELATIVE to segment start
                            // Add segment_start (which includes chunk offset) for absolute timing
                            let abs_start = segment_start + ts.start as f64;
                            let abs_end = segment_start + ts.end as f64;
                            let duration = abs_end - abs_start;
                            
                            // Skip punctuation that spans long gaps (silence markers)
                            let is_punctuation = !chunk_text.chars().any(|c| c.is_alphabetic());
                            if is_punctuation && duration > 1.0 {
                                tracing::debug!("Skipping punctuation spanning silence: {:.2}s-{:.2}s '{}'", abs_start, abs_end, chunk_text);
                                continue;
                            }
                            
                            // For zero-duration words at time 0, save to prepend to next word
                            if duration < 0.001 && abs_start < 0.1 {
                                tracing::debug!("Deferring misaligned word '{}' to next token", chunk_text);
                                if !pending_prefix.is_empty() {
                                    pending_prefix.push(' ');
                                }
                                pending_prefix.push_str(chunk_text);
                                continue;
                            }
                            
                            // Check if this is a continuation token (no leading space, starts with letter)
                            let is_continuation = !chunk_text_raw.starts_with(' ') 
                                && !chunk_text_raw.starts_with('\'')
                                && chunk_text.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false);
                            
                            if is_continuation && !word_chunks.is_empty() {
                                // Merge with previous token
                                let last = word_chunks.last_mut().unwrap();
                                last.1 = abs_end;
                                last.2.push_str(chunk_text);
                                tracing::trace!("Merged BPE token '{}' -> '{}'", chunk_text, last.2);
                            } else {
                                // Prepend any deferred words
                                let final_text = if pending_prefix.is_empty() {
                                    chunk_text.to_string()
                                } else {
                                    let combined = format!("{} {}", pending_prefix, chunk_text);
                                    tracing::debug!("Prepended '{}' -> '{}'", pending_prefix, combined);
                                    pending_prefix.clear();
                                    combined
                                };
                                tracing::trace!("Word {:.2}s-{:.2}s '{}'", abs_start, abs_end, final_text);
                                word_chunks.push((abs_start, abs_end, final_text));
                            }
                        }
                    }
                }
                
                // If there's still a pending prefix, add it as its own word at segment start
                if !pending_prefix.is_empty() {
                    tracing::debug!("Adding remaining deferred words: '{}'", pending_prefix);
                    word_chunks.push((segment_start, segment_start + 0.5, pending_prefix));
                }
                
                if !got_word_timestamps {
                    tracing::warn!("Segment has no word timestamps - DTW may not be supported by model");
                    word_chunks.push((segment_start, segment_end, text.to_string()));
                }
            }
            
            // Drop context guard to release lock between chunks
            drop(ctx_guard);
        }
        
        tracing::debug!("Transcription complete: {} word chunks from {} audio chunks", word_chunks.len(), num_chunks);
        
        // Group word chunks into subtitle segments (5-7 seconds target, break at sentences)
        let segments = group_words_into_subtitles(&word_chunks, 5.0, 10.0);
        
        tracing::info!("Transcription complete: {} word chunks -> {} subtitle segments", 
            word_chunks.len(), segments.len());

        Ok(RuntimeData::Json(serde_json::json!({
            "text": full_text,
            "segments": segments
        })))
    }
}

/// Join words intelligently - no space before punctuation or contractions
fn smart_join(words: &[&str]) -> String {
    let mut result = String::new();
    for word in words {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        
        // Don't add space before punctuation or contractions
        let needs_space = !result.is_empty() 
            && !word.starts_with(',')
            && !word.starts_with('.')
            && !word.starts_with('!')
            && !word.starts_with('?')
            && !word.starts_with(':')
            && !word.starts_with(';')
            && !word.starts_with('\'')  // contractions like 's, 'll, 're
            && !word.starts_with('\u{2019}');  // curly apostrophe '
        
        if needs_space {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

/// Group word chunks into subtitle segments using actual word timestamps
/// 
/// This ensures subtitles align with actual speech - no cutting mid-word
fn group_words_into_subtitles(
    chunks: &[(f64, f64, String)],
    _target_duration: f64,
    max_duration: f64,
) -> Vec<serde_json::Value> {
    if chunks.is_empty() {
        return Vec::new();
    }
    
    let mut segments = Vec::new();
    let mut current_words: Vec<&str> = Vec::new();
    let mut current_start = chunks[0].0;
    let mut current_end = chunks[0].0;
    
    for (start, end, text) in chunks {
        let word = text.trim();
        
        // Skip empty entries
        if word.is_empty() {
            continue;
        }
        
        // Add word to current segment
        current_words.push(word);
        current_end = *end;
        
        // Now check if we should emit a segment
        let current_duration = current_end - current_start;
        
        // Check for sentence-ending punctuation
        let is_sentence_end = word.ends_with('.') 
            || word.ends_with('!') 
            || word.ends_with('?');
        
        // Decide if we should break here
        // ALWAYS break at sentence end for clean subtitle boundaries
        let should_break = is_sentence_end || current_duration >= max_duration;
        
        if should_break && !current_words.is_empty() {
            // Emit current segment with smart joining (no spaces before punctuation)
            let segment_text = smart_join(&current_words);
            segments.push(serde_json::json!({
                "start": current_start,
                "end": current_end,
                "text": segment_text.trim()
            }));
            
            // Start fresh for next segment
            current_words.clear();
            // Next word will set current_start
            current_start = f64::MAX; // Will be reset on next word
        }
        
        // Track start of segment from first word
        if current_words.len() == 1 {
            current_start = *start;
        }
    }
    
    // Don't forget the last segment
    if !current_words.is_empty() {
        let segment_text = smart_join(&current_words);
        segments.push(serde_json::json!({
            "start": current_start,
            "end": current_end,
            "text": segment_text.trim()
        }));
    }
    
    segments
}

/// Split text into sentences and distribute time proportionally by character count
/// (Fallback when word timestamps aren't available)
fn split_by_sentences(text: &str, start_time: f64, duration: f64) -> Vec<serde_json::Value> {
    // Split on sentence-ending punctuation while keeping the punctuation
    let mut sentences: Vec<String> = Vec::new();
    let mut current = String::new();
    
    for c in text.chars() {
        current.push(c);
        if c == '.' || c == '!' || c == '?' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current = String::new();
        }
    }
    // Don't forget remaining text without ending punctuation
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }
    
    if sentences.is_empty() {
        return vec![serde_json::json!({
            "start": start_time,
            "end": start_time + duration,
            "text": text.trim()
        })];
    }
    
    // Calculate total character count for proportional timing
    let total_chars: usize = sentences.iter().map(|s| s.len()).sum();
    if total_chars == 0 {
        return Vec::new();
    }
    
    // Create segments with proportional timing
    let mut result = Vec::new();
    let mut current_time = start_time;
    
    for sentence in sentences {
        let char_ratio = sentence.len() as f64 / total_chars as f64;
        let sentence_duration = duration * char_ratio;
        let end_time = current_time + sentence_duration;
        
        // Further split if sentence is too long (more than 7 seconds)
        if sentence_duration > 7.0 {
            let sub_segments = split_long_sentence(&sentence, current_time, sentence_duration, 5.0);
            result.extend(sub_segments);
        } else {
            result.push(serde_json::json!({
                "start": current_time,
                "end": end_time,
                "text": sentence.trim()
            }));
        }
        
        current_time = end_time;
    }
    
    result
}

/// Split a long sentence into smaller chunks at comma/clause boundaries
fn split_long_sentence(text: &str, start_time: f64, duration: f64, target_duration: f64) -> Vec<serde_json::Value> {
    // Split on commas for clause boundaries
    let parts: Vec<&str> = text.split(',').collect();
    
    if parts.len() <= 1 {
        // Can't split further, just return as is
        return vec![serde_json::json!({
            "start": start_time,
            "end": start_time + duration,
            "text": text.trim()
        })];
    }
    
    // Group parts to hit target duration
    let total_chars: usize = parts.iter().map(|p| p.len()).sum();
    let chars_per_second = total_chars as f64 / duration;
    let target_chars = (target_duration * chars_per_second) as usize;
    
    let mut result = Vec::new();
    let mut current_text = String::new();
    let mut current_start = start_time;
    let mut current_chars = 0usize;
    
    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        
        // Add comma back except for last part
        let part_with_comma = if i < parts.len() - 1 {
            format!("{},", part)
        } else {
            part.to_string()
        };
        
        if !current_text.is_empty() {
            current_text.push(' ');
        }
        current_text.push_str(&part_with_comma);
        current_chars += part.len();
        
        // Check if we've accumulated enough for a segment
        if current_chars >= target_chars && i < parts.len() - 1 {
            let segment_duration = (current_chars as f64 / total_chars as f64) * duration;
            result.push(serde_json::json!({
                "start": current_start,
                "end": current_start + segment_duration,
                "text": current_text.trim()
            }));
            current_start += segment_duration;
            current_text = String::new();
            current_chars = 0;
        }
    }
    
    // Don't forget the last segment
    if !current_text.is_empty() {
        let remaining_duration = (start_time + duration) - current_start;
        result.push(serde_json::json!({
            "start": current_start,
            "end": current_start + remaining_duration,
            "text": current_text.trim()
        }));
    }
    
    result
}

/// Simple audio resampling using linear interpolation
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(samples.len() - 1);
        let frac = src_idx.fract() as f32;

        let sample = samples[idx0] * (1.0 - frac) + samples[idx1] * frac;
        resampled.push(sample);
    }

    resampled
}

/// Factory for RustWhisperNode (Whisper speech-to-text)
struct RustWhisperNodeFactory;

impl StreamingNodeFactory for RustWhisperNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let mut node = RustWhisperNode::new();

        // Create context for initialization
        let context = NodeContext {
            node_id: node_id.clone(),
            node_type: "RustWhisperNode".to_string(),
            params: params.clone(),
            session_id: None,
            metadata: HashMap::new(),
        };

        // Initialize synchronously using tokio runtime
        // This loads the model
        let init_result = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                node.initialize(&context).await?;
                Ok::<_, Error>(node)
            })
        })
        .join()
        .map_err(|_| Error::Execution("Whisper initialization thread panicked".to_string()))?;

        let initialized_node = init_result?;
        let streaming_node = WhisperStreamingNode::new(initialized_node);

        Ok(Box::new(AsyncNodeWrapper(Arc::new(streaming_node))))
    }

    fn node_type(&self) -> &str {
        "RustWhisperNode"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("RustWhisperNode")
                .description("Speech-to-text transcription using Whisper (Rust implementation)")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json, RuntimeDataType::Text])
        )
    }

    fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
        // Whisper has static capabilities regardless of params
        Some(RustWhisperNode::media_capabilities())
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}

// Temporarily disabled - VideoProcessorNode has incomplete implementation
/*
struct VideoProcessorNodeFactory;
impl StreamingNodeFactory for VideoProcessorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = VideoProcessorNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "VideoProcessorNode"
    }
}
*/

struct VideoFlipNodeFactory;
impl StreamingNodeFactory for VideoFlipNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::video_flip::VideoFlipConfig;
        let config: VideoFlipConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoFlipNode::new(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoFlip"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::video_flip::VideoFlipConfig;
        Some(
            NodeSchema::new("VideoFlip")
                .description("Flips video frames horizontally or vertically")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoFlipConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoEncoderNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoEncoderNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: VideoEncoderConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoEncoderNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoEncoder: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoEncoder"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("VideoEncoder")
                .description("Encodes raw video frames to compressed bitstreams (VP8/AV1/H.264)")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoEncoderConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoDecoderNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoDecoderNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let config: VideoDecoderConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoDecoderNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoDecoder: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoDecoder"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("VideoDecoder")
                .description("Decodes compressed video bitstreams to raw frames")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoDecoderConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoScalerNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoScalerNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::video::{VideoScalerNode, VideoScalerConfig};
        let config: VideoScalerConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoScalerNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoScaler: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoScaler"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::video::VideoScalerConfig;
        Some(
            NodeSchema::new("VideoScaler")
                .description("Scales/resizes video frames")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoScalerConfig>(),
        )
    }
}

#[cfg(feature = "video")]
struct VideoFormatConverterNodeFactory;

#[cfg(feature = "video")]
impl StreamingNodeFactory for VideoFormatConverterNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::video::{VideoFormatConverterNode, VideoFormatConverterConfig};
        let config: VideoFormatConverterConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = VideoFormatConverterNode::new(config)
            .map_err(|e| Error::Execution(format!("Failed to create VideoFormatConverter: {}", e)))?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VideoFormatConverter"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::video::VideoFormatConverterConfig;
        Some(
            NodeSchema::new("VideoFormatConverter")
                .description("Converts between pixel formats (RGB/YUV/NV12)")
                .category("video")
                .accepts([RuntimeDataType::Video])
                .produces([RuntimeDataType::Video])
                .config_schema_from::<VideoFormatConverterConfig>(),
        )
    }
}

// Temporarily disabled - SynchronizedAudioVideoNode has incomplete implementation
/*
struct SynchronizedAudioVideoNodeFactory;
impl StreamingNodeFactory for SynchronizedAudioVideoNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = SynchronizedAudioVideoNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "SynchronizedAudioVideoNode"
    }
}
*/

struct PassThroughNodeFactory;
impl StreamingNodeFactory for PassThroughNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        let node = PassThroughNode::new(node_id, &params_str)?;
        Ok(Box::new(SyncNodeWrapper(node)))
    }

    fn node_type(&self) -> &str {
        "PassThrough"
    }
}

/// WhisperX transcription node (Python) - provides word-level timestamps via alignment
struct WhisperXNodeFactory;
impl StreamingNodeFactory for WhisperXNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "WhisperXTranscriber", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "WhisperXTranscriber", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "WhisperXNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("WhisperXNode")
                .description("Speech-to-text with word-level timestamps using WhisperX (Python)")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json])
        )
    }
}

/// HuggingFace Whisper transcription node (Python) - word-level timestamps via transformers
struct HFWhisperNodeFactory;
impl StreamingNodeFactory for HFWhisperNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "WhisperTranscriptionNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "WhisperTranscriptionNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "HFWhisperNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Yields WordUpdate objects for each word
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("HFWhisperNode")
                .description("Speech-to-text with word-level timestamps using HuggingFace Whisper (Python)")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json])
        )
    }
}

struct KokoroTTSNodeFactory;
impl StreamingNodeFactory for KokoroTTSNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "KokoroTTSNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "KokoroTTSNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "KokoroTTSNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Kokoro yields multiple audio chunks per text input
    }
}

struct VibeVoiceTTSNodeFactory;
impl StreamingNodeFactory for VibeVoiceTTSNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "VibeVoiceTTSNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "VibeVoiceTTSNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "VibeVoiceTTSNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // VibeVoice yields multiple audio chunks per text input
    }
}

struct SimplePyTorchNodeFactory;
impl StreamingNodeFactory for SimplePyTorchNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "SimplePyTorchNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "SimplePyTorchNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SimplePyTorchNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }
}

struct AudioBufferAccumulatorNodeFactory;
impl StreamingNodeFactory for AudioBufferAccumulatorNodeFactory {
    fn create(
        &self,
        _node_id: String,  // Reserved for future node identification/logging
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::AudioBufferAccumulatorNode;

        let min_duration_ms = params
            .get("minUtteranceDurationMs")
            .or(params.get("min_utterance_duration_ms"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let max_duration_ms = params
            .get("maxUtteranceDurationMs")
            .or(params.get("max_utterance_duration_ms"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        let node = AudioBufferAccumulatorNode::new(min_duration_ms, max_duration_ms);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "AudioBufferAccumulatorNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or 1 items (when speech ends)
    }
}

struct TextCollectorNodeFactory;
impl StreamingNodeFactory for TextCollectorNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::text_collector::{TextCollectorNode, TextCollectorConfig};
        let config: TextCollectorConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = TextCollectorNode::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "TextCollectorNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or multiple items (complete sentences)
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::text_collector::TextCollectorConfig;
        Some(
            NodeSchema::new("TextCollectorNode")
                .description("Accumulates streaming text tokens and yields complete sentences")
                .category("text")
                .accepts([RuntimeDataType::Text])
                .produces([RuntimeDataType::Text])
                .config_schema_from::<TextCollectorConfig>(),
        )
    }
}

#[cfg(feature = "silero-vad")]
struct SileroVADNodeFactory;

#[cfg(feature = "silero-vad")]
impl StreamingNodeFactory for SileroVADNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::silero_vad::{SileroVADNode, SileroVADConfig};
        let config: SileroVADConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = SileroVADNode::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SileroVADNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs 2 items: VAD event + pass-through audio
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        use crate::nodes::silero_vad::SileroVADConfig;
        Some(
            NodeSchema::new("SileroVADNode")
                .description("Voice activity detection using Silero VAD ONNX model")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Json, RuntimeDataType::Audio])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Fast,
                })
                .config_schema_from::<SileroVADConfig>(),
        )
    }
}

// Spec 007: Speculative VAD Gate for low-latency streaming
struct SpeculativeVADGateFactory;
impl StreamingNodeFactory for SpeculativeVADGateFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::speculative_vad_gate::{SpeculativeVADGate, SpeculativeVADGateConfig};

        // Deserialize config directly - #[serde(default)] handles missing fields,
        // #[serde(alias = "camelCase")] handles both snake_case and camelCase
        let config: SpeculativeVADGateConfig = serde_json::from_value(params.clone())
            .unwrap_or_default();

        let node = SpeculativeVADGate::new(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SpeculativeVADGate"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs audio + optional cancellation messages
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        use crate::nodes::speculative_vad_gate::SpeculativeVADGateConfig;
        Some(
            NodeSchema::new("SpeculativeVADGate")
                .description("Speculative VAD gate for low-latency voice interaction")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio, RuntimeDataType::ControlMessage])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Realtime,
                })
                .config_schema_from::<SpeculativeVADGateConfig>(),
        )
    }
}

// Speculative VAD Coordinator - integrates speculative forwarding with Silero VAD
struct SpeculativeVADCoordinatorFactory;
impl StreamingNodeFactory for SpeculativeVADCoordinatorFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::speculative_vad_coordinator::{SpeculativeVADCoordinator, SpeculativeVADCoordinatorConfig};

        let config: SpeculativeVADCoordinatorConfig = serde_json::from_value(params.clone())
            .unwrap_or_default();

        let node = SpeculativeVADCoordinator::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SpeculativeVADCoordinator"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Outputs audio + VAD JSON + optional cancellation messages
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        use crate::nodes::speculative_vad_coordinator::SpeculativeVADCoordinatorConfig;
        Some(
            NodeSchema::new("SpeculativeVADCoordinator")
                .description("Speculative VAD coordinator integrating immediate forwarding with Silero VAD for false positive detection")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio, RuntimeDataType::Json, RuntimeDataType::ControlMessage])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Realtime,
                })
                .config_schema_from::<SpeculativeVADCoordinatorConfig>(),
        )
    }
}

struct LFM2AudioNodeFactory;
impl StreamingNodeFactory for LFM2AudioNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "LFM2AudioNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "LFM2AudioNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LFM2AudioNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // LFM2Audio yields multiple tokens (text and audio) per input
    }
}

struct AudioChunkerNodeFactory;
impl StreamingNodeFactory for AudioChunkerNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::audio_chunker::{AudioChunkerNode, AudioChunkerConfig};
        let config: AudioChunkerConfig = serde_json::from_value(params.clone()).unwrap_or_default();
        let node = AudioChunkerNode::with_config(config);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "AudioChunkerNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Can output 0 or multiple chunks per input
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        use crate::nodes::audio_chunker::AudioChunkerConfig;
        Some(
            NodeSchema::new("AudioChunkerNode")
                .description("Splits incoming audio into fixed-size chunks")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio])
                .config_schema_from::<AudioChunkerConfig>(),
        )
    }
}

struct FastResampleNodeFactory;
impl StreamingNodeFactory for FastResampleNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        use crate::nodes::audio::{FastResampleNode, ResampleQuality};
        use crate::nodes::audio_resample_streaming::{AutoResampleConfig, AutoResampleStreamingNode, ResampleStreamingNode};

        // Parse optional source_rate (can be "auto" or omitted for auto-detection)
        let source_rate = params
            .get("sourceRate")
            .or(params.get("source_rate"))
            .and_then(|v| {
                // Allow "auto" string to mean auto-detect
                if v.as_str() == Some("auto") {
                    None
                } else {
                    v.as_u64().map(|n| n as u32)
                }
            });

        // Parse optional target_rate (can be "auto" or omitted for passthrough/adaptive)
        let target_rate = params
            .get("targetRate")
            .or(params.get("target_rate"))
            .and_then(|v| {
                if v.as_str() == Some("auto") {
                    None
                } else {
                    v.as_u64().map(|n| n as u32)
                }
            });

        let quality_str = params
            .get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("Medium");

        let quality = match quality_str {
            "Low" => ResampleQuality::Low,
            "Medium" => ResampleQuality::Medium,
            "High" => ResampleQuality::High,
            _ => ResampleQuality::Medium,
        };

        let channels = params.get("channels").and_then(|v| v.as_u64()).map(|n| n as usize);

        // Use AutoResampleStreamingNode when source or target rate is not specified
        // This enables lazy initialization with auto-detection from incoming data
        if source_rate.is_none() || target_rate.is_none() {
            let config = AutoResampleConfig {
                source_rate,
                target_rate,
                quality,
                channels,
            };
            let node = AutoResampleStreamingNode::new(node_id, config);
            return Ok(Box::new(AsyncNodeWrapper(Arc::new(node))));
        }

        // Both rates specified - use the original fixed-rate resampler
        let source_rate = source_rate.unwrap();
        let target_rate = target_rate.unwrap();
        let channels = channels.unwrap_or(1);

        let inner = FastResampleNode::new(source_rate, target_rate, quality, channels)?;
        let node = ResampleStreamingNode::new(inner, target_rate);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "FastResampleNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        false // Always outputs exactly 1 chunk per input
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("FastResampleNode")
                .description("High-quality audio resampling using sinc interpolation. Supports auto-detection of sample rates from connected nodes.")
                .category("audio")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio])
                .config_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source_rate": {
                            "oneOf": [
                                { "type": "integer", "minimum": 8000, "maximum": 192000 },
                                { "type": "string", "enum": ["auto"] }
                            ],
                            "description": "Source sample rate in Hz. Use 'auto' or omit to detect from incoming audio."
                        },
                        "target_rate": {
                            "oneOf": [
                                { "type": "integer", "minimum": 8000, "maximum": 192000 },
                                { "type": "string", "enum": ["auto"] }
                            ],
                            "description": "Target sample rate in Hz. Use 'auto' or omit to adapt to downstream requirements."
                        },
                        "quality": {
                            "type": "string",
                            "description": "Resampling quality",
                            "enum": ["Low", "Medium", "High"],
                            "default": "Medium"
                        },
                        "channels": {
                            "type": "integer",
                            "description": "Number of audio channels. Omit to detect from incoming audio.",
                            "minimum": 1,
                            "maximum": 8
                        }
                    }
                })),
        )
    }

    fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        // Check if explicit source and target rates are provided
        let source_rate = params
            .get("sourceRate")
            .or(params.get("source_rate"))
            .and_then(|v| {
                if v.as_str() == Some("auto") { None } else { v.as_u64().map(|n| n as u32) }
            });

        let target_rate = params
            .get("targetRate")
            .or(params.get("target_rate"))
            .and_then(|v| {
                if v.as_str() == Some("auto") { None } else { v.as_u64().map(|n| n as u32) }
            });

        let channels = params
            .get("channels")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        // When both source and target rates are explicit, return Configured capabilities
        // Note: Channels are kept flexible on input (range 1-8) to allow the resample node
        // to accept any channel count and output the configured count.
        // This is similar to how a resampler can accept various input rates.
        if let (Some(_source), Some(target)) = (source_rate, target_rate) {
            // Output channel constraint (exact if specified, else flexible)
            let output_channel_constraint = channels
                .map(ConstraintValue::Exact)
                .unwrap_or(ConstraintValue::Range { min: 1, max: 8 });

            return Some(MediaCapabilities::with_input_output(
                // Input: accept wide range of sample rates and channels
                MediaConstraints::Audio(AudioConstraints {
                    sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
                    channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                }),
                // Output: exact target rate with optional exact channels
                MediaConstraints::Audio(AudioConstraints {
                    sample_rate: Some(ConstraintValue::Exact(target)),
                    channels: Some(output_channel_constraint),
                    format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
                }),
            ));
        }

        // When auto-configuration is needed, return Adaptive capabilities:
        // - Input: accepts a wide range of sample rates (8kHz - 192kHz)
        // - Output: None initially - adapts to downstream requirements during reverse pass
        Some(MediaCapabilities::with_input(MediaConstraints::Audio(
            AudioConstraints {
                sample_rate: Some(ConstraintValue::Range {
                    min: 8000,
                    max: 192000,
                }),
                channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
                format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
            },
        )))
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        // Default to Adaptive - the actual behavior is determined by media_capabilities():
        // - If media_capabilities() returns both input AND output, it acts as Configured
        // - If media_capabilities() returns only input, it acts as Adaptive
        // The resolver checks for output capabilities to determine if adaptation is needed.
        CapabilityBehavior::Adaptive
    }
}

// Test node factories for Python streaming nodes
struct ExpanderNodeFactory;
impl StreamingNodeFactory for ExpanderNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "ExpanderNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "ExpanderNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "ExpanderNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct RangeGeneratorNodeFactory;
impl StreamingNodeFactory for RangeGeneratorNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "RangeGeneratorNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "RangeGeneratorNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "RangeGeneratorNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct TransformAndExpandNodeFactory;
impl StreamingNodeFactory for TransformAndExpandNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "TransformAndExpandNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "TransformAndExpandNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "TransformAndExpandNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct ChainedTransformNodeFactory;
impl StreamingNodeFactory for ChainedTransformNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "ChainedTransformNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "ChainedTransformNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "ChainedTransformNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct ConditionalExpanderNodeFactory;
impl StreamingNodeFactory for ConditionalExpanderNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "ConditionalExpanderNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "ConditionalExpanderNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "ConditionalExpanderNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }
}

struct FilterNodeFactory;
impl StreamingNodeFactory for FilterNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = if let Some(sid) = session_id {
            PythonStreamingNode::with_session(node_id, "FilterNode", params, sid)?
        } else {
            PythonStreamingNode::new(node_id, "FilterNode", params)?
        };
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "FilterNode"
    }

    fn is_python_node(&self) -> bool {
        true
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // May output 0 or 1 items per input
    }
}

/// Create a default streaming node registry with all built-in nodes registered
pub fn create_default_streaming_registry() -> StreamingNodeRegistry {
    let mut registry = StreamingNodeRegistry::new();

    // Register all built-in streaming nodes
    registry.register(Arc::new(CalculatorNodeFactory));
    // Temporarily disabled - incomplete implementations
    // registry.register(Arc::new(VideoProcessorNodeFactory));
    registry.register(Arc::new(VideoFlipNodeFactory));

    // Register video codec nodes (Spec 012: Video Codec Support)
    #[cfg(feature = "video")]
    {
        registry.register(Arc::new(VideoEncoderNodeFactory));
        registry.register(Arc::new(VideoDecoderNodeFactory));
        registry.register(Arc::new(VideoScalerNodeFactory));
        registry.register(Arc::new(VideoFormatConverterNodeFactory));
    }

    // registry.register(Arc::new(SynchronizedAudioVideoNodeFactory));
    registry.register(Arc::new(PassThroughNodeFactory));

    // Register audio processing nodes
    registry.register(Arc::new(AudioChunkerNodeFactory));
    registry.register(Arc::new(AudioBufferAccumulatorNodeFactory));
    registry.register(Arc::new(FastResampleNodeFactory));

    // Register text processing nodes
    registry.register(Arc::new(TextCollectorNodeFactory));

    // Register remote pipeline node
    registry.register(Arc::new(RemotePipelineNodeFactory));

    // Register Silero VAD node (Rust ONNX)
    #[cfg(feature = "silero-vad")]
    registry.register(Arc::new(SileroVADNodeFactory));

    // Register Whisper transcription nodes
    registry.register(Arc::new(RustWhisperNodeFactory));  // Rust rwhisper
    registry.register(Arc::new(WhisperXNodeFactory));     // Python WhisperX with alignment
    registry.register(Arc::new(HFWhisperNodeFactory));    // Python HuggingFace with word timestamps

    // Register Speculative VAD Gate (Spec 007 - low-latency streaming)
    registry.register(Arc::new(SpeculativeVADGateFactory));

    // Register Speculative VAD Coordinator (integrates forwarding + Silero VAD)
    registry.register(Arc::new(SpeculativeVADCoordinatorFactory));

    // Register Python TTS nodes
    registry.register(Arc::new(KokoroTTSNodeFactory));
    registry.register(Arc::new(VibeVoiceTTSNodeFactory));

    // Register Python speech-to-speech nodes
    registry.register(Arc::new(LFM2AudioNodeFactory));

    // Register Python test nodes
    registry.register(Arc::new(SimplePyTorchNodeFactory));

    // Register Python streaming test nodes for integration tests
    registry.register(Arc::new(ExpanderNodeFactory));
    registry.register(Arc::new(RangeGeneratorNodeFactory));
    registry.register(Arc::new(TransformAndExpandNodeFactory));
    registry.register(Arc::new(ChainedTransformNodeFactory));
    registry.register(Arc::new(ConditionalExpanderNodeFactory));
    registry.register(Arc::new(FilterNodeFactory));

    // Register output formatters
    registry.register(Arc::new(SrtOutputNodeFactory));

    registry
}

/// SRT output node that converts Whisper JSON segments to SRT subtitle format
struct SrtOutputStreamingNode {
    include_numbers: bool,
    max_line_length: usize,
    segment_counter: std::sync::atomic::AtomicUsize,
}

impl SrtOutputStreamingNode {
    fn new(include_numbers: bool, max_line_length: usize) -> Self {
        Self {
            include_numbers,
            max_line_length,
            segment_counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn seconds_to_timecode(seconds: f64) -> String {
        let total_ms = (seconds * 1000.0).round() as u64;
        let ms = total_ms % 1000;
        let total_secs = total_ms / 1000;
        let secs = total_secs % 60;
        let total_mins = total_secs / 60;
        let mins = total_mins % 60;
        let hours = total_mins / 60;
        format!("{:02}:{:02}:{:02},{:03}", hours, mins, secs, ms)
    }

    fn wrap_text(text: &str, max_len: usize) -> String {
        if max_len == 0 || text.len() <= max_len {
            return text.to_string();
        }

        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in text.split_whitespace() {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= max_len {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines.join("\n")
    }
}

#[async_trait::async_trait]
impl crate::nodes::AsyncStreamingNode for SrtOutputStreamingNode {
    fn node_type(&self) -> &str {
        "SrtOutput"
    }

    async fn initialize(&self) -> Result<(), Error> {
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let json = match &data {
            RuntimeData::Json(j) => j.clone(),
            RuntimeData::Text(t) => {
                // Plain text - wrap in a single segment
                serde_json::json!({
                    "text": t,
                    "segments": [{"start": 0.0, "end": 10.0, "text": t}]
                })
            }
            _ => {
                return Err(Error::Execution(format!(
                    "SrtOutput expects JSON or Text, got: {}",
                    data.data_type()
                )));
            }
        };

        let mut srt_output = String::new();

        // Check if this is a WordUpdate from Python HFWhisper (has "word" field)
        if json.get("word").is_some() {
            // Single word update - accumulate for streaming mode
            // For now, just format it as a single subtitle
            let word = json.get("word").and_then(|v| v.as_str()).unwrap_or("");
            let start = json.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let end = json.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
            
            if !word.trim().is_empty() {
                let counter = self.segment_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                let start_tc = Self::seconds_to_timecode(start);
                let end_tc = Self::seconds_to_timecode(end);
                
                if self.include_numbers {
                    srt_output.push_str(&format!(
                        "{}\n{} --> {}\n{}\n\n",
                        counter, start_tc, end_tc, word.trim()
                    ));
                } else {
                    srt_output.push_str(&format!(
                        "{} --> {}\n{}\n\n",
                        start_tc, end_tc, word.trim()
                    ));
                }
            }
        } else if let Some(segments) = json.get("segments").and_then(|s| s.as_array()) {
            // Standard segments format from Rust Whisper
            for segment in segments {
                let start = segment.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let end = segment.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let text = segment.get("text").and_then(|v| v.as_str()).unwrap_or("");

                if !text.trim().is_empty() {
                    let counter = self.segment_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    let start_tc = Self::seconds_to_timecode(start);
                    let end_tc = Self::seconds_to_timecode(end);
                    let formatted_text = Self::wrap_text(text.trim(), self.max_line_length);

                    if self.include_numbers {
                        srt_output.push_str(&format!(
                            "{}\n{} --> {}\n{}\n\n",
                            counter, start_tc, end_tc, formatted_text
                        ));
                    } else {
                        srt_output.push_str(&format!(
                            "{} --> {}\n{}\n\n",
                            start_tc, end_tc, formatted_text
                        ));
                    }
                }
            }
        } else if let Some(chunks) = json.get("chunks").and_then(|c| c.as_array()) {
            // HuggingFace Whisper chunks format with word-level timestamps
            // Group words into subtitle segments (5-7 seconds each)
            let word_chunks: Vec<(f64, f64, String)> = chunks.iter()
                .filter_map(|chunk| {
                    let text = chunk.get("text").and_then(|t| t.as_str())?;
                    let timestamp = chunk.get("timestamp").and_then(|t| t.as_array())?;
                    let start = timestamp.get(0).and_then(|v| v.as_f64())?;
                    let end = timestamp.get(1).and_then(|v| v.as_f64())?;
                    Some((start, end, text.to_string()))
                })
                .collect();
            
            // Group words into subtitle segments
            let segments = group_words_into_subtitles(&word_chunks, 5.0, 10.0);
            
            for segment in segments {
                let start = segment.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let end = segment.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let text = segment.get("text").and_then(|v| v.as_str()).unwrap_or("");

                if !text.trim().is_empty() {
                    let counter = self.segment_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    let start_tc = Self::seconds_to_timecode(start);
                    let end_tc = Self::seconds_to_timecode(end);
                    let formatted_text = Self::wrap_text(text.trim(), self.max_line_length);

                    if self.include_numbers {
                        srt_output.push_str(&format!(
                            "{}\n{} --> {}\n{}\n\n",
                            counter, start_tc, end_tc, formatted_text
                        ));
                    } else {
                        srt_output.push_str(&format!(
                            "{} --> {}\n{}\n\n",
                            start_tc, end_tc, formatted_text
                        ));
                    }
                }
            }
        }

        Ok(RuntimeData::Text(srt_output))
    }
}

struct SrtOutputNodeFactory;

impl StreamingNodeFactory for SrtOutputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let include_numbers = params
            .get("include_numbers")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let max_line_length = params
            .get("max_line_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let node = SrtOutputStreamingNode::new(include_numbers, max_line_length);
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "SrtOutput"
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("SrtOutput")
                .description("Converts Whisper JSON output to SRT subtitle format")
                .category("utility")
                .accepts([RuntimeDataType::Json, RuntimeDataType::Text])
                .produces([RuntimeDataType::Text])
        )
    }
}
