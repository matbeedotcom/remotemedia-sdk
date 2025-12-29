//! LLM client abstraction for the pipeline controller

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Error;

/// Request to an LLM
#[derive(Debug, Clone, Serialize)]
pub struct LlmRequest {
    /// System prompt
    pub system: String,

    /// User message
    pub user: String,

    /// Temperature (0.0 - 1.0, lower = more deterministic)
    pub temperature: f32,

    /// Maximum tokens in response
    pub max_tokens: u32,
}

/// Response from an LLM
#[derive(Debug, Clone, Deserialize)]
pub struct LlmResponse {
    /// Generated text
    pub text: String,

    /// Number of input tokens used
    pub input_tokens: Option<u32>,

    /// Number of output tokens generated
    pub output_tokens: Option<u32>,

    /// Model used
    pub model: Option<String>,
}

/// Trait for LLM clients
///
/// This abstraction allows swapping between different LLM providers
/// (Anthropic, OpenAI, local models, etc.) or using mock clients for testing.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Send a completion request to the LLM
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, Error>;

    /// Get the name of this client (for logging)
    fn name(&self) -> &str;
}

/// Mock LLM client for testing
///
/// Returns predetermined responses based on configured rules.
pub struct MockLlmClient {
    /// Default response to return
    pub default_response: String,

    /// Specific responses for patterns in the user message
    pub pattern_responses: Vec<(String, String)>,
}

impl MockLlmClient {
    pub fn new() -> Self {
        Self {
            default_response: r#"{"reasoning": "Pipeline is healthy", "issues": [], "action": {"action_type": "NoOp", "reason": "All metrics within normal range"}}"#.to_string(),
            pattern_responses: Vec::new(),
        }
    }

    /// Add a pattern-based response
    pub fn with_pattern(mut self, pattern: &str, response: &str) -> Self {
        self.pattern_responses.push((pattern.to_string(), response.to_string()));
        self
    }

    /// Configure to return RestartProcess for failed nodes
    pub fn with_restart_on_failure(self) -> Self {
        self.with_pattern(
            "Failed",
            r#"{"reasoning": "Node has failed, restarting", "issues": ["Node failure detected"], "action": {"action_type": "RestartProcess", "node_id": "failed_node", "reason": "Process crashed"}}"#
        )
    }
}

impl Default for MockLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, Error> {
        // Check for pattern matches
        for (pattern, response) in &self.pattern_responses {
            if request.user.contains(pattern) {
                return Ok(LlmResponse {
                    text: response.clone(),
                    input_tokens: Some(100),
                    output_tokens: Some(50),
                    model: Some("mock".to_string()),
                });
            }
        }

        Ok(LlmResponse {
            text: self.default_response.clone(),
            input_tokens: Some(100),
            output_tokens: Some(50),
            model: Some("mock".to_string()),
        })
    }

    fn name(&self) -> &str {
        "mock"
    }
}

/// Anthropic Claude client
#[cfg(feature = "anthropic")]
pub mod anthropic {
    use super::*;

    pub struct AnthropicClient {
        api_key: String,
        model: String,
        http_client: reqwest::Client,
    }

    impl AnthropicClient {
        pub fn new(api_key: String, model: String) -> Self {
            Self {
                api_key,
                model,
                http_client: reqwest::Client::new(),
            }
        }

        pub fn from_env() -> Result<Self, Error> {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| Error::Execution("ANTHROPIC_API_KEY not set".to_string()))?;

            let model = std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

