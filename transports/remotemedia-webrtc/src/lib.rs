//! WebRTC transport for RemoteMedia pipelines
//!
//! This crate provides WebRTC-based real-time media streaming transport with
//! multi-peer mesh networking support.
//!
//! # Features
//!
//! - **Multi-peer mesh topology**: Up to 10 simultaneous peer connections
//! - **Audio/Video synchronization**: Per-peer sync managers with jitter buffers
//! - **Media codecs**: Opus audio, VP9/H264 video
//! - **Data channels**: Reliable/unreliable messaging
//! - **JSON-RPC 2.0 signaling**: WebSocket-based peer discovery and SDP exchange
//! - **RemoteMedia pipeline integration**: Implements PipelineTransport trait
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │  WebRTC Peers (Browser/Native)                         │
//! │  ↓ (WebRTC peer connections - mesh topology)           │
//! │  WebRtcTransport                                       │
//! │  ├─ SignalingClient (JSON-RPC 2.0 over WebSocket)     │
//! │  ├─ PeerManager (mesh of PeerConnections)             │
//! │  │   └─ Per-peer SyncManager (A/V sync, jitter)       │
//! │  ├─ SessionManager (pipeline sessions)                 │
//! │  └─ SessionRouter (routes data: peers ↔ pipeline)     │
//! │     ↓                                                   │
//! │  remotemedia-runtime-core::PipelineRunner              │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_webrtc::{WebRtcTransport, WebRtcTransportConfig};
//! use remotemedia_runtime_core::PipelineRunner;
//!
//! // Configure transport
//! let config = WebRtcTransportConfig {
//!     signaling_url: "ws://localhost:8080".to_string(),
//!     stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
//!     max_peers: 10,
//!     ..Default::default()
//! };
//!
//! // Create transport
//! let runner = PipelineRunner::new()?;
//! let transport = WebRtcTransport::new(config, runner).await?;
//!
//! // Connect to peer
//! let peer_id = transport.connect_peer("peer-abc123").await?;
//!
//! // Start streaming session with pipeline
//! let session = transport.stream(manifest).await?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

// Public modules
pub mod config;
pub mod error;

// Internal modules
mod signaling;
mod peer;
mod sync;
mod media;
mod channels;
mod session;
mod transport;

// Re-exports for public API
pub use config::{WebRtcTransportConfig, ConfigOptions};
pub use error::{Error, Result};

/// Get the version of this crate
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let ver = version();
        assert!(!ver.is_empty());
    }
}
