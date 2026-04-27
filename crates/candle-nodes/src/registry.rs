//! Node factory registration for Candle ML nodes
//!
//! Provides factory registration helpers for integrating Candle nodes
//! with the RemoteMedia StreamingNodeRegistry.
//!
//! # Auto-Registration
//!
//! When this crate is added as a dependency, the `CandleNodesProvider` is
//! automatically registered via the `inventory` system. Nodes are registered
//! based on enabled feature flags.

use remotemedia_core::nodes::provider::NodeProvider;
use remotemedia_core::nodes::streaming_node::{StreamingNodeFactory, StreamingNodeRegistry};
use std::sync::Arc;

/// Trait for Candle node factories
pub trait CandleNodeFactory: StreamingNodeFactory {
    /// Get the model family this factory creates nodes for
    fn model_family(&self) -> &'static str;

    /// Get supported model variants
    fn supported_variants(&self) -> &[&'static str];
}

/// Provider for Candle ML inference nodes.
///
/// Automatically registers nodes based on enabled feature flags:
/// - `whisper` - Speech-to-text via Whisper models
/// - `yolo` - Object detection via YOLO models
/// - `llm` - Text generation via Phi and LLaMA models
/// - `vad` - Voice activity detection via Silero VAD
///
/// Priority is 800 (below core nodes at 1000, above Python nodes at 500).
pub struct CandleNodesProvider;

impl NodeProvider for CandleNodesProvider {
    fn register(&self, registry: &mut StreamingNodeRegistry) {
        #[cfg(feature = "whisper")]
        {
            use crate::whisper::WhisperNodeFactory;
            registry.register(Arc::new(WhisperNodeFactory::new()));
            tracing::debug!("Registered candle-whisper node factory");
        }

        #[cfg(feature = "yolo")]
        {
            use crate::yolo::YoloNodeFactory;
            registry.register(Arc::new(YoloNodeFactory::new()));
            tracing::debug!("Registered candle-yolo node factory");
        }

        #[cfg(feature = "llm")]
        {
            use crate::llm::{LlamaNodeFactory, PhiNodeFactory};
            registry.register(Arc::new(PhiNodeFactory::new()));
            registry.register(Arc::new(LlamaNodeFactory::new()));
            tracing::debug!("Registered candle-phi and candle-llama node factories");
        }

        #[cfg(feature = "vad")]
        {
            use crate::vad::SileroVadNodeFactory;
            registry.register(Arc::new(SileroVadNodeFactory::new()));
            tracing::debug!("Registered candle-silero-vad node factory");
        }

        // Emotion vector nodes (always available, no feature flag)
        {
            use crate::emotion::{EmotionExtractorNodeFactory, EmotionSteeringNodeFactory};
            registry.register(Arc::new(EmotionExtractorNodeFactory::new()));
            registry.register(Arc::new(EmotionSteeringNodeFactory::new()));
            tracing::debug!("Registered emotion-extractor and emotion-steering node factories");
        }
    }

    fn provider_name(&self) -> &'static str {
        "candle-nodes"
    }

    fn node_count(&self) -> usize {
        let mut count = 0;
        #[cfg(feature = "whisper")]
        {
            count += 1;
        }
        #[cfg(feature = "yolo")]
        {
            count += 1;
        }
        #[cfg(feature = "llm")]
        {
            count += 2;
        }
        #[cfg(feature = "vad")]
        {
            count += 1;
        }
        // Emotion vector nodes (always available)
        count += 2;
        count
    }

    fn priority(&self) -> i32 {
        // Below core nodes (1000), above Python nodes (500)
        800
    }
}

// Auto-register the Candle nodes provider
inventory::submit! {
    &CandleNodesProvider as &'static dyn NodeProvider
}

/// Register all enabled Candle nodes with a registry
///
/// **DEPRECATED**: Use the automatic registration via `inventory` instead.
/// This function is kept for backward compatibility.
///
/// # Example
///
/// ```ignore
/// let mut registry = StreamingNodeRegistry::new();
/// register_candle_nodes(&mut registry);
/// ```
#[deprecated(
    since = "0.4.0",
    note = "Use automatic registration via inventory. Just add candle-nodes as a dependency."
)]
pub fn register_candle_nodes(registry: &mut StreamingNodeRegistry) {
    CandleNodesProvider.register(registry);
}

/// Get a list of all registered Candle node types
pub fn list_candle_node_types() -> Vec<&'static str> {
    let mut types = Vec::new();

    #[cfg(feature = "whisper")]
    types.push("candle-whisper");

    #[cfg(feature = "yolo")]
    types.push("candle-yolo");

    #[cfg(feature = "llm")]
    {
        types.push("candle-phi");
        types.push("candle-llama");
    }

    #[cfg(feature = "vad")]
    types.push("candle-silero-vad");

    // Emotion vector nodes (always available)
    types.push("EmotionExtractorNode");
    types.push("EmotionSteeringNode");

    types
}

/// Check if a node type is a Candle node
pub fn is_candle_node(node_type: &str) -> bool {
    matches!(
        node_type,
        "candle-whisper" | "candle-yolo" | "candle-phi" | "candle-llama"
            | "candle-silero-vad" | "EmotionExtractorNode" | "EmotionSteeringNode"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_node_types() {
        let types = list_candle_node_types();
        // At minimum, should be empty if no features enabled
        assert!(types.len() <= 5);
    }

    #[test]
    fn test_is_candle_node() {
        assert!(is_candle_node("candle-whisper"));
        assert!(is_candle_node("candle-yolo"));
        assert!(!is_candle_node("some-other-node"));
    }

    #[test]
    fn test_provider_metadata() {
        let provider = CandleNodesProvider;
        assert_eq!(provider.provider_name(), "candle-nodes");
        assert_eq!(provider.priority(), 800);
    }
}
