//! CLI-specific pipeline nodes
//!
//! This module contains nodes that are specific to the CLI application:
//! - `MicInput` - Source node that captures audio from microphone
//! - `SpeakerOutput` - Sink node that plays audio through speakers
//! - `SrtOutput` - Converts transcription to SRT subtitle format
//!
//! # Usage in Pipelines
//!
//! ```yaml
//! version: v1
//! metadata:
//!   name: voice-pipeline
//!
//! nodes:
//!   # Microphone input (source node)
//!   - id: mic
//!     node_type: MicInput
//!     params:
//!       sample_rate: 16000
//!       channels: 1
//!
//!   # Process audio...
//!   - id: processor
//!     node_type: SomeAudioProcessor
//!
//!   # Speaker output (sink node)
//!   - id: speaker
//!     node_type: SpeakerOutput
//!     params:
//!       sample_rate: 16000
//!       channels: 1
//!
//! connections:
//!   - from: mic
//!     to: processor
//!   - from: processor
//!     to: speaker
//! ```

pub mod mic_input;
pub mod registry;
pub mod speaker_output;
pub mod srt_output;

// Node types and configs
pub use mic_input::{MicInputConfig, MicInputNode};
pub use speaker_output::{SpeakerOutputConfig, SpeakerOutputNode};
pub use srt_output::{SrtOutputConfig, SrtOutputNode};

// Streaming registry and factories
pub use registry::{
    create_cli_streaming_registry, get_cli_node_factories,
    MicInputNodeFactory, SpeakerOutputNodeFactory, SrtOutputNodeFactory,
};

/// Register CLI-specific nodes with the streaming registry
///
/// This is a convenience function that returns a streaming registry
/// with all CLI nodes pre-registered.
pub fn register_cli_nodes() -> remotemedia_runtime_core::nodes::streaming_node::StreamingNodeRegistry {
    create_cli_streaming_registry()
}
