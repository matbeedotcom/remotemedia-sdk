//! llama.cpp nodes provider — registers all llama.cpp nodes

use crate::nodes::provider::NodeProvider;
use crate::nodes::streaming_node::StreamingNodeRegistry;
use std::sync::Arc;

use super::{
    LlamaCppActivationNodeFactory, LlamaCppEmbeddingNodeFactory,
    LlamaCppGenerationNodeFactory, LlamaCppSteerNodeFactory,
};

/// Provider for llama.cpp nodes.
///
/// Registers all four llama.cpp streaming nodes:
/// - LlamaCppGenerationNode — text generation
/// - LlamaCppEmbeddingNode — text embeddings
/// - LlamaCppActivationNode — activation extraction
/// - LlamaCppSteerNode — activation steering
pub struct LlamaCppNodesProvider;

impl NodeProvider for LlamaCppNodesProvider {
    fn register(&self, registry: &mut StreamingNodeRegistry) {
        registry.register(Arc::new(LlamaCppGenerationNodeFactory));
        registry.register(Arc::new(LlamaCppEmbeddingNodeFactory));
        registry.register(Arc::new(LlamaCppActivationNodeFactory));
        registry.register(Arc::new(LlamaCppSteerNodeFactory));
    }

    fn provider_name(&self) -> &'static str {
        "llama-cpp-nodes"
    }

    fn node_count(&self) -> usize {
        4
    }

    fn priority(&self) -> i32 {
        // High priority — ML nodes should be discoverable
        900
    }
}

// Auto-register the llama.cpp nodes provider
#[cfg(feature = "llama-cpp")]
inventory::submit! {
    &LlamaCppNodesProvider as &'static dyn NodeProvider
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_registers_nodes() {
        let mut registry = StreamingNodeRegistry::new();
        let provider = LlamaCppNodesProvider;

        provider.register(&mut registry);

        assert!(registry.has_node_type("LlamaCppGenerationNode"));
        assert!(registry.has_node_type("LlamaCppEmbeddingNode"));
        assert!(registry.has_node_type("LlamaCppActivationNode"));
        assert!(registry.has_node_type("LlamaCppSteerNode"));
    }

    #[test]
    fn test_provider_metadata() {
        let provider = LlamaCppNodesProvider;
        assert_eq!(provider.provider_name(), "llama-cpp-nodes");
        assert_eq!(provider.priority(), 900);
        assert_eq!(provider.node_count(), 4);
    }
}
