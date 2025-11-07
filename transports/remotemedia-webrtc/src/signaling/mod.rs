//! Signaling protocol and client for WebRTC peer discovery and SDP exchange
//!
//! This module implements JSON-RPC 2.0 over WebSocket for signaling operations.

pub mod protocol;
pub mod client;
pub mod connection;

pub use protocol::IceCandidateParams;
pub use client::SignalingClient;
