/// Text Collector Node
///
/// Accumulates streaming text tokens and yields complete sentences based on punctuation.
/// Works with streaming text generation (e.g., LFM2-Audio text output).
///
/// Pipeline flow:
///   LFM2Audio → [Text tokens] → TextCollector → [Complete sentences] → Next node
///
/// The collector receives:
/// - Individual text tokens/chunks
/// - Special tokens like <|text_end|>
///
/// When a sentence boundary is detected (. ! ? , ; :), it outputs the accumulated text.

use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Buffer state for a single session
#[derive(Debug, Clone)]
struct TextBufferState {
    /// Accumulated text buffer
    accumulated_text: String,
    /// Total chunks accumulated
    chunks_accumulated: usize,
}

impl Default for TextBufferState {
    fn default() -> Self {
        Self {
            accumulated_text: String::new(),
            chunks_accumulated: 0,
        }
    }
}

/// Text Collector Node
pub struct TextCollectorNode {
    /// Sentence boundary characters (default: .!?,;:\n)
    boundary_chars: Vec<char>,

    /// Minimum sentence length before yielding (characters)
    min_sentence_length: usize,

    /// Yield partial sentences when <|text_end|> is received
    yield_partial_on_end: bool,

    /// Buffer states per session
    states: Arc<Mutex<std::collections::HashMap<String, TextBufferState>>>,
}

impl TextCollectorNode {
    pub fn new(
        split_pattern: Option<String>,
        min_sentence_length: Option<usize>,
        yield_partial_on_end: Option<bool>,
    ) -> Result<Self> {
        // Parse split pattern to extract boundary characters
        // Default: [.!?;\n]+ (no commas - we want to keep them in sentences)
        let boundary_chars = if let Some(pattern) = split_pattern {
            // Simple parsing: extract characters from pattern like [.!?;\n]+
            pattern.chars()
                .filter(|c| !['[', ']', '+', '\\', 'n', 'r', 't'].contains(c))
                .collect()
        } else {
            vec!['.', '!', '?', ';', '\n']
        };

        Ok(Self {
            boundary_chars,
            min_sentence_length: min_sentence_length.unwrap_or(3),
            yield_partial_on_end: yield_partial_on_end.unwrap_or(true),
            states: Arc::new(Mutex::new(std::collections::HashMap::new())),
        })
    }

    fn is_boundary_char(&self, c: char) -> bool {
        self.boundary_chars.contains(&c)
    }

    fn extract_complete_sentences(&self, buffer: &str) -> (Vec<String>, String) {
        let mut sentences = Vec::new();
        let mut current_sentence = String::new();
        let chars: Vec<char> = buffer.chars().collect();

        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            current_sentence.push(c);

            // Check if this is a boundary character
            if self.is_boundary_char(c) {
                // Include consecutive boundary chars in the sentence
                while i + 1 < chars.len() && self.is_boundary_char(chars[i + 1]) {
                    i += 1;
                    current_sentence.push(chars[i]);
                }

                // We've found a complete sentence
                // Only trim leading whitespace, preserve trailing punctuation spacing
                let sentence = current_sentence.trim_start().to_string();
                if sentence.len() >= self.min_sentence_length {
                    sentences.push(sentence);
                }
                current_sentence.clear();
            }

            i += 1;
        }

        // Remainder (incomplete sentence) - don't trim to preserve spacing
        let remainder = current_sentence;

        (sentences, remainder)
    }
}

#[async_trait]
impl AsyncStreamingNode for TextCollectorNode {
    fn node_type(&self) -> &str {
        "TextCollectorNode"
    }

    async fn process(&self, _data: RuntimeData) -> Result<RuntimeData> {
        Err(Error::Execution(
            "TextCollectorNode requires streaming mode - use process_streaming() instead".into()
        ))
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize>
    where
        F: FnMut(RuntimeData) -> Result<()> + Send,
    {
        let session_key = session_id.clone().unwrap_or_else(|| "default".to_string());

        // Only process text data
        let text_chunk = match &data {
            RuntimeData::Text(text_string) => {
                text_string.clone()
            }
            _ => {
                // Pass through non-text data unchanged
                tracing::debug!("[TextCollector] Passing through non-text data: {:?}", data.data_type());
                callback(data)?;
                return Ok(1);
            }
        };

        tracing::debug!("[TextCollector] Session {}: Received text chunk: '{}'", session_key, text_chunk);

        let mut states = self.states.lock().await;
        let state = states.entry(session_key.clone()).or_insert_with(TextBufferState::default);

        // Check for special end tokens
        let has_text_end = text_chunk.contains("<|text_end|>");
        let has_audio_end = text_chunk.contains("<|audio_end|>");

        // Remove special tokens from the text (don't trim to preserve spacing)
        let cleaned_text = text_chunk
            .replace("<|text_end|>", "")
            .replace("<|audio_end|>", "")
            .to_string();

        // Add to buffer
        state.accumulated_text.push_str(&cleaned_text);
        state.chunks_accumulated += 1;

        tracing::debug!(
            "[TextCollector] Session {}: Buffer now: '{}' ({} chars)",
            session_key,
            state.accumulated_text,
            state.accumulated_text.len()
        );

        // Extract complete sentences
        let (sentences, remainder) = self.extract_complete_sentences(&state.accumulated_text);

        let mut output_count = 0;

        // Yield complete sentences
        for sentence in sentences {
            tracing::info!("[TextCollector] Session {}: Yielding sentence: '{}'", session_key, sentence);
            callback(RuntimeData::Text(sentence))?;
            output_count += 1;
        }

        // Update buffer with remainder
        state.accumulated_text = remainder;

        // Handle end tokens
        if has_text_end || has_audio_end {
            // Yield any remaining partial sentence if configured
            if self.yield_partial_on_end && !state.accumulated_text.is_empty() {
                tracing::info!(
                    "[TextCollector] Session {}: Yielding partial on end: '{}'",
                    session_key,
                    state.accumulated_text
                );
                callback(RuntimeData::Text(state.accumulated_text.clone()))?;
                output_count += 1;
                state.accumulated_text.clear();
            }

            // Pass through end tokens
            if has_text_end {
                tracing::info!("[TextCollector] Session {}: Passing through <|text_end|>", session_key);
                callback(RuntimeData::Text("<|text_end|>".to_string()))?;
                output_count += 1;
            }
            if has_audio_end {
                tracing::info!("[TextCollector] Session {}: Passing through <|audio_end|>", session_key);
                callback(RuntimeData::Text("<|audio_end|>".to_string()))?;
                output_count += 1;
            }

            // Reset state for next message
            state.accumulated_text.clear();
            state.chunks_accumulated = 0;
        }

        Ok(output_count)
    }
}
