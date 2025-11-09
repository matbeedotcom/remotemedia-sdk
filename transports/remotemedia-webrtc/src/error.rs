//! Error types for WebRTC transport

/// Result type alias using WebRTC Error
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in WebRTC transport operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid configuration parameter
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Signaling connection error
    #[error("Signaling error: {0}")]
    SignalingError(String),

    /// Peer not found
    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    /// NAT traversal failed (ICE connection failure)
    #[error("NAT traversal failed: {0}")]
    NatTraversalFailed(String),

    /// Media encoding/decoding error
    #[error("Encoding error: {0}")]
    EncodingError(String),

    /// Session not found
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// Session management error
    #[error("Session error: {0}")]
    SessionError(String),

    /// Invalid data format
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// Operation timeout
    #[error("Operation timeout: {0}")]
    OperationTimeout(String),

    /// WebRTC peer connection error
    #[error("Peer connection error: {0}")]
    PeerConnectionError(String),

    /// ICE candidate error
    #[error("ICE candidate error: {0}")]
    IceCandidateError(String),

    /// SDP negotiation error
    #[error("SDP negotiation error: {0}")]
    SdpError(String),

    /// Data channel error
    #[error("Data channel error: {0}")]
    DataChannelError(String),

    /// Media track error
    #[error("Media track error: {0}")]
    MediaTrackError(String),

    /// Synchronization error (audio/video sync)
    #[error("Synchronization error: {0}")]
    SyncError(String),

    /// Pipeline integration error
    #[error("Pipeline error: {0}")]
    PipelineError(String),

    /// WebSocket error
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Internal error (should not occur in normal operation)
    #[error("Internal error: {0}")]
    InternalError(String),

    /// WebRTC library error
    #[error("WebRTC error: {0}")]
    WebRtcError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Any other error
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Error {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::SignalingError(_)
                | Error::NatTraversalFailed(_)
                | Error::OperationTimeout(_)
                | Error::WebSocketError(_)
                | Error::IoError(_)
        )
    }

    /// Check if this error is a configuration error
    pub fn is_config_error(&self) -> bool {
        matches!(self, Error::InvalidConfig(_))
    }

    /// Check if this error is a peer-related error
    pub fn is_peer_error(&self) -> bool {
        matches!(
            self,
            Error::PeerNotFound(_)
                | Error::PeerConnectionError(_)
                | Error::IceCandidateError(_)
                | Error::SdpError(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::InvalidConfig("test".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: test");
    }

    #[test]
    fn test_error_is_retryable() {
        assert!(Error::SignalingError("test".to_string()).is_retryable());
        assert!(Error::OperationTimeout("test".to_string()).is_retryable());
        assert!(!Error::InvalidConfig("test".to_string()).is_retryable());
    }

    #[test]
    fn test_error_is_config_error() {
        assert!(Error::InvalidConfig("test".to_string()).is_config_error());
        assert!(!Error::SignalingError("test".to_string()).is_config_error());
    }

    #[test]
    fn test_error_is_peer_error() {
        assert!(Error::PeerNotFound("test".to_string()).is_peer_error());
        assert!(Error::PeerConnectionError("test".to_string()).is_peer_error());
        assert!(!Error::InvalidConfig("test".to_string()).is_peer_error());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::from(io_err);
        assert!(matches!(err, Error::IoError(_)));
    }
}
