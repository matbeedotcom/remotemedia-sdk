//! Streaming node trait for generic data processing
//!
//! This module defines the traits for nodes that can participate in
//! real-time streaming pipelines with generic data types.

use crate::data::RuntimeData;
use crate::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Synchronous streaming node trait
///
/// Implement this for Rust nodes that can process data synchronously.
pub trait SyncStreamingNode: Send + Sync {
    /// Get the node type name
    fn node_type(&self) -> &str;

    /// Process single-input data
    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error>;

    /// Process multi-input data (for nodes that require multiple named inputs)
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

/// Asynchronous streaming node trait
///
/// Implement this for nodes that require async processing (e.g., Python nodes, I/O-bound operations).
#[async_trait::async_trait]
pub trait AsyncStreamingNode: Send + Sync {
    /// Get the node type name
    fn node_type(&self) -> &str;

    /// Process single-input data asynchronously
    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error>;

    /// Process multi-input data asynchronously
    async fn process_multi(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, Error> {
        // Default: extract first input and process
        if let Some((_name, data)) = inputs.into_iter().next() {
            self.process(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    /// Check if this node requires multiple inputs
    fn is_multi_input(&self) -> bool {
        false
    }
}

/// Unified streaming node trait that can handle both sync and async nodes
///
/// This is the trait used by the registry and pipeline executor.
/// It's automatically implemented for both SyncStreamingNode and AsyncStreamingNode.
#[async_trait::async_trait]
pub trait StreamingNode: Send + Sync {
    /// Get the node type name
    fn node_type(&self) -> &str;

    /// Process single-input data asynchronously
    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error>;

    /// Process multi-input data asynchronously
    async fn process_multi_async(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, Error>;

    /// Check if this node requires multiple inputs
    fn is_multi_input(&self) -> bool;
}

/// Wrapper that makes a SyncStreamingNode into a StreamingNode
pub struct SyncNodeWrapper<T: SyncStreamingNode>(pub T);

#[async_trait::async_trait]
impl<T: SyncStreamingNode + 'static> StreamingNode for SyncNodeWrapper<T> {
    fn node_type(&self) -> &str {
        self.0.node_type()
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        self.0.process(data)
    }

    async fn process_multi_async(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, Error> {
        self.0.process_multi(inputs)
    }

    fn is_multi_input(&self) -> bool {
        self.0.is_multi_input()
    }
}

/// Wrapper that makes an AsyncStreamingNode into a StreamingNode
pub struct AsyncNodeWrapper<T: AsyncStreamingNode>(pub Arc<T>);

#[async_trait::async_trait]
impl<T: AsyncStreamingNode + 'static> StreamingNode for AsyncNodeWrapper<T> {
    fn node_type(&self) -> &str {
        self.0.node_type()
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        self.0.process(data).await
    }

    async fn process_multi_async(&self, inputs: HashMap<String, RuntimeData>) -> Result<RuntimeData, Error> {
        self.0.process_multi(inputs).await
    }

    fn is_multi_input(&self) -> bool {
        self.0.is_multi_input()
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
