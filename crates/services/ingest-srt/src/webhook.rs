//! Webhook delivery module
//!
//! Handles delivering health events to configured webhook endpoints
//! with retry logic and exponential backoff.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::sleep;

use remotemedia_health_analyzer::HealthEvent;

/// Webhook delivery configuration
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Initial backoff delay in milliseconds
    pub initial_backoff_ms: u64,

    /// Maximum backoff delay in milliseconds
    pub max_backoff_ms: u64,

    /// Request timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 10000,
            timeout_seconds: 10,
        }
    }
}

/// Webhook payload sent to the endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    /// Event type (e.g., "alert.silence", "system.stream_started")
    pub event_type: String,

    /// Session ID this event belongs to
    pub session_id: String,

    /// ISO 8601 timestamp when the event occurred
    pub timestamp: String,

    /// Time in milliseconds since stream started
    pub relative_ms: u64,

    /// Event-specific data
    pub data: serde_json::Value,
}

impl WebhookPayload {
    /// Create a webhook payload from a HealthEvent
    pub fn from_health_event(event: &HealthEvent) -> Self {
        // Extract session_id from event variants that have it
        let session_id = match event {
            HealthEvent::Drift { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::Freeze { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::Silence { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::LowVolume { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::Clipping { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::ChannelImbalance { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::Dropouts { stream_id, .. } => stream_id.clone().unwrap_or_default(),
            HealthEvent::StreamStarted { session_id, .. } => session_id.clone().unwrap_or_default(),
            HealthEvent::StreamEnded { session_id, .. } => session_id.clone().unwrap_or_default(),
            _ => String::new(),
        };

        // Extract relative_ms from events that have it
        let relative_ms = match event {
            HealthEvent::StreamStarted { relative_ms, .. } => *relative_ms,
            HealthEvent::StreamEnded { relative_ms, .. } => *relative_ms,
            _ => 0,
        };

        Self {
            event_type: event.event_type().to_string(),
            session_id,
            timestamp: event.timestamp().to_rfc3339(),
            relative_ms,
            data: serde_json::to_value(event).unwrap_or_default(),
        }
    }
}

/// Result of a webhook delivery attempt
#[derive(Debug, Clone)]
pub struct WebhookDeliveryResult {
    /// Whether delivery succeeded
    pub success: bool,

    /// HTTP status code (if request completed)
    pub status_code: Option<u16>,

    /// Number of attempts made
    pub attempts: u32,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Total time taken for all attempts
    pub total_duration_ms: u64,
}

/// Webhook sender for delivering events to a URL
pub struct WebhookSender {
    /// HTTP client
    client: Client,

    /// Target webhook URL
    url: String,

    /// Configuration
    config: WebhookConfig,

    /// Session ID for logging
    session_id: String,
}

impl WebhookSender {
    /// Create a new webhook sender
    pub fn new(url: String, session_id: String) -> Self {
        Self::with_config(url, session_id, WebhookConfig::default())
    }

    /// Create a new webhook sender with custom configuration
    pub fn with_config(url: String, session_id: String, config: WebhookConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            url,
            config,
            session_id,
        }
    }

    /// Send a health event to the webhook
    pub async fn send(&self, event: &HealthEvent) -> WebhookDeliveryResult {
        let payload = WebhookPayload::from_health_event(event);
        self.send_payload(&payload).await
    }

    /// Send a payload to the webhook with retry logic
    pub async fn send_payload(&self, payload: &WebhookPayload) -> WebhookDeliveryResult {
        let start = std::time::Instant::now();
        let mut attempts = 0;
        let mut last_error: Option<String> = None;
        let mut last_status: Option<u16> = None;

        while attempts < self.config.max_retries {
            attempts += 1;

            match self.try_send(payload).await {
                Ok(status) => {
                    let duration = start.elapsed().as_millis() as u64;

                    if status >= 200 && status < 300 {
                        tracing::info!(
                            session_id = %self.session_id,
                            url = %self.url,
                            event_type = %payload.event_type,
                            status = status,
                            attempts = attempts,
                            duration_ms = duration,
                            "Webhook delivered successfully"
                        );

                        return WebhookDeliveryResult {
                            success: true,
                            status_code: Some(status),
                            attempts,
                            error: None,
                            total_duration_ms: duration,
                        };
                    }

                    // Non-2xx response - retry if retryable
                    last_status = Some(status);
                    last_error = Some(format!("HTTP status {}", status));

                    if !Self::is_retryable_status(status) {
                        tracing::warn!(
                            session_id = %self.session_id,
                            url = %self.url,
                            event_type = %payload.event_type,
                            status = status,
                            "Webhook delivery failed with non-retryable status"
                        );
                        break;
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    tracing::warn!(
                        session_id = %self.session_id,
                        url = %self.url,
                        event_type = %payload.event_type,
                        attempt = attempts,
                        error = %e,
                        "Webhook delivery attempt failed"
                    );
                }
            }

            // Calculate backoff delay
            if attempts < self.config.max_retries {
                let backoff = self.calculate_backoff(attempts);
                sleep(Duration::from_millis(backoff)).await;
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        tracing::error!(
            session_id = %self.session_id,
            url = %self.url,
            event_type = %payload.event_type,
            attempts = attempts,
            duration_ms = duration,
            error = ?last_error,
            "Webhook delivery failed after all retries"
        );

        WebhookDeliveryResult {
            success: false,
            status_code: last_status,
            attempts,
            error: last_error,
            total_duration_ms: duration,
        }
    }

    /// Try to send a single request
    async fn try_send(&self, payload: &WebhookPayload) -> Result<u16, reqwest::Error> {
        let response = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "remotemedia-ingest-srt/1.0")
            .json(payload)
            .send()
            .await?;

        Ok(response.status().as_u16())
    }

    /// Calculate exponential backoff delay
    fn calculate_backoff(&self, attempt: u32) -> u64 {
        let base = self.config.initial_backoff_ms;
        let exponential = base * 2u64.pow(attempt - 1);

        // Add jitter (±25%)
        let jitter_range = exponential / 4;
        let jitter = if jitter_range > 0 {
            (rand_jitter() % jitter_range as u32) as u64
        } else {
            0
        };

        let with_jitter = if rand_jitter() % 2 == 0 {
            exponential.saturating_add(jitter)
        } else {
            exponential.saturating_sub(jitter)
        };

        with_jitter.min(self.config.max_backoff_ms)
    }

    /// Check if an HTTP status code is retryable
    fn is_retryable_status(status: u16) -> bool {
        // Retry on server errors (5xx) and specific client errors
        matches!(status, 408 | 429 | 500..=599)
    }
}

/// Simple jitter function using system time as entropy
fn rand_jitter() -> u32 {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    (now.subsec_nanos() ^ now.as_secs() as u32) % 1000
}

/// Webhook worker that subscribes to events and delivers them
pub struct WebhookWorker {
    /// Webhook sender
    sender: Arc<WebhookSender>,

    /// Event receiver
    event_rx: broadcast::Receiver<HealthEvent>,

    /// Shutdown signal
    shutdown_rx: Option<broadcast::Receiver<()>>,
}

impl WebhookWorker {
    /// Create a new webhook worker
    pub fn new(
        url: String,
        session_id: String,
        event_rx: broadcast::Receiver<HealthEvent>,
    ) -> Self {
        Self {
            sender: Arc::new(WebhookSender::new(url, session_id)),
            event_rx,
            shutdown_rx: None,
        }
    }

    /// Set shutdown signal receiver
    pub fn with_shutdown(mut self, shutdown_rx: broadcast::Receiver<()>) -> Self {
        self.shutdown_rx = Some(shutdown_rx);
        self
    }

    /// Run the webhook worker
    ///
    /// Listens for events and delivers them to the webhook URL.
    pub async fn run(mut self) {
        tracing::info!(
            url = %self.sender.url,
            session_id = %self.sender.session_id,
            "Webhook worker started"
        );

        loop {
            // Check for shutdown
            if let Some(ref mut shutdown_rx) = self.shutdown_rx {
                match shutdown_rx.try_recv() {
                    Ok(()) | Err(broadcast::error::TryRecvError::Closed) => {
                        tracing::info!("Webhook worker shutdown requested");
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Empty)
                    | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                }
            }

            // Receive events
            match self.event_rx.recv().await {
                Ok(event) => {
                    // Spawn delivery task to avoid blocking
                    let sender = self.sender.clone();
                    tokio::spawn(async move {
                        sender.send(&event).await;
                    });
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        session_id = %self.sender.session_id,
                        skipped = n,
                        "Webhook worker lagged, skipped events"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!(
                        session_id = %self.sender.session_id,
                        "Webhook worker event channel closed"
                    );
                    break;
                }
            }
        }

        tracing::info!(
            session_id = %self.sender.session_id,
            "Webhook worker stopped"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_payload_serialization() {
        let payload = WebhookPayload {
            event_type: "alert.silence".to_string(),
            session_id: "sess_123".to_string(),
            timestamp: "2025-01-01T00:00:30.000Z".to_string(),
            relative_ms: 30000,
            data: serde_json::json!({
                "duration_ms": 3500,
                "threshold_ms": 3000
            }),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"event_type\":\"alert.silence\""));
        assert!(json.contains("\"session_id\":\"sess_123\""));
        assert!(json.contains("\"relative_ms\":30000"));

        // Deserialize back
        let parsed: WebhookPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type, "alert.silence");
        assert_eq!(parsed.session_id, "sess_123");
        assert_eq!(parsed.relative_ms, 30000);
    }

    #[test]
    fn test_webhook_config_defaults() {
        let config = WebhookConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_backoff_ms, 100);
        assert_eq!(config.max_backoff_ms, 10000);
        assert_eq!(config.timeout_seconds, 10);
    }

    #[test]
    fn test_calculate_backoff() {
        let sender = WebhookSender::new(
            "http://example.com/webhook".to_string(),
            "sess_123".to_string(),
        );

        // First attempt: ~100ms
        let backoff1 = sender.calculate_backoff(1);
        assert!(backoff1 >= 75 && backoff1 <= 125);

        // Second attempt: ~200ms
        let backoff2 = sender.calculate_backoff(2);
        assert!(backoff2 >= 150 && backoff2 <= 250);

        // Third attempt: ~400ms
        let backoff3 = sender.calculate_backoff(3);
        assert!(backoff3 >= 300 && backoff3 <= 500);
    }

    #[test]
    fn test_is_retryable_status() {
        // Retryable
        assert!(WebhookSender::is_retryable_status(408)); // Request Timeout
        assert!(WebhookSender::is_retryable_status(429)); // Too Many Requests
        assert!(WebhookSender::is_retryable_status(500)); // Internal Server Error
        assert!(WebhookSender::is_retryable_status(502)); // Bad Gateway
        assert!(WebhookSender::is_retryable_status(503)); // Service Unavailable
        assert!(WebhookSender::is_retryable_status(504)); // Gateway Timeout

        // Not retryable
        assert!(!WebhookSender::is_retryable_status(200)); // OK
        assert!(!WebhookSender::is_retryable_status(400)); // Bad Request
        assert!(!WebhookSender::is_retryable_status(401)); // Unauthorized
        assert!(!WebhookSender::is_retryable_status(403)); // Forbidden
        assert!(!WebhookSender::is_retryable_status(404)); // Not Found
    }

    #[test]
    fn test_webhook_delivery_result() {
        let result = WebhookDeliveryResult {
            success: true,
            status_code: Some(200),
            attempts: 1,
            error: None,
            total_duration_ms: 50,
        };

        assert!(result.success);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.attempts, 1);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_webhook_payload_from_health_event() {
        let event = HealthEvent::stream_started(Some("sess_123".to_string()));
        let payload = WebhookPayload::from_health_event(&event);

        assert_eq!(payload.event_type, "stream_started");
        assert_eq!(payload.session_id, "sess_123");
        assert!(!payload.timestamp.is_empty());
    }
}
