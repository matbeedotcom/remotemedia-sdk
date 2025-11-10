//! HTTP/REST client implementation for remote pipeline execution
//!
//! This module provides an HTTP client that implements the `PipelineClient` trait
//! for executing pipelines on remote HTTP/REST servers.
//!
//! # Features
//!
//! - Unary execution via POST /execute
//! - Health checks via GET /health
//! - Authentication via Authorization header
//! - JSON serialization of manifests and data
//!
//! # Usage
//!
//! ```ignore
//! use remotemedia_runtime_core::transport::client::http::HttpPipelineClient;
//!
//! let client = HttpPipelineClient::new("http://localhost:8080", None).await?;
//! let result = client.execute_unary(manifest, input).await?;
//! ```

use crate::manifest::Manifest;
use crate::transport::client::{ClientStreamSession, PipelineClient};
use crate::transport::TransportData;
use crate::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// HTTP client for remote pipeline execution
///
/// Connects to a remotemedia HTTP/REST server and executes pipelines remotely.
pub struct HttpPipelineClient {
    /// Base URL (e.g., "http://localhost:8080" or "https://api.example.com")
    base_url: String,

    /// Optional authentication token
    auth_token: Option<String>,

    /// Reqwest HTTP client
    client: reqwest::Client,
}

impl HttpPipelineClient {
    /// Create a new HTTP client
    ///
    /// # Arguments
    ///
    /// * `base_url` - Server base URL (e.g., "http://localhost:8080")
    /// * `auth_token` - Optional authentication token
    ///
    /// # Returns
    ///
    /// * `Ok(HttpPipelineClient)` - Client created successfully
    /// * `Err(Error)` - Failed to create client
    ///
    /// # Example
    ///
    /// ```ignore
    /// let client = HttpPipelineClient::new("http://localhost:8080", None).await?;
    /// ```
    pub async fn new(base_url: impl Into<String>, auth_token: Option<String>) -> Result<Self> {
        let base_url = base_url.into();

        // Validate base URL format
        if base_url.is_empty() {
            return Err(Error::ConfigError(
                "HTTP base_url cannot be empty".to_string(),
            ));
        }

        // Ensure base URL starts with http:// or https://
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err(Error::ConfigError(format!(
                "HTTP base_url must start with http:// or https://, got: {}",
                base_url
            )));
        }

        // Create reqwest client with timeout
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Transport(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            base_url,
            auth_token,
            client,
        })
    }

    /// Build authorization header if token is present
    fn build_auth_header(&self) -> Option<String> {
        self.auth_token
            .as_ref()
            .map(|token| format!("Bearer {}", token))
    }
}

/// Request body for /execute endpoint
#[derive(Debug, Serialize)]
struct ExecuteRequest {
    /// Pipeline manifest
    manifest: serde_json::Value,
    /// Input data
    input: TransportData,
}

/// Response body from /execute endpoint
#[derive(Debug, Deserialize)]
struct ExecuteResponse {
    /// Output data
    output: TransportData,
}

#[async_trait]
impl PipelineClient for HttpPipelineClient {
    /// Execute a pipeline with unary semantics
    ///
    /// Sends POST request to /execute endpoint with manifest and input data.
    async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        let url = format!("{}/execute", self.base_url);

        // Serialize manifest to JSON
        let manifest_json = serde_json::to_value(manifest.as_ref()).map_err(|e| {
            Error::Transport(format!("Failed to serialize manifest: {}", e))
        })?;

        let request_body = ExecuteRequest {
            manifest: manifest_json,
            input,
        };

        // Build request
        let mut request = self.client.post(&url).json(&request_body);

        // Add auth header if present
        if let Some(auth_header) = self.build_auth_header() {
            request = request.header("authorization", auth_header);
        }

        // Send request
        let response = request.send().await.map_err(|e| {
            Error::RemoteExecutionFailed(format!("HTTP request failed: {}", e))
        })?;

