//! Core nodes provider - registers all built-in core streaming nodes
//!
//! This provider registers the fundamental nodes that are part of remotemedia-core:
//! - Audio processing nodes (resampling, chunking, VAD)
//! - Video processing nodes (flip, encode, decode, scale)
//! - Text processing nodes (collector)
//! - Health monitoring nodes
//! - Utility nodes (passthrough, calculator)
//!
//! Python-based nodes are NOT registered here - they are in the separate
//! `remotemedia-nodes-python` crate.

use crate::nodes::provider::NodeProvider;
use crate::nodes::streaming_node::StreamingNodeRegistry;
use std::sync::Arc;

// Import all factory types from streaming_registry
use super::streaming_registry::{
    AudioBufferAccumulatorNodeFactory, AudioChunkerNodeFactory, CalculatorNodeFactory,
    FastResampleNodeFactory, PassThroughNodeFactory, SpeculativeAudioCommitNodeFactory,
    SpeculativeVADCoordinatorFactory, SpeculativeVADGateFactory, TextCollectorNodeFactory,
    VideoFlipNodeFactory,
};

// Import factories defined in their own modules
use crate::nodes::audio_channel_splitter::AudioChannelSplitterNodeFactory;
use crate::nodes::audio_evidence::AudioEvidenceNodeFactory;
use crate::nodes::audio_level::AudioLevelNodeFactory;
use crate::nodes::channel_balance::ChannelBalanceNodeFactory;
use crate::nodes::clipping_detector::ClippingDetectorNodeFactory;
use crate::nodes::conversation_coordinator::ConversationCoordinatorNodeFactory;
use crate::nodes::conversation_flow::ConversationFlowNodeFactory;
use crate::nodes::event_correlator::EventCorrelatorNodeFactory;
use crate::nodes::health_emitter::HealthEmitterNodeFactory;
use crate::nodes::multimodal_llm::MultimodalLLMNodeFactory;
use crate::nodes::openai_chat::OpenAIChatNodeFactory;
use crate::nodes::remote_pipeline::RemotePipelineNodeFactory;
use crate::nodes::session_health::SessionHealthNodeFactory;
use crate::nodes::silence_detector::SilenceDetectorNodeFactory;
use crate::nodes::speech_presence::SpeechPresenceNodeFactory;
use crate::nodes::timing_drift::TimingDriftNodeFactory;

/// Provider for core built-in nodes.
///
/// Registers fundamental audio, video, and utility nodes that ship with remotemedia-core.
/// Has high priority (1000) to ensure core nodes are registered first.
pub struct CoreNodesProvider;

