//! SRT (SubRip) subtitle output node
//!
//! Converts Whisper transcription output (with segments and timestamps)
//! into SRT subtitle format.
//!
//! # Input Format
//!
//! Expects JSON from WhisperSTT node:
//! ```json
//! {
//!   "text": "Full transcription text",
//!   "segments": [
//!     {"start": 0.0, "end": 2.5, "text": "First segment"},
//!     {"start": 2.5, "end": 5.0, "text": "Second segment"}
//!   ]
//! }
//! ```
//!
//! # Output Format
//!
//! SRT format:
//! ```text
//! 1
//! 00:00:00,000 --> 00:00:02,500
//! First segment
//!
//! 2
//! 00:00:02,500 --> 00:00:05,000
//! Second segment
//! ```

use async_trait::async_trait;
use remotemedia_core::capabilities::{
    ConstraintValue, MediaCapabilities, MediaConstraints, TextConstraints,
};
use remotemedia_core::executor::node_executor::{NodeContext, NodeExecutor};
use remotemedia_core::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Configuration for SrtOutputNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtOutputConfig {
    /// Include sequence numbers (1, 2, 3, ...)
    #[serde(default = "default_include_numbers")]
    pub include_numbers: bool,

    /// Maximum characters per subtitle line (0 = no limit)
    #[serde(default)]
    pub max_line_length: usize,
}

fn default_include_numbers() -> bool {
    true
}

impl Default for SrtOutputConfig {
    fn default() -> Self {
        Self {
            include_numbers: true,
            max_line_length: 0,
        }
    }
}

/// SRT subtitle output node
///
/// Converts transcription segments with timestamps to SRT format.
pub struct SrtOutputNode {
    config: SrtOutputConfig,
    segment_counter: usize,
}

impl SrtOutputNode {
    /// Create a new SrtOutputNode with default config
    pub fn new() -> Self {
        Self {
            config: SrtOutputConfig::default(),
            segment_counter: 0,
        }
    }

    /// Create from JSON params
    pub fn from_params(params: Value) -> Self {
        let config: SrtOutputConfig = serde_json::from_value(params).unwrap_or_default();
        Self {
            config,
            segment_counter: 0,
        }
    }

