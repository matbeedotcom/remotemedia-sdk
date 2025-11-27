//! WebRTC transport wrapper

use anyhow::Result;

/// WebRTC client wrapper
pub struct WebRTCClient {
    signaling_url: String,
}

impl WebRTCClient {
    /// Create a new WebRTC client
    pub fn new(signaling_url: &str) -> Self {
        Self {
            signaling_url: signaling_url.to_string(),
        }
    }

    /// Connect to signaling server
    pub async fn connect(&self) -> Result<()> {
        // TODO: Use remotemedia-webrtc transport
        tracing::info!("Connecting to WebRTC signaling: {}", self.signaling_url);
        anyhow::bail!("WebRTC not yet implemented")
    }
}