            Ok(Self::new(api_key, model))
        }
    }

    #[derive(Serialize)]
    struct AnthropicRequest {
        model: String,
        max_tokens: u32,
        temperature: f32,
        system: String,
        messages: Vec<AnthropicMessage>,
    }

    #[derive(Serialize)]
    struct AnthropicMessage {
        role: String,
        content: String,
    }

    #[derive(Deserialize)]
    struct AnthropicResponse {
        content: Vec<ContentBlock>,
        usage: Usage,
        model: String,
    }

    #[derive(Deserialize)]
    struct ContentBlock {
        text: String,
    }

    #[derive(Deserialize)]
    struct Usage {
        input_tokens: u32,
        output_tokens: u32,
    }

    #[async_trait]
    impl LlmClient for AnthropicClient {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, Error> {
            let api_request = AnthropicRequest {
                model: self.model.clone(),
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                system: request.system,
                messages: vec![AnthropicMessage {
                    role: "user".to_string(),
                    content: request.user,
                }],
            };

            let response = self
                .http_client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&api_request)
                .send()
                .await
                .map_err(|e| Error::Execution(format!("Anthropic API request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(Error::Execution(format!(
                    "Anthropic API error {}: {}",
                    status, body
                )));
            }

            let api_response: AnthropicResponse = response
                .json()
                .await
                .map_err(|e| Error::Execution(format!("Failed to parse Anthropic response: {}", e)))?;

            let text = api_response
                .content
                .into_iter()
                .map(|b| b.text)
                .collect::<Vec<_>>()
                .join("");

            Ok(LlmResponse {
                text,
                input_tokens: Some(api_response.usage.input_tokens),
                output_tokens: Some(api_response.usage.output_tokens),
                model: Some(api_response.model),
            })
        }

        fn name(&self) -> &str {
            "anthropic"
        }
    }
}

/// Rate-limited wrapper for any LLM client
pub struct RateLimitedClient<C: LlmClient> {
    inner: C,
    min_interval: std::time::Duration,
    last_call: std::sync::Mutex<Option<std::time::Instant>>,
}

impl<C: LlmClient> RateLimitedClient<C> {
    pub fn new(inner: C, min_interval: std::time::Duration) -> Self {
        Self {
            inner,
            min_interval,
            last_call: std::sync::Mutex::new(None),
        }
    }
}

#[async_trait]
impl<C: LlmClient> LlmClient for RateLimitedClient<C> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, Error> {
        // Check if we need to wait
        {
            let mut last = self.last_call.lock().unwrap();
            if let Some(last_time) = *last {
                let elapsed = last_time.elapsed();
                if elapsed < self.min_interval {
                    let wait = self.min_interval - elapsed;
                    tokio::time::sleep(wait).await;
                }
            }
            *last = Some(std::time::Instant::now());
        }

        self.inner.complete(request).await
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

/// Cached LLM client that reuses responses for identical requests
pub struct CachedClient<C: LlmClient> {
    inner: C,
    cache: std::sync::Mutex<std::collections::HashMap<String, LlmResponse>>,
    max_cache_size: usize,
}

impl<C: LlmClient> CachedClient<C> {
    pub fn new(inner: C, max_cache_size: usize) -> Self {
        Self {
            inner,
            cache: std::sync::Mutex::new(std::collections::HashMap::new()),
            max_cache_size,
        }
    }

    fn cache_key(request: &LlmRequest) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        request.system.hash(&mut hasher);
        request.user.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[async_trait]
impl<C: LlmClient> LlmClient for CachedClient<C> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, Error> {
        let key = Self::cache_key(&request);

        // Check cache
        {
            let cache = self.cache.lock().unwrap();
            if let Some(response) = cache.get(&key) {
                return Ok(response.clone());
            }
        }

        // Make request
        let response = self.inner.complete(request).await?;

        // Cache response
        {
            let mut cache = self.cache.lock().unwrap();
            if cache.len() >= self.max_cache_size {
                // Simple eviction: remove a random entry
                if let Some(old_key) = cache.keys().next().cloned() {
                    cache.remove(&old_key);
                }
            }
            cache.insert(key, response.clone());
        }

        Ok(response)
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_client_default() {
        let client = MockLlmClient::new();
        let response = client
            .complete(LlmRequest {
                system: "test".to_string(),
                user: "test".to_string(),
                temperature: 0.1,
                max_tokens: 100,
            })
            .await
            .unwrap();

        assert!(response.text.contains("NoOp"));
    }

    #[tokio::test]
    async fn test_mock_client_pattern() {
        let client = MockLlmClient::new()
            .with_pattern("error", r#"{"action": "fix"}"#);

        let response = client
            .complete(LlmRequest {
                system: "test".to_string(),
                user: "there is an error".to_string(),
                temperature: 0.1,
                max_tokens: 100,
            })
            .await
            .unwrap();

        assert!(response.text.contains("fix"));
    }
}