        // Check status
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::RemoteExecutionFailed(format!(
                "HTTP {} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown"),
                error_text
            )));
        }

        // Parse response
        let execute_response: ExecuteResponse = response.json().await.map_err(|e| {
            Error::RemoteExecutionFailed(format!("Failed to parse response: {}", e))
        })?;

        Ok(execute_response.output)
    }

    /// Create a streaming session
    ///
    /// HTTP streaming is optional and not fully implemented.
    /// Returns a stub session that can be extended later.
    async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn ClientStreamSession>> {
        // HTTP streaming could be implemented via Server-Sent Events or WebSockets
        // For now, return a stub
        Ok(Box::new(HttpStreamSession::new(
            format!("http-session-{}", uuid::Uuid::new_v4()),
            self.base_url.clone(),
            self.auth_token.clone(),
            manifest,
        )))
    }

    /// Check if the remote endpoint is healthy
    ///
    /// Sends GET request to /health endpoint.
    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);

        // Build request
        let mut request = self.client.get(&url);

        // Add auth header if present (optional for health checks)
        if let Some(auth_header) = self.build_auth_header() {
            request = request.header("authorization", auth_header);
        }

        // Send request with short timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            request.send()
        ).await {
            Ok(Ok(response)) => Ok(response.status().is_success()),
            Ok(Err(e)) => {
                tracing::warn!("Health check failed for {}: {}", self.base_url, e);
                Ok(false)
            }
            Err(_) => {
                tracing::warn!("Health check timeout for {}", self.base_url);
                Ok(false)
            }
        }
    }
}

/// HTTP streaming session (stub implementation)
///
/// HTTP streaming could be implemented via:
/// - Server-Sent Events (SSE) for unidirectional streaming
/// - WebSockets for bidirectional streaming
/// - HTTP/2 streams
///
/// For now, this is a stub that returns "not implemented" errors.
pub struct HttpStreamSession {
    /// Unique session ID
    session_id: String,

    /// Base URL
    _base_url: String,

    /// Auth token
    _auth_token: Option<String>,

    /// Manifest
    _manifest: Arc<Manifest>,

    /// Whether the session is active
    active: bool,
}

impl HttpStreamSession {
    /// Create a new HTTP stream session
    pub fn new(
        session_id: String,
        base_url: String,
        auth_token: Option<String>,
        manifest: Arc<Manifest>,
    ) -> Self {
        Self {
            session_id,
            _base_url: base_url,
            _auth_token: auth_token,
            _manifest: manifest,
            active: true,
        }
    }
}

#[async_trait]
impl ClientStreamSession for HttpStreamSession {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn send(&mut self, _data: TransportData) -> Result<()> {
        if !self.active {
            return Err(Error::Transport("Session is closed".to_string()));
        }

        // TODO: Implement HTTP streaming (SSE/WebSocket/HTTP2)
        Err(Error::Transport(
            "HTTP streaming not yet implemented - use execute_unary() for request/response".to_string(),
        ))
    }

    async fn receive(&mut self) -> Result<Option<TransportData>> {
        if !self.active {
            return Ok(None);
        }

        // TODO: Implement HTTP streaming (SSE/WebSocket/HTTP2)
        Err(Error::Transport(
            "HTTP streaming not yet implemented - use execute_unary() for request/response".to_string(),
        ))
    }

    async fn close(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        self.active = false;
        tracing::info!("HTTP stream session {} closed", self.session_id);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_client() {
        let client = HttpPipelineClient::new("http://localhost:8080", None).await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_create_client_with_auth() {
        let client = HttpPipelineClient::new(
            "https://api.example.com",
            Some("test-token".to_string()),
        )
        .await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_empty_url_error() {
        let client = HttpPipelineClient::new("", None).await;
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_invalid_url_scheme() {
        let client = HttpPipelineClient::new("ftp://invalid.com", None).await;
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_health_check_unreachable() {
        let client = HttpPipelineClient::new("http://localhost:9999", None)
            .await
            .unwrap();
        let result = client.health_check().await;
        // Should return Ok(false) for unreachable endpoint
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
