//! HTTP transport wrapper

use anyhow::Result;

/// HTTP client wrapper
pub struct HttpClient {
    endpoint: String,
    #[allow(dead_code)]
    auth_token: Option<String>,
}

impl HttpClient {
    /// Create a new HTTP client
    pub fn new(endpoint: &str, auth_token: Option<String>) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            auth_token,
        }
    }

    /// Check server health
    pub async fn health(&self) -> Result<bool> {
        // TODO: Use remotemedia-http transport
        tracing::info!("Checking HTTP server health: {}", self.endpoint);
        Ok(false)
    }

    /// Execute a pipeline
    pub async fn predict(&self, _pipeline: &str, _input: Vec<u8>) -> Result<Vec<u8>> {
        // TODO: Implement using remotemedia-http
        anyhow::bail!("HTTP execution not yet implemented")
    }
}
