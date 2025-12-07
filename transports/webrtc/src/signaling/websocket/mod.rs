//! WebSocket signaling server for JSON-RPC 2.0 protocol
//!
//! This module provides WebSocket-based signaling as an alternative to gRPC.
//! It implements JSON-RPC 2.0 over WebSocket for browser compatibility.

mod server;
mod handler;

pub use server::WebSocketSignalingServer;
pub use server::WebSocketServerHandle;
