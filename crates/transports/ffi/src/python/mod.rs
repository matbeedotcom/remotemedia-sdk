//! Python FFI bindings for RemoteMedia
//!
//! This module provides PyO3 bindings for RemoteMedia functionality,
//! organized into submodules for different features.
//!
//! # Submodules
//!
//! - **webrtc**: WebRTC server bindings (enabled with `python-webrtc` feature)

// WebRTC bindings (only compiled with `python-webrtc` feature)
#[cfg(feature = "python-webrtc")]
pub mod webrtc;
