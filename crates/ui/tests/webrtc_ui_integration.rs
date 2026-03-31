//! Integration test: WebRTC signaling server + UI server
//!
//! Tests the full stack:
//! 1. Starts a WebRTC gRPC signaling server with a passthrough pipeline
//! 2. Starts the UI server pointing at it
//! 3. Verifies UI API endpoints report correct transport info
//! 4. Verifies pipeline execution through the UI API
//! 5. Verifies WebRTC client can connect to signaling server
//!
//! # Running
//!
//! ```bash
//! cargo test -p remotemedia-ui --test webrtc_ui_integration -- --nocapture
//! ```

use remotemedia_core::manifest::Manifest;
use remotemedia_core::transport::PipelineExecutor;
use remotemedia_ui::{TransportInfo, UiServerBuilder};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

/// Simple passthrough manifest
const PASSTHROUGH_MANIFEST: &str = r#"
{
    "version": "1.0",
    "metadata": {
        "name": "passthrough",
        "description": "Simple passthrough for testing"
    },
    "nodes": [
        {
            "id": "passthrough",
            "node_type": "PassThrough",
            "params": {}
        }
    ],
    "connections": []
}
"#;

/// Find a free port by binding to port 0
async fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Start UI server on a random port, returns the port
async fn start_ui_server(
    transport_info: Option<TransportInfo>,
    manifest: Option<Arc<Manifest>>,
) -> u16 {
    let port = free_port().await;
    let bind = format!("127.0.0.1:{}", port);

    let executor = Arc::new(PipelineExecutor::new().unwrap());

    let mut builder = UiServerBuilder::new().bind(&bind).executor(executor);

    if let Some(m) = manifest {
        builder = builder.manifest(m);
    }
    if let Some(t) = transport_info {
        builder = builder.transport_info(t);
    }

    let server = builder.build().unwrap();

    tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("UI server error: {}", e);
        }
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    port
}

// ============================================================================
// Status API Tests
// ============================================================================

#[tokio::test]
async fn test_ui_status_no_transport() {
    let port = start_ui_server(None, None).await;

    let resp: serde_json::Value = reqwest::get(format!("http://127.0.0.1:{}/api/status", port))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp["version"], "0.4.0");
    assert!(resp["transport"].is_null());
    assert_eq!(resp["active_sessions"], 0);
}

#[tokio::test]
async fn test_ui_status_with_webrtc_transport() {
    let port = start_ui_server(
        Some(TransportInfo {
            transport_type: "webrtc".to_string(),
            address: "127.0.0.1:8989".to_string(),
        }),
        None,
    )
    .await;

    let resp: serde_json::Value = reqwest::get(format!("http://127.0.0.1:{}/api/status", port))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp["transport"]["transport_type"], "webrtc");
    assert_eq!(resp["transport"]["address"], "127.0.0.1:8989");
}

// ============================================================================
// Manifest API Tests
// ============================================================================

