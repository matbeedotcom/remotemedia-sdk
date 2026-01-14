//! Node factory registration for Candle ML nodes
//!
//! Provides factory registration helpers for integrating Candle nodes
//! with the RemoteMedia StreamingNodeRegistry.

use remotemedia_core::nodes::streaming_node::{StreamingNodeFactory, StreamingNodeRegistry};
use std::sync::Arc;

/// Trait for Candle node factories
pub trait CandleNodeFactory: StreamingNodeFactory {
    /// Get the model family this factory creates nodes for
    fn model_family(&self) -> &'static str;
    
    /// Get supported model variants
    fn supported_variants(&self) -> &[&'static str];
}

/// Register all enabled Candle nodes with a registry
///
/// This function registers node factories based on enabled feature flags.
///
/// # Example
///
/// ```ignore
/// let mut registry = StreamingNodeRegistry::new();
/// register_candle_nodes(&mut registry);
/// ```
pub fn register_candle_nodes(registry: &mut StreamingNodeRegistry) {
    #[cfg(feature = "whisper")]
    {
        use crate::whisper::WhisperNodeFactory;
        registry.register(Arc::new(WhisperNodeFactory::new()));
        tracing::info!("Registered candle-whisper node factory");
    }

    #[cfg(feature = "yolo")]
    {
        use crate::yolo::YoloNodeFactory;
        registry.register(Arc::new(YoloNodeFactory::new()));
        tracing::info!("Registered candle-yolo node factory");
    }

    #[cfg(feature = "llm")]
    {
        use crate::llm::{PhiNodeFactory, LlamaNodeFactory};
        registry.register(Arc::new(PhiNodeFactory::new()));
        registry.register(Arc::new(LlamaNodeFactory::new()));
        tracing::info!("Registered candle-phi and candle-llama node factories");
    }
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
    
    types
}

/// Check if a node type is a Candle node
pub fn is_candle_node(node_type: &str) -> bool {
    matches!(
        node_type,
        "candle-whisper" | "candle-yolo" | "candle-phi" | "candle-llama"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_node_types() {
        let types = list_candle_node_types();
        // At minimum, should be empty if no features enabled
        assert!(types.len() <= 4);
    }

    #[test]
    fn test_is_candle_node() {
        assert!(is_candle_node("candle-whisper"));
        assert!(is_candle_node("candle-yolo"));
        assert!(!is_candle_node("some-other-node"));
    }
}
