//! Rust-only WebRTC test server for Playwright e2e.
//!
//! Same wire surface as `lfm2_audio_webrtc_server` (WebRTC audio + the
//! `control.*` JSON-RPC methods on the signaling WS) but the pipeline
//! is pure Rust — no Python, no ML, no GPU. That keeps CI time and
//! environment requirements minimal while still exercising:
//!
//! - WebRTC SDP / ICE negotiation over the WebSocket signaler
//! - `control.subscribe` → `control.event` notification round-trip
//!   against a real tap (VAD emits JSON per chunk, so subs always
//!   have traffic even on a silent mic)
//! - `control.publish` to an arbitrary aux port (router accepts the
//!   packet; the fan-out doesn't need a real handler for the test)
//!
//! Topology:
//!
//! ```text
//!   mic (48k) ─► resample_in (48k→16k) ─► chunker (512) ─► vad ─► accumulator
//! ```
//!
//! The `AudioChunkerNode` is load-bearing: SileroVAD's ONNX model
//! expects fixed 512-sample input chunks; without chunking, the raw
//! resample output has arbitrary frame sizes and VAD throws
//! `Invalid dimension #2` on every chunk.
//!
//! Usage:
//!
//! ```bash
//! cargo run --example webrtc_test_server \
//!     -p remotemedia-webrtc --features ws-signaling -- --port 18091
//! ```

#![cfg(feature = "ws-signaling")]

use remotemedia_core::manifest::{
    Connection, Manifest, ManifestMetadata, NodeManifest,
};
use remotemedia_core::transport::PipelineExecutor;
use remotemedia_webrtc::config::WebRtcTransportConfig;
use remotemedia_webrtc::signaling::WebSocketSignalingServer;
use std::sync::Arc;

fn build_manifest() -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "webrtc-test-pipeline".to_string(),
            description: Some(
                "Rust-only WebRTC+control-bus test pipeline for Playwright \
                 e2e. No Python, no ML."
                    .to_string(),
            ),
            ..Default::default()
        },
        nodes: vec![
            NodeManifest {
                id: "resample_in".to_string(),
                node_type: "FastResampleNode".to_string(),
                params: serde_json::json!({
                    "source_rate": 48000,
                    "target_rate": 16000,
                    "quality": "Medium",
                    "channels": 1,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "chunker".to_string(),
                node_type: "AudioChunkerNode".to_string(),
                params: serde_json::json!({
                    "chunkSize": 512,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "vad".to_string(),
                node_type: "SileroVADNode".to_string(),
                params: serde_json::json!({
                    "threshold": 0.5,
                    "sample_rate": 16000,
                    "min_speech_duration_ms": 250,
                    "min_silence_duration_ms": 400,
                    "speech_pad_ms": 150,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "accumulator".to_string(),
                node_type: "AudioBufferAccumulatorNode".to_string(),
                params: serde_json::json!({
                    "min_utterance_duration_ms": 300,
                    "max_utterance_duration_ms": 30000,
                }),
                ..Default::default()
            },
        ],
        connections: vec![
            Connection {
                from: "resample_in".to_string(),
                to: "chunker".to_string(),
            },
            Connection {
                from: "chunker".to_string(),
                to: "vad".to_string(),
            },
            Connection {
                from: "vad".to_string(),
                to: "accumulator".to_string(),
            },
        ],
        python_env: None,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let mut args = std::env::args().skip(1);
    let mut port: u16 = 18091;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                port = args
                    .next()
                    .ok_or("--port requires a value")?
                    .parse()
                    .map_err(|e| format!("bad --port: {e}"))?;
            }
            // Accepted for CLI compat — server binds 0.0.0.0 internally.
            "--host" => {
                let _ = args.next().ok_or("--host requires a value")?;
            }
            "-h" | "--help" => {
                eprintln!("webrtc_test_server [--host ADDR] [--port PORT]");
                return Ok(());
            }
            other => {
                eprintln!("unrecognized arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let manifest = Arc::new(build_manifest());
    let executor = Arc::new(PipelineExecutor::new()?);
    let config = Arc::new(WebRtcTransportConfig::default());

    let server = WebSocketSignalingServer::new(port, config, executor, manifest);
    let _handle = server.start().await?;

    println!("READY ws://127.0.0.1:{port}/ws");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