impl NodeProvider for CoreNodesProvider {
    fn register(&self, registry: &mut StreamingNodeRegistry) {
        // Basic utility nodes
        registry.register(Arc::new(CalculatorNodeFactory));
        registry.register(Arc::new(PassThroughNodeFactory));

        // Video processing nodes
        registry.register(Arc::new(VideoFlipNodeFactory));

        #[cfg(feature = "video")]
        {
            use super::streaming_registry::{
                VideoDecoderNodeFactory, VideoEncoderNodeFactory, VideoFormatConverterNodeFactory,
                VideoScalerNodeFactory,
            };
            registry.register(Arc::new(VideoEncoderNodeFactory));
            registry.register(Arc::new(VideoDecoderNodeFactory));
            registry.register(Arc::new(VideoScalerNodeFactory));
            registry.register(Arc::new(VideoFormatConverterNodeFactory));
        }

        // Audio processing nodes
        registry.register(Arc::new(AudioChunkerNodeFactory));
        registry.register(Arc::new(AudioBufferAccumulatorNodeFactory));
        registry.register(Arc::new(SpeculativeAudioCommitNodeFactory));
        registry.register(Arc::new(FastResampleNodeFactory));
        registry.register(Arc::new(AudioChannelSplitterNodeFactory));

        // Text processing nodes
        registry.register(Arc::new(TextCollectorNodeFactory));
        registry.register(Arc::new(ConversationCoordinatorNodeFactory));

        // File-sink nodes — write Audio / Video to disk in WAV / Y4M.
        {
            use super::streaming_registry::{
                AudioFileWriterNodeFactory, VideoFileWriterNodeFactory,
            };
            registry.register(Arc::new(AudioFileWriterNodeFactory));
            registry.register(Arc::new(VideoFileWriterNodeFactory));
        }

        // Avatar pipeline (spec 2026-04-27): emoji-tag extraction
        #[cfg(feature = "avatar-emotion")]
        {
            use super::streaming_registry::EmotionExtractorNodeFactory;
            registry.register(Arc::new(EmotionExtractorNodeFactory));
        }

        // Avatar pipeline (spec 2026-04-27 §3.4): SyntheticLipSyncNode —
        // deterministic stand-in for tests + manifest fallback.
        #[cfg(feature = "avatar-lipsync")]
        {
            use super::streaming_registry::SyntheticLipSyncNodeFactory;
            registry.register(Arc::new(SyntheticLipSyncNodeFactory));
        }

        // Avatar pipeline (spec 2026-04-27 §3.4): Audio2FaceLipSyncNode —
        // real ONNX-driven blendshape lip-sync. Bundle-path-driven via
        // params; factory loads the .moc3 + ONNX network at session
        // start (heavy — ~3.6s on Apple Silicon CPU).
        #[cfg(feature = "avatar-audio2face")]
        {
            use super::streaming_registry::Audio2FaceLipSyncNodeFactory;
            registry.register(Arc::new(Audio2FaceLipSyncNodeFactory));
        }

        // Avatar pipeline (spec 2026-04-27 §6.1): Live2DRenderNode —
        // wgpu-backed Cubism renderer. Factory boots the wgpu device,
        // uploads the model's textures, and allocates per-drawable
        // GPU buffers at session start.
        #[cfg(feature = "avatar-render-wgpu")]
        {
            use super::streaming_registry::Live2DRenderNodeFactory;
            registry.register(Arc::new(Live2DRenderNodeFactory));
        }

        // LLM nodes
        registry.register(Arc::new(OpenAIChatNodeFactory));
        registry.register(Arc::new(MultimodalLLMNodeFactory));

        // Remote pipeline node
        registry.register(Arc::new(RemotePipelineNodeFactory));

        // VAD nodes (Rust ONNX)
        #[cfg(feature = "silero-vad")]
        {
            use super::streaming_registry::SileroVADNodeFactory;
            registry.register(Arc::new(SileroVADNodeFactory));
        }

        // Speculative VAD nodes (spec 007)
        registry.register(Arc::new(SpeculativeVADGateFactory));
        registry.register(Arc::new(SpeculativeVADCoordinatorFactory));

        // Speaker diarization
        #[cfg(feature = "speaker-diarization")]
        registry.register(Arc::new(
            crate::nodes::speaker_diarization::SpeakerDiarizationNodeFactory,
        ));

        // Health monitoring nodes (spec 027)
        registry.register(Arc::new(HealthEmitterNodeFactory));
        registry.register(Arc::new(AudioLevelNodeFactory));
        registry.register(Arc::new(ClippingDetectorNodeFactory));
        registry.register(Arc::new(ChannelBalanceNodeFactory));
        registry.register(Arc::new(SilenceDetectorNodeFactory));

        // Stream health monitoring - business layer
        registry.register(Arc::new(SpeechPresenceNodeFactory));
        registry.register(Arc::new(ConversationFlowNodeFactory));
        registry.register(Arc::new(SessionHealthNodeFactory));

        // Stream health monitoring - technical layer
        registry.register(Arc::new(TimingDriftNodeFactory));
        registry.register(Arc::new(EventCorrelatorNodeFactory));
        registry.register(Arc::new(AudioEvidenceNodeFactory));

        // Output formatters
        use super::streaming_registry::SrtOutputNodeFactory;
        registry.register(Arc::new(SrtOutputNodeFactory));

        // llama.cpp nodes (native GGUF inference)
        #[cfg(feature = "llama-cpp")]
        {
            use super::llama_cpp::{
                LlamaCppActivationNodeFactory, LlamaCppEmbeddingNodeFactory,
                LlamaCppGenerationNodeFactory, LlamaCppSteerNodeFactory,
            };
            registry.register(Arc::new(LlamaCppGenerationNodeFactory));
            registry.register(Arc::new(LlamaCppEmbeddingNodeFactory));
            registry.register(Arc::new(LlamaCppActivationNodeFactory));
            registry.register(Arc::new(LlamaCppSteerNodeFactory));
        }
    }

    fn provider_name(&self) -> &'static str {
        "core-nodes"
    }

    fn node_count(&self) -> usize {
        // Approximate count - varies by feature flags
        25
    }

    fn priority(&self) -> i32 {
        // Core nodes have highest priority
        1000
    }
}

// Auto-register the core nodes provider
// Uses static reference for const initialization required by inventory
inventory::submit! {
    &CoreNodesProvider as &'static dyn NodeProvider
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_provider_registers_nodes() {
        let mut registry = StreamingNodeRegistry::new();
        let provider = CoreNodesProvider;

        provider.register(&mut registry);

        // Should have registered some nodes
        assert!(!registry.list_types().is_empty());

        // Check some expected nodes exist
        assert!(registry.has_node_type("CalculatorNode"));
        assert!(registry.has_node_type("PassThrough"));
        assert!(registry.has_node_type("VideoFlip"));
        assert!(registry.has_node_type("FastResampleNode"));
    }

    #[test]
    fn test_provider_metadata() {
        let provider = CoreNodesProvider;
        assert_eq!(provider.provider_name(), "core-nodes");
        assert_eq!(provider.priority(), 1000);
        assert!(provider.node_count() > 0);
    }
}
