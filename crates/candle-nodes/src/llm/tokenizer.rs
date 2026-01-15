//! Tokenizer utilities for LLM nodes

use crate::error::{CandleNodeError, Result};
use std::path::Path;

/// Wrapper around tokenizers crate for LLM text processing
pub struct LlmTokenizer {
    #[cfg(feature = "llm")]
    inner: tokenizers::Tokenizer,
    #[cfg(not(feature = "llm"))]
    _phantom: std::marker::PhantomData<()>,
    eos_id: Option<u32>,
    bos_id: Option<u32>,
}

impl LlmTokenizer {
    /// Load tokenizer from a file path
    #[cfg(feature = "llm")]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let inner = tokenizers::Tokenizer::from_file(path).map_err(|e| {
            CandleNodeError::model_load(
                path.display().to_string(),
                format!("Failed to load tokenizer: {}", e),
            )
        })?;
        
        let vocab = inner.get_vocab(true);
        let eos_id = Self::find_special_token(&vocab, &["[EOS]", "[SEP]", "eos_token"]);
        let bos_id = Self::find_special_token(&vocab, &["[BOS]", "[CLS]", "bos_token"]);
        
        Ok(Self { inner, eos_id, bos_id })
    }

    #[cfg(not(feature = "llm"))]
    pub fn from_file(_path: impl AsRef<Path>) -> Result<Self> {
        Err(CandleNodeError::configuration(
            "tokenizer",
            "LLM feature not enabled",
        ))
    }

    #[cfg(feature = "llm")]
    fn find_special_token(vocab: &std::collections::HashMap<String, u32>, names: &[&str]) -> Option<u32> {
        for name in names {
            if let Some(&id) = vocab.get(*name) {
                return Some(id);
            }
        }
        None
    }

    /// Encode text to token IDs
    #[cfg(feature = "llm")]
    pub fn encode(&self, text: &str, add_special_tokens: bool) -> Result<Vec<u32>> {
        let encoding = self
            .inner
            .encode(text, add_special_tokens)
            .map_err(|e| CandleNodeError::data_conversion(format!("Tokenization failed: {}", e)))?;
        Ok(encoding.get_ids().to_vec())
    }

    #[cfg(not(feature = "llm"))]
    pub fn encode(&self, _text: &str, _add_special_tokens: bool) -> Result<Vec<u32>> {
        Err(CandleNodeError::configuration(
            "tokenizer",
            "LLM feature not enabled",
        ))
    }

    /// Decode token IDs back to text
    #[cfg(feature = "llm")]
    pub fn decode(&self, tokens: &[u32], skip_special_tokens: bool) -> Result<String> {
        self.inner
            .decode(tokens, skip_special_tokens)
            .map_err(|e| CandleNodeError::data_conversion(format!("Decoding failed: {}", e)))
    }

    #[cfg(not(feature = "llm"))]
    pub fn decode(&self, _tokens: &[u32], _skip_special_tokens: bool) -> Result<String> {
        Err(CandleNodeError::configuration(
            "tokenizer",
            "LLM feature not enabled",
        ))
    }

    /// Decode a single token
    #[cfg(feature = "llm")]
    pub fn decode_token(&self, token: u32) -> Result<String> {
        self.decode(&[token], false)
    }

    #[cfg(not(feature = "llm"))]
    pub fn decode_token(&self, _token: u32) -> Result<String> {
        Err(CandleNodeError::configuration(
            "tokenizer",
            "LLM feature not enabled",
        ))
    }

    /// Get vocabulary size
    #[cfg(feature = "llm")]
    pub fn vocab_size(&self) -> usize {
        self.inner.get_vocab_size(true)
    }

    #[cfg(not(feature = "llm"))]
    pub fn vocab_size(&self) -> usize {
        0
    }

    /// Get the EOS token ID
    pub fn eos_token_id(&self) -> Option<u32> {
        self.eos_id
    }

    /// Get the BOS token ID
    pub fn bos_token_id(&self) -> Option<u32> {
        self.bos_id
    }

    /// Check if a token is an EOS token
    pub fn is_eos(&self, token: u32) -> bool {
        self.eos_id.map_or(false, |eos| eos == token)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_tokenizer_feature_gating() {
        // This test verifies compilation works regardless of feature
        assert!(true);
    }
}
