//! Media codec integration (Opus audio, VP9/H264 video)
//!
//! Handles encoding/decoding and media track management.

pub mod audio;
pub mod video;
pub mod tracks;

pub use audio::{AudioEncoder, AudioDecoder};
pub use video::{VideoEncoder, VideoDecoder};
pub use tracks::{AudioTrack, VideoTrack};
