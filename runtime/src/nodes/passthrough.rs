//! PassThroughNode - Simple pass-through node for testing
//!
//! This node simply returns its input unchanged, useful for testing
//! the streaming pipeline infrastructure.

use crate::data::RuntimeData;
use crate::nodes::StreamingNode;
use crate::Error;

/// PassThroughNode that returns input unchanged
pub struct PassThroughNode {
    pub id: String,
}

impl PassThroughNode {
    pub fn new(id: String, _params: &str) -> Result<Self, Error> {
        Ok(Self { id })
    }

    pub fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}

impl StreamingNode for PassThroughNode {
    fn node_type(&self) -> &str {
        "PassThrough"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        PassThroughNode::process(self, data)
    }
}
