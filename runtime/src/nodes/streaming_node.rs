//! Streaming node trait for generic data processing
//!
//! This module defines the trait for nodes that can participate in
//! real-time streaming pipelines with generic data types.

use crate::data::RuntimeData;
use crate::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Trait for nodes that can process generic streaming data
///
/// This trait is specifically designed for streaming RPC contexts
/// where nodes process chunks of data in real-time.
pub trait StreamingNode: Send + Sync {
    /// Get the node type name
    fn node_type(&self) -> &str;

    /// Process single-input data
    ///
    /// For nodes that accept a single input stream.
    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        Err(Error::Execution(format!(
            "Node {} does not support single-input processing",
            self.node_type()
        )))
    }

    /// Process multi-input data
    ///
    /// For nodes that require multiple named inputs (e.g., audio + video sync).
    fn process_multi(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, Error> {
        // Default: extract first input and process
        if let Some((_name, data)) = inputs.into_iter().next() {
            self.process(data)
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    /// Check if this node requires multiple inputs
    fn is_multi_input(&self) -> bool {
        false
    }
}

/// Factory trait for creating streaming node instances
pub trait StreamingNodeFactory: Send + Sync {
    /// Create a new streaming node instance
    fn create(&self, node_id: String, params: &Value) -> Result<Box<dyn StreamingNode>, Error>;

    /// Get the node type this factory creates
    fn node_type(&self) -> &str;
}

/// Registry for streaming nodes
pub struct StreamingNodeRegistry {
    factories: HashMap<String, Arc<dyn StreamingNodeFactory>>,
}

impl StreamingNodeRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a streaming node factory
    pub fn register(&mut self, factory: Arc<dyn StreamingNodeFactory>) {
        let node_type = factory.node_type().to_string();
        self.factories.insert(node_type, factory);
    }

    /// Create a streaming node by type
    pub fn create_node(
        &self,
        node_type: &str,
        node_id: String,
        params: &Value,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let factory = self.factories.get(node_type).ok_or_else(|| {
            Error::Execution(format!(
                "No streaming node factory registered for type '{}'. Available types: {:?}",
                node_type,
                self.list_types()
            ))
        })?;

        factory.create(node_id, params)
    }

    /// Check if a node type is registered
    pub fn has_node_type(&self, node_type: &str) -> bool {
        self.factories.contains_key(node_type)
    }

    /// List all registered node types
    pub fn list_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self.factories.keys().cloned().collect();
        types.sort();
        types
    }
}

impl Default for StreamingNodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
