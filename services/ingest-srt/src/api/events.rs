//! SSE events endpoint
//!
//! Provides real-time event streaming to clients via Server-Sent Events.

use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    http::StatusCode,
    Json,
};
use futures::stream::Stream;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use super::sessions::ErrorResponse;
use super::AppState;
use remotemedia_health_analyzer::HealthEvent;

/// SSE event stream for a session
///
/// GET /api/ingest/sessions/:id/events
pub async fn events_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let session = match state.session_manager.get_session(&session_id).await {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "session_not_found".to_string(),
                    message: format!("Session {} not found", session_id),
                }),
            ));
        }
    };

    // Subscribe to session events
    let receiver = session.subscribe();

    // Convert broadcast stream to SSE events
    let stream = EventStream::new(receiver);

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// Wrapper around BroadcastStream that converts HealthEvents to SSE Events
struct EventStream {
    inner: BroadcastStream<HealthEvent>,
}

impl EventStream {
    fn new(receiver: broadcast::Receiver<HealthEvent>) -> Self {
        Self {
            inner: BroadcastStream::new(receiver),
        }
    }
}

impl Stream for EventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(health_event))) => {
                // Determine event type for SSE
                let event_name = if health_event.is_system() {
                    "system"
                } else if health_event.is_health() {
                    "health"
                } else if health_event.is_alert() {
                    "alert"
                } else {
                    "event"
                };

                // Serialize to JSON
                match serde_json::to_string(&health_event) {
                    Ok(json) => {
                        tracing::debug!(
                            event_type = event_name,
                            json_len = json.len(),
                            "SSE sending event to client"
                        );
                        let sse_event = Event::default().event(event_name).data(json);
                        Poll::Ready(Some(Ok(sse_event)))
                    }
                    Err(e) => {
                        tracing::error!("Failed to serialize event: {}", e);
                        // Skip this event and continue
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Poll::Ready(Some(Err(e))) => {
                // Broadcast lagged - subscriber fell behind
                tracing::warn!("Broadcast lagged: {}", e);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(None) => {
                // Channel closed - session ended
                tracing::debug!("SSE stream closed (channel ended)");
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// SSE event payload for session events
#[derive(serde::Serialize)]
pub struct SessionEvent {
    /// Event type (system, alert, health)
    #[serde(rename = "type")]
    pub event_type: String,

    /// Session ID
    pub session_id: String,

    /// Timestamp (ISO 8601)
    pub timestamp: String,

    /// Time since stream started in milliseconds
    pub relative_ms: u64,

    /// Event-specific data
    pub data: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_event_serialization() {
        let event = SessionEvent {
            event_type: "alert.silence".to_string(),
            session_id: "sess_123".to_string(),
            timestamp: "2025-01-01T00:00:30.000Z".to_string(),
            relative_ms: 30000,
            data: serde_json::json!({
                "duration_ms": 3500,
                "threshold_ms": 3000
            }),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"alert.silence\""));
        assert!(json.contains("\"session_id\":\"sess_123\""));
        assert!(json.contains("\"relative_ms\":30000"));
    }
}
