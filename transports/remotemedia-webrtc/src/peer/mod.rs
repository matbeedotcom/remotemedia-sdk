//! WebRTC peer connection management
//!
//! Handles peer lifecycle, ICE negotiation, and media track management.

pub mod connection;
pub mod manager;

pub use connection::{ConnectionState, PeerConnection};
pub use manager::{PeerInfo, PeerManager};
