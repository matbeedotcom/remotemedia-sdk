//! WebRTC transport for RemoteMedia pipelines
//!
//! **Status**: Placeholder for future implementation
//!
//! This crate will provide WebRTC-based real-time media streaming transport
//! using the `remotemedia-runtime-core` abstraction layer.
//!
//! # Planned Features
//!
//! - WebRTC peer connection management
//! - ICE/STUN/TURN support
//! - Data channel streaming for pipeline I/O
//! - Media track streaming for audio/video
//! - Low-latency real-time processing
//!
//! # Architecture (Planned)
//!
//! ```text
//! ┌────────────────────────────────────────┐
//! │  WebRTC Client (Browser/Native)       │
//! │  ↓ (WebRTC peer connection)            │
//! │  remotemedia-webrtc Transport          │
//! │  ├─ Signaling server                   │
//! │  ├─ Data channels for control          │
//! │  └─ Media tracks for audio/video       │
//! │     ↓                                   │
//! │  remotemedia-runtime-core              │
//! │  └─ PipelineRunner (streaming)         │
//! └────────────────────────────────────────┘
//! ```
//!
//! # Future Implementation Notes
//!
//! When implementing this transport:
//! 1. Use `PipelineRunner::create_stream_session()` for streaming pipelines
//! 2. Map WebRTC data channels to `StreamSession::send_input()` / `receive_output()`
//! 3. Handle ICE candidates and SDP negotiation in signaling layer
//! 4. Consider using `webrtc` crate from workspace dependencies
//!
//! # Example (Future API)
//!
//! ```ignore
//! use remotemedia_webrtc::WebRTCTransport;
//! use remotemedia_runtime_core::transport::PipelineRunner;
//!
//! let runner = PipelineRunner::new()?;
//! let transport = WebRTCTransport::new(runner)?;
//! transport.listen("0.0.0.0:8080").await?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

/// Placeholder error type for WebRTC transport
#[derive(Debug, thiserror::Error)]
pub enum WebRTCError {
    /// Not yet implemented
    #[error("WebRTC transport is not yet implemented")]
    NotImplemented,
}

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
