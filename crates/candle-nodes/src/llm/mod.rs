//! LLM text generation nodes (placeholder)
//!
//! Full implementation in Phase 5 (User Story 3)

mod config;

pub use config::{GenerationConfig, LlmConfig};

use remotemedia_core::capabilities::CapabilityBehavior;
use remotemedia_core::nodes::streaming_node::StreamingNodeFactory;
use remotemedia_core::Error;
use serde_json::Value;

/// Phi LLM node (placeholder)
pub struct PhiNode;

/// LLaMA LLM node (placeholder)  
pub struct LlamaNode;

/// Phi node factory
pub struct PhiNodeFactory;

impl PhiNodeFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PhiNodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingNodeFactory for PhiNodeFactory {
    fn create(
        &self,
        _node_id: String,
        _params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn remotemedia_core::nodes::streaming_node::StreamingNode>, Error> {
        Err(Error::Execution("Phi node not yet implemented".to_string()))
    }

    fn node_type(&self) -> &str {
        "candle-phi"
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}

/// LLaMA node factory
pub struct LlamaNodeFactory;

impl LlamaNodeFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LlamaNodeFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingNodeFactory for LlamaNodeFactory {
    fn create(
        &self,
        _node_id: String,
        _params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn remotemedia_core::nodes::streaming_node::StreamingNode>, Error> {
        Err(Error::Execution("LLaMA node not yet implemented".to_string()))
    }

    fn node_type(&self) -> &str {
        "candle-llama"
    }

    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Static
    }
}
