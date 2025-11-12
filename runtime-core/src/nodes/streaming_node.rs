//! Streaming node trait for generic data processing
//!
//! This module defines the traits for nodes that can participate in
//! real-time streaming pipelines with generic data types.

use crate::data::RuntimeData;
use crate::Error;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Node execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeStatus {
    /// Node created but not initialized
    Idle = 0,
    /// Node is currently initializing (loading models, resources, etc.)
    Initializing = 1,
    /// Node is initialized and ready to process data
    Ready = 2,
    /// Node is currently processing data
    Processing = 3,
    /// Node encountered an error
    Error = 4,
    /// Node is being cleaned up
    Stopping = 5,
    /// Node has been stopped/destroyed
    Stopped = 6,
}

impl NodeStatus {
    /// Convert from u8 (for atomic storage)
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => NodeStatus::Idle,
            1 => NodeStatus::Initializing,
            2 => NodeStatus::Ready,
            3 => NodeStatus::Processing,
            4 => NodeStatus::Error,
            5 => NodeStatus::Stopping,
            6 => NodeStatus::Stopped,
            _ => NodeStatus::Idle,
        }
    }

    /// Convert to string for logging/display
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeStatus::Idle => "idle",
            NodeStatus::Initializing => "initializing",
            NodeStatus::Ready => "ready",
            NodeStatus::Processing => "processing",
            NodeStatus::Error => "error",
            NodeStatus::Stopping => "stopping",
            NodeStatus::Stopped => "stopped",
        }
    }
}

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

    /// Initialize the node (load models, resources, etc.)
    async fn initialize(&self) -> Result<(), Error> {
        Ok(()) // Default: no-op
    }

    /// Process single-input data asynchronously
    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error>;

    /// Process multi-input data asynchronously
    async fn process_multi(
        &self,
        inputs: HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
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

    /// Process input and stream multiple outputs via callback
    ///
    /// This method allows nodes to produce multiple outputs from a single input (e.g., VAD events + pass-through audio).
    /// Nodes that produce multiple outputs should override this method.
    /// The default implementation just calls process() and invokes the callback once.
    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let output = self.process(data).await?;
        callback(output)?;
        Ok(1)
    }

    /// Process control message for pipeline flow control
    ///
    /// This method allows nodes to handle control messages such as:
    /// - CancelSpeculation: Cancel processing of a speculative segment
    /// - FlushBuffer: Flush any buffered data immediately
    /// - UpdatePolicy: Update batching or buffering policies
    ///
    /// Default implementation ignores control messages. Nodes that need to handle
    /// control messages should override this method.
    ///
    /// # Arguments
    /// * `message` - The control message data
    /// * `session_id` - Optional session ID for session-scoped control
    ///
    /// # Returns
    /// * `Ok(true)` if the message was handled
    /// * `Ok(false)` if the message was ignored
    /// * `Err(_)` if handling failed
    async fn process_control_message(
        &self,
        _message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool, Error> {
        // Default: ignore control messages
        // Nodes that need control message handling should override this
        Ok(false)
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

    /// Get the node ID
    fn node_id(&self) -> &str {
        "" // Default implementation - should be overridden by nodes that have IDs
    }

    /// Get the current execution status
    fn get_status(&self) -> NodeStatus {
        NodeStatus::Idle // Default for nodes without status tracking
    }

    /// Initialize the node (load models, resources, etc.)
    /// This is called during pre-initialization before streaming starts
    async fn initialize(&self) -> Result<(), Error> {
        Ok(()) // Default: no-op for nodes without initialization
    }

    /// Process single-input data asynchronously
    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error>;

    /// Process multi-input data asynchronously
    async fn process_multi_async(
        &self,
        inputs: HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error>;

    /// Check if this node requires multiple inputs
    fn is_multi_input(&self) -> bool;

    /// Process data with streaming callback (for multi-output nodes)
    /// Default implementation falls back to process_async
    async fn process_streaming_async(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        callback: Box<dyn FnMut(RuntimeData) -> Result<(), Error> + Send>,
    ) -> Result<usize, Error> {
        let mut callback = callback;
        let output = self.process_async(data).await?;
        callback(output)?;
        Ok(1)
    }

    /// Process control message for pipeline flow control
    ///
    /// This method allows nodes to handle control messages such as:
    /// - CancelSpeculation: Cancel processing of a speculative segment
    /// - FlushBuffer: Flush any buffered data immediately
    /// - UpdatePolicy: Update batching or buffering policies
    ///
    /// Default implementation ignores control messages. Nodes that need to handle
    /// control messages should override this method.
    ///
    /// # Arguments
    /// * `message` - The control message data
    /// * `session_id` - Optional session ID for session-scoped control
    ///
    /// # Returns
    /// * `Ok(true)` if the message was handled
    /// * `Ok(false)` if the message was ignored
    /// * `Err(_)` if handling failed
    async fn process_control_message(
        &self,
        _message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool, Error> {
        // Default: ignore control messages
        // Nodes that need control message handling should override this
        Ok(false)
    }
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

    async fn process_multi_async(
        &self,
        inputs: HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
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

    async fn initialize(&self) -> Result<(), Error> {
        self.0.initialize().await
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        self.0.process(data).await
    }

    async fn process_multi_async(
        &self,
        inputs: HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        self.0.process_multi(inputs).await
    }

    fn is_multi_input(&self) -> bool {
        self.0.is_multi_input()
    }

    async fn process_streaming_async(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: Box<dyn FnMut(RuntimeData) -> Result<(), Error> + Send>,
    ) -> Result<usize, Error> {
        // Create an adapter closure that calls the boxed callback
        self.0
            .process_streaming(data, session_id, move |output_data| callback(output_data))
            .await
    }
}

/// Factory trait for creating streaming node instances
pub trait StreamingNodeFactory: Send + Sync {
    /// Create a new streaming node instance
    ///
    /// # Arguments
    /// * `node_id` - Unique identifier for this node instance
    /// * `params` - Node initialization parameters
    /// * `session_id` - Optional session ID for multiprocess execution
    fn create(
        &self,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error>;

    /// Get the node type this factory creates
    fn node_type(&self) -> &str;

    /// Check if this factory creates Python-based nodes
    /// Python nodes need special handling for caching the unwrapped instance
    fn is_python_node(&self) -> bool {
        false // Default: not a Python node
    }

    /// Check if this factory creates multi-output streaming nodes
    /// Multi-output nodes produce multiple outputs per input (e.g., VAD produces events + pass-through audio)
    fn is_multi_output_streaming(&self) -> bool {
        false // Default: single output
    }
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
    ///
    /// # Arguments
    /// * `node_type` - The type of node to create
    /// * `node_id` - Unique identifier for this node instance
    /// * `params` - Node initialization parameters
    /// * `session_id` - Optional session ID for multiprocess execution
    pub fn create_node(
        &self,
        node_type: &str,
        node_id: String,
        params: &Value,
        session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let factory = self.factories.get(node_type).ok_or_else(|| {
            Error::Execution(format!(
                "No streaming node factory registered for type '{}'. Available types: {:?}",
                node_type,
                self.list_types()
            ))
        })?;

        factory.create(node_id, params, session_id)
    }

    /// Check if a node type is registered
    pub fn has_node_type(&self, node_type: &str) -> bool {
        self.factories.contains_key(node_type)
    }

    /// Check if a node type is a Python-based node
    pub fn is_python_node(&self, node_type: &str) -> bool {
        self.factories
            .get(node_type)
            .map(|factory| factory.is_python_node())
            .unwrap_or(false)
    }

    /// Check if a node type is a multi-output streaming node
    pub fn is_multi_output_streaming(&self, node_type: &str) -> bool {
        self.factories
            .get(node_type)
            .map(|factory| factory.is_multi_output_streaming())
            .unwrap_or(false)
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
