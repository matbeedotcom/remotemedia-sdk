//! Default streaming node registry with built-in node factories

use crate::nodes::calculator::CalculatorNode;
use crate::nodes::passthrough::PassThroughNode;
use crate::nodes::sync_av::SynchronizedAudioVideoNode;
use crate::nodes::video_processor::VideoProcessorNode;
use crate::nodes::{StreamingNode, StreamingNodeFactory, StreamingNodeRegistry};
use crate::Error;
use serde_json::Value;
use std::sync::Arc;

// Factory implementations for built-in streaming nodes

struct CalculatorNodeFactory;
impl StreamingNodeFactory for CalculatorNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        Ok(Box::new(CalculatorNode::new(node_id, &params_str)?))
    }

    fn node_type(&self) -> &str {
        "CalculatorNode"
    }
}

struct VideoProcessorNodeFactory;
impl StreamingNodeFactory for VideoProcessorNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        Ok(Box::new(VideoProcessorNode::new(node_id, &params_str)?))
    }

    fn node_type(&self) -> &str {
        "VideoProcessorNode"
    }
}

struct SynchronizedAudioVideoNodeFactory;
impl StreamingNodeFactory for SynchronizedAudioVideoNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        Ok(Box::new(SynchronizedAudioVideoNode::new(node_id, &params_str)?))
    }

    fn node_type(&self) -> &str {
        "SynchronizedAudioVideoNode"
    }
}

struct PassThroughNodeFactory;
impl StreamingNodeFactory for PassThroughNodeFactory {
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error> {
        let params_str = params.to_string();
        Ok(Box::new(PassThroughNode::new(node_id, &params_str)?))
    }

    fn node_type(&self) -> &str {
        "PassThrough"
    }
}

/// Create a default streaming node registry with all built-in nodes registered
pub fn create_default_streaming_registry() -> StreamingNodeRegistry {
    let mut registry = StreamingNodeRegistry::new();

    // Register all built-in streaming nodes
    registry.register(Arc::new(CalculatorNodeFactory));
    registry.register(Arc::new(VideoProcessorNodeFactory));
    registry.register(Arc::new(SynchronizedAudioVideoNodeFactory));
    registry.register(Arc::new(PassThroughNodeFactory));

    registry
}
