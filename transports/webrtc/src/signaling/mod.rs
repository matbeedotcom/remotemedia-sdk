//! Signaling protocol and client for WebRTC peer discovery and SDP exchange
//!
//! This module implements two signaling transports:
//! - WebSocket-based JSON-RPC 2.0 (default)
//! - gRPC bidirectional streaming (optional, requires `grpc-signaling` feature)

pub mod client;
pub mod connection;
pub mod protocol;

#[cfg(feature = "grpc-signaling")]
pub mod grpc;

pub use client::SignalingClient;
pub use protocol::IceCandidateParams;

#[cfg(feature = "grpc-signaling")]
pub use grpc::WebRtcSignalingService;
