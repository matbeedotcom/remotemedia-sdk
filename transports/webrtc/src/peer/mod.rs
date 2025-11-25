//! WebRTC peer connection management
//!
//! Handles peer lifecycle, ICE negotiation, and media track management.

pub mod connection;
pub mod manager;

#[cfg(feature = "grpc-signaling")]
pub mod server_peer;

pub use connection::{ConnectionState, PeerConnection};
pub use manager::{PeerInfo, PeerManager};

#[cfg(feature = "grpc-signaling")]
pub use server_peer::ServerPeer;
