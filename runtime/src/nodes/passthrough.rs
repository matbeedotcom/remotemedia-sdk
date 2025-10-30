//! PassThroughNode - Simple pass-through node for testing
//!
//! This node simply returns its input unchanged, useful for testing
//! the streaming pipeline infrastructure.

use crate::data::RuntimeData;
use crate::nodes::SyncStreamingNode;
use crate::Error;

/// PassThroughNode that returns input unchanged
pub struct PassThroughNode {
    pub id: String,
}

impl PassThroughNode {
    pub fn new(id: String, _params: &str) -> Result<Self, Error> {
        Ok(Self { id })
    }
}

impl SyncStreamingNode for PassThroughNode {
    fn node_type(&self) -> &str {
        "PassThrough"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Ok(data)
    }
}
