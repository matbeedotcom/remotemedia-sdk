//! Audio processing nodes
//!
//! High-performance Rust implementations of audio nodes

pub mod fast;
pub mod format_converter;
pub mod format_converter_fast;
pub mod resample;
pub mod resample_fast;
pub mod vad;
pub mod vad_fast;

pub use fast::FastAudioNode;
pub use format_converter::{FormatConverterNodeFactory, RustFormatConverterNode};
pub use format_converter_fast::FastFormatConverter;
pub use resample::{ResampleNodeFactory, RustResampleNode};
pub use resample_fast::{FastResampleNode, ResampleQuality};
pub use vad::{RustVADNode, VADNodeFactory};
pub use vad_fast::FastVADNode;