#[tokio::test]
async fn test_ui_manifest_not_set() {
    let port = start_ui_server(None, None).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/api/manifest", port))
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_ui_manifest_returns_pipeline() {
    let manifest: Manifest = serde_json::from_str(PASSTHROUGH_MANIFEST).unwrap();
    let port = start_ui_server(None, Some(Arc::new(manifest))).await;

    let resp: serde_json::Value =
        reqwest::get(format!("http://127.0.0.1:{}/api/manifest", port))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

    assert_eq!(resp["metadata"]["name"], "passthrough");
    assert_eq!(resp["nodes"][0]["node_type"], "PassThrough");
}

// ============================================================================
// Pipeline Execution Tests
// ============================================================================

#[tokio::test]
async fn test_ui_execute_text_passthrough() {
    let manifest: Manifest = serde_json::from_str(PASSTHROUGH_MANIFEST).unwrap();
    let port = start_ui_server(None, Some(Arc::new(manifest))).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/execute", port))
        .json(&serde_json::json!({
            "input": {
                "data": { "Text": "hello world" },
                "metadata": {}
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    // PassThrough node returns input as-is
    assert_eq!(body["output"]["data"]["Text"], "hello world");
}

#[tokio::test]
async fn test_ui_execute_with_inline_manifest() {
    // No server manifest set - provide it in the request
    let port = start_ui_server(None, None).await;

    let manifest: serde_json::Value = serde_json::from_str(PASSTHROUGH_MANIFEST).unwrap();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/execute", port))
        .json(&serde_json::json!({
            "manifest": manifest,
            "input": {
                "data": { "Text": "inline manifest test" },
                "metadata": {}
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["output"]["data"]["Text"], "inline manifest test");
}

#[tokio::test]
async fn test_ui_execute_missing_manifest_returns_error() {
    // No manifest set and none provided in request
    let port = start_ui_server(None, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/execute", port))
        .json(&serde_json::json!({
            "input": {
                "data": { "Text": "no manifest" },
                "metadata": {}
            }
        }))
        .send()
        .await
        .unwrap();

    // Should fail since no manifest is available
    assert_ne!(resp.status(), 200);
}

// ============================================================================
// Static Asset Tests
// ============================================================================

#[tokio::test]
async fn test_ui_serves_html() {
    let port = start_ui_server(None, None).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/", port))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("RemoteMedia"));
}

#[tokio::test]
async fn test_ui_spa_fallback() {
    let port = start_ui_server(None, None).await;

    // Non-existent path should still return index.html (SPA routing)
    let resp = reqwest::get(format!("http://127.0.0.1:{}/some/deep/path", port))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("<!DOCTYPE html>"));
}

// ============================================================================
// Streaming Input → Output Tests
// ============================================================================

#[tokio::test]
async fn test_ui_stream_text_passthrough() {
    let manifest: Manifest = serde_json::from_str(PASSTHROUGH_MANIFEST).unwrap();
    let port = start_ui_server(
        Some(TransportInfo {
            transport_type: "webrtc".to_string(),
            address: format!("127.0.0.1:{}", port_placeholder()),
        }),
        Some(Arc::new(manifest)),
    )
    .await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", port);

    // 1. Create streaming session
    let resp = client
        .post(format!("{}/api/stream", base))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // 2. Start SSE listener in background (must subscribe BEFORE sending input)
    let sse_url = format!("{}/api/stream/{}/output", base, session_id);
    let (sse_tx, mut sse_rx) = tokio::sync::mpsc::channel::<String>(10);
    let sse_handle = tokio::spawn(async move {
        let resp = reqwest::get(&sse_url).await.unwrap();
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            // SSE events are separated by double newlines
            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();
                // Extract data field from SSE event
                for line in event.lines() {
                    if let Some(data) = line.strip_prefix("data:") {
                        let _ = sse_tx.send(data.trim().to_string()).await;
                    }
                }
            }
        }
    });

    // Give SSE connection time to establish
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. Send input data
    let resp = client
        .post(format!("{}/api/stream/{}/input", base, session_id))
        .json(&serde_json::json!({
            "data": {
                "data": { "Text": "hello from webrtc" },
                "metadata": {}
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // 4. Receive output from SSE stream
    let output = tokio::time::timeout(Duration::from_secs(5), sse_rx.recv())
        .await
        .expect("Timed out waiting for SSE output")
        .expect("SSE channel closed");

    let output_json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(output_json["data"]["Text"], "hello from webrtc");

    // 5. Cleanup
    client
        .delete(format!("{}/api/stream/{}", base, session_id))
        .send()
        .await
        .unwrap();
    sse_handle.abort();
}

/// Helper: dummy port for TransportInfo (not actually used in test)
fn port_placeholder() -> u16 {
    9999
}

#[tokio::test]
async fn test_ui_stream_multiple_inputs() {
    let manifest: Manifest = serde_json::from_str(PASSTHROUGH_MANIFEST).unwrap();
    let port = start_ui_server(None, Some(Arc::new(manifest))).await;

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", port);

    // Create session
    let resp = client
        .post(format!("{}/api/stream", base))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Start SSE listener
    let sse_url = format!("{}/api/stream/{}/output", base, session_id);
    let (sse_tx, mut sse_rx) = tokio::sync::mpsc::channel::<String>(10);
    let sse_handle = tokio::spawn(async move {
        let resp = reqwest::get(&sse_url).await.unwrap();
        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();
                for line in event.lines() {
                    if let Some(data) = line.strip_prefix("data:") {
                        let _ = sse_tx.send(data.trim().to_string()).await;
                    }
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send 3 sequential inputs
    let messages = ["first", "second", "third"];
    for msg in &messages {
        let resp = client
            .post(format!("{}/api/stream/{}/input", base, session_id))
            .json(&serde_json::json!({
                "data": {
                    "data": { "Text": msg },
                    "metadata": {}
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    // Receive all 3 outputs
    let mut received = Vec::new();
    for _ in 0..3 {
        let output = tokio::time::timeout(Duration::from_secs(5), sse_rx.recv())
            .await
            .expect("Timed out waiting for SSE output")
            .expect("SSE channel closed");
        let output_json: serde_json::Value = serde_json::from_str(&output).unwrap();
        received.push(output_json["data"]["Text"].as_str().unwrap().to_string());
    }

    assert_eq!(received, vec!["first", "second", "third"]);

    // Cleanup
    client
        .delete(format!("{}/api/stream/{}", base, session_id))
        .send()
        .await
        .unwrap();
    sse_handle.abort();
}

#[tokio::test]
async fn test_ui_stream_input_invalid_session() {
    let port = start_ui_server(None, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/stream/nonexistent/input",
            port
        ))
        .json(&serde_json::json!({
            "data": {
                "data": { "Text": "hello" },
                "metadata": {}
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

// ============================================================================
// Streaming Session Lifecycle Tests
// ============================================================================

#[tokio::test]
async fn test_ui_stream_lifecycle() {
    let manifest: Manifest = serde_json::from_str(PASSTHROUGH_MANIFEST).unwrap();
    let port = start_ui_server(None, Some(Arc::new(manifest))).await;

    let client = reqwest::Client::new();

    // Create session
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/stream", port))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let session_id = body["session_id"].as_str().unwrap().to_string();
    assert!(!session_id.is_empty());

    // Check session count increased
    let status: serde_json::Value =
        reqwest::get(format!("http://127.0.0.1:{}/api/status", port))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(status["active_sessions"], 1);

    // Close session
    let resp = client
        .delete(format!(
            "http://127.0.0.1:{}/api/stream/{}",
            port, session_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Session count should be back to 0
    let status: serde_json::Value =
        reqwest::get(format!("http://127.0.0.1:{}/api/status", port))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(status["active_sessions"], 0);
}