    /// Returns the media capabilities for this node (spec 022).
    ///
    /// **Input requirements:**
    /// - Text: UTF-8 JSON format (Whisper output with segments)
    ///
    /// **Output capabilities:**
    /// - Text: UTF-8 plain text (SRT subtitle format)
    pub fn media_capabilities() -> MediaCapabilities {
        MediaCapabilities::with_input_output(
            // Input: JSON from Whisper
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("json".to_string())),
            }),
            // Output: SRT plain text
            MediaConstraints::Text(TextConstraints {
                encoding: Some(ConstraintValue::Exact("utf-8".to_string())),
                format: Some(ConstraintValue::Exact("srt".to_string())),
            }),
        )
    }

    /// Format a single segment as SRT
    fn format_segment(&mut self, start: f64, end: f64, text: &str) -> String {
        self.segment_counter += 1;

        let start_tc = Self::seconds_to_timecode(start);
        let end_tc = Self::seconds_to_timecode(end);

        let text = text.trim();
        let formatted_text = if self.config.max_line_length > 0 {
            Self::wrap_text(text, self.config.max_line_length)
        } else {
            text.to_string()
        };

        if self.config.include_numbers {
            format!(
                "{}\n{} --> {}\n{}\n",
                self.segment_counter, start_tc, end_tc, formatted_text
            )
        } else {
            format!("{} --> {}\n{}\n", start_tc, end_tc, formatted_text)
        }
    }

    /// Convert seconds to SRT timecode format (HH:MM:SS,mmm)
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

    /// Wrap text to max line length
    fn wrap_text(text: &str, max_len: usize) -> String {
        if text.len() <= max_len {
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

    /// Process a Whisper output with segments
    fn process_whisper_output(&mut self, input: &Value) -> Result<String> {
        let mut srt_output = String::new();

        // Check if this is a Whisper output with segments
        if let Some(segments) = input.get("segments").and_then(|s| s.as_array()) {
            for segment in segments {
                let start = segment
                    .get("start")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let end = segment.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let text = segment
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if !text.trim().is_empty() {
                    srt_output.push_str(&self.format_segment(start, end, text));
                    srt_output.push('\n');
                }
            }
        } else if let Some(text) = input.get("text").and_then(|t| t.as_str()) {
            // Fallback: single text without segments (use 0-10s as default)
            srt_output.push_str(&self.format_segment(0.0, 10.0, text));
            srt_output.push('\n');
        } else if let Some(text) = input.as_str() {
            // Plain text input
            srt_output.push_str(&self.format_segment(0.0, 10.0, text));
            srt_output.push('\n');
        }

        Ok(srt_output)
    }
}

impl Default for SrtOutputNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeExecutor for SrtOutputNode {
    async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> {
        // Re-parse config from context params
        if let Ok(config) = serde_json::from_value::<SrtOutputConfig>(ctx.params.clone()) {
            self.config = config;
        }
        self.segment_counter = 0;

        tracing::info!(
            "SrtOutputNode initialized (include_numbers={}, max_line_length={})",
            self.config.include_numbers,
            self.config.max_line_length
        );
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        let srt = self.process_whisper_output(&input)?;

        if srt.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![Value::String(srt)])
        }
    }

    async fn cleanup(&mut self) -> Result<()> {
        tracing::info!(
            "SrtOutputNode cleanup: processed {} segments",
            self.segment_counter
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timecode_conversion() {
        assert_eq!(SrtOutputNode::seconds_to_timecode(0.0), "00:00:00,000");
        assert_eq!(SrtOutputNode::seconds_to_timecode(1.5), "00:00:01,500");
        assert_eq!(SrtOutputNode::seconds_to_timecode(61.234), "00:01:01,234");
        assert_eq!(SrtOutputNode::seconds_to_timecode(3661.0), "01:01:01,000");
    }

    #[test]
    fn test_wrap_text() {
        let text = "This is a long subtitle that should be wrapped";
        let wrapped = SrtOutputNode::wrap_text(text, 20);
        assert!(wrapped.contains('\n'));

        let short = "Short text";
        let not_wrapped = SrtOutputNode::wrap_text(short, 20);
        assert!(!not_wrapped.contains('\n'));
    }

    #[test]
    fn test_format_segment() {
        let mut node = SrtOutputNode::new();
        let srt = node.format_segment(1.5, 4.0, "Hello world");

        assert!(srt.contains("1\n"));
        assert!(srt.contains("00:00:01,500 --> 00:00:04,000"));
        assert!(srt.contains("Hello world"));
    }

    #[tokio::test]
    async fn test_process_whisper_output() {
        let mut node = SrtOutputNode::new();

        let input = serde_json::json!({
            "text": "Hello world. How are you?",
            "segments": [
                {"start": 0.0, "end": 2.0, "text": "Hello world."},
                {"start": 2.0, "end": 4.0, "text": "How are you?"}
            ]
        });

        let result = node.process(input).await.unwrap();
        assert_eq!(result.len(), 1);

        let srt = result[0].as_str().unwrap();
        assert!(srt.contains("1\n00:00:00,000 --> 00:00:02,000"));
        assert!(srt.contains("Hello world."));
        assert!(srt.contains("2\n00:00:02,000 --> 00:00:04,000"));
        assert!(srt.contains("How are you?"));
    }

    #[test]
    fn test_media_capabilities() {
        let caps = SrtOutputNode::media_capabilities();

        // Check input constraints (JSON from Whisper)
        let input = caps.default_input().expect("Should have default input");
        match input {
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
            _ => panic!("Expected Text input constraints"),
        }

        // Check output constraints (SRT text)
        let output = caps.default_output().expect("Should have default output");
        match output {
            MediaConstraints::Text(text) => {
                assert_eq!(
                    text.encoding,
                    Some(ConstraintValue::Exact("utf-8".to_string()))
                );
                assert_eq!(
                    text.format,
                    Some(ConstraintValue::Exact("srt".to_string()))
                );
            }
            _ => panic!("Expected Text output constraints"),
        }
    }
}
