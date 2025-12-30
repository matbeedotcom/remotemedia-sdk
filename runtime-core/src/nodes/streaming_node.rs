//! Streaming node trait for generic data processing
//!
//! This module defines the traits for nodes that can participate in
//! real-time streaming pipelines with generic data types.
//!
//! # Capability Resolution (spec 023)
//!
//! Nodes can declare their media capabilities via trait methods:
//! - `media_capabilities()` - Returns input/output constraints
//! - `capability_behavior()` - Returns how capabilities are determined
//! - `potential_capabilities()` - For RuntimeDiscovered nodes (Phase 1)
//! - `actual_capabilities()` - For RuntimeDiscovered nodes (Phase 2)

use crate::capabilities::{CapabilityBehavior, MediaCapabilities};
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

    // =========================================================================
    // Capability Resolution Methods (spec 023)
    // =========================================================================

    /// Return media capabilities for this node instance.
    ///
    /// Override this method to declare the node's input requirements and
    /// output capabilities. The default returns `None`, which is treated
    /// as passthrough behavior (output matches input).
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn media_capabilities(&self) -> Option<MediaCapabilities> {
    ///     Some(MediaCapabilities::with_input(
    ///         MediaConstraints::Audio(AudioConstraints {
    ///             sample_rate: Some(ConstraintValue::Exact(16000)),
    ///             channels: Some(ConstraintValue::Exact(1)),
    ///             format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
    ///         })
    ///     ))
    /// }
    /// ```
    fn media_capabilities(&self) -> Option<MediaCapabilities> {
        None
    }

    /// Return capability behavior for this node.
    ///
    /// This determines how the pipeline resolver resolves this node's
    /// capabilities during pipeline construction.
    ///
    /// Default: `Passthrough` (output matches input)
    ///
    /// # Behaviors
    ///
    /// - `Static` - Fixed capabilities from `media_capabilities()`
    /// - `Configured` - Capabilities from factory's `media_capabilities(params)`
    /// - `Passthrough` - Output inherits from upstream
    /// - `Adaptive` - Output adapts to downstream requirements
    /// - `RuntimeDiscovered` - Two-phase: potential then actual capabilities
    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Passthrough
    }

    /// Return potential capabilities for RuntimeDiscovered nodes (Phase 1).
    ///
    /// For nodes with `RuntimeDiscovered` behavior, this method returns
    /// a broad range of capabilities for early validation before the
    /// device is actually initialized.
    ///
    /// Default: Returns `media_capabilities()` result.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn potential_capabilities(&self) -> Option<MediaCapabilities> {
    ///     // Return broad range - actual device may be more restrictive
    ///     Some(MediaCapabilities::with_output(
    ///         MediaConstraints::Audio(AudioConstraints {
    ///             sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
    ///             channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
    ///             format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
    ///         })
    ///     ))
    /// }
    /// ```
    fn potential_capabilities(&self) -> Option<MediaCapabilities> {
        self.media_capabilities()
    }

    /// Return actual capabilities after device init (Phase 2).
    ///
    /// For nodes with `RuntimeDiscovered` behavior, this method returns
    /// the actual capabilities discovered after `initialize()` completes.
    /// The pipeline will re-validate using these capabilities.
    ///
    /// Default: Returns `media_capabilities()` result.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn actual_capabilities(&self) -> Option<MediaCapabilities> {
    ///     // Return actual device capabilities discovered during init
    ///     self.discovered_capabilities.clone()
    /// }
    /// ```
    fn actual_capabilities(&self) -> Option<MediaCapabilities> {
        self.media_capabilities()
    }

    /// Configure this node based on upstream capabilities (spec 025).
    ///
    /// Called by `SessionRouter` after a RuntimeDiscovered upstream node reports
    /// its actual capabilities. This allows Adaptive and Passthrough nodes to
    /// configure themselves based on actual upstream values before data processing.
    ///
    /// Default: No-op. Override in Adaptive/Passthrough nodes that need upstream info.
    ///
    /// # Arguments
    /// * `upstream_caps` - The actual output capabilities from the upstream node
    ///
    /// # Returns
    /// * `Ok(())` if configuration succeeded
    /// * `Err(_)` if the node cannot accept the upstream capabilities
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn configure_from_upstream(&self, upstream_caps: &MediaCapabilities) -> Result<(), Error> {
    ///     if let Some(MediaConstraints::Audio(audio)) = upstream_caps.default_output() {
    ///         if let Some(ConstraintValue::Exact(rate)) = &audio.sample_rate {
    ///             self.set_source_rate(*rate);
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn configure_from_upstream(&self, _upstream_caps: &MediaCapabilities) -> Result<(), Error> {
        Ok(()) // Default: no-op for nodes that don't need upstream configuration
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

    /// Get the node schema for type generation.
    ///
    /// Override this to provide schema metadata (description, accepts/produces,
    /// config parameters) that gets exported to TypeScript types via NAPI.
    ///
    /// Returns None by default - nodes without schema info are still usable
    /// but won't have typed configs in generated TypeScript.
    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        None
    }

    // =========================================================================
    // Capability Resolution Methods (spec 023)
    // =========================================================================

    /// Return media capabilities for nodes created by this factory.
    ///
    /// Called with params during resolution (before node instantiation).
    /// This allows capability resolution to happen before nodes are created,
    /// enabling early validation during pipeline construction.
    ///
    /// Default: Returns `None` (passthrough behavior).
    ///
    /// # Arguments
    /// * `params` - Node configuration parameters from manifest
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn media_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
    ///     let sample_rate = params.get("sample_rate")
    ///         .and_then(|v| v.as_u64())
    ///         .unwrap_or(48000) as u32;
    ///
    ///     Some(MediaCapabilities::with_output(
    ///         MediaConstraints::Audio(AudioConstraints {
    ///             sample_rate: Some(ConstraintValue::Exact(sample_rate)),
    ///             channels: Some(ConstraintValue::Exact(1)),
    ///             format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
    ///         })
    ///     ))
    /// }
    /// ```
    fn media_capabilities(&self, _params: &Value) -> Option<MediaCapabilities> {
        None
    }

    /// Return capability behavior for nodes created by this factory.
    ///
    /// This determines how the pipeline resolver resolves capabilities
    /// for nodes created by this factory.
    ///
    /// Default: `Passthrough` (output matches input)
    ///
    /// # Behaviors
    ///
    /// - `Static` - Fixed capabilities, same for all instances
    /// - `Configured` - Capabilities depend on params (use `media_capabilities(params)`)
    /// - `Passthrough` - Output inherits from upstream
    /// - `Adaptive` - Output adapts to downstream requirements
    /// - `RuntimeDiscovered` - Two-phase: potential then actual capabilities
    fn capability_behavior(&self) -> CapabilityBehavior {
        CapabilityBehavior::Passthrough
    }

    /// Return potential capabilities for RuntimeDiscovered nodes (Phase 1).
    ///
    /// For factories that create `RuntimeDiscovered` nodes, this method returns
    /// a broad range of capabilities for early validation before the device
    /// is actually initialized.
    ///
    /// Default: Returns `media_capabilities(params)` result.
    ///
    /// # Arguments
    /// * `params` - Node configuration parameters from manifest
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn potential_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
    ///     // Return broad range - actual device may be more restrictive
    ///     Some(MediaCapabilities::with_output(
    ///         MediaConstraints::Audio(AudioConstraints {
    ///             sample_rate: Some(ConstraintValue::Range { min: 8000, max: 192000 }),
    ///             channels: Some(ConstraintValue::Range { min: 1, max: 8 }),
    ///             format: Some(ConstraintValue::Exact(AudioSampleFormat::F32)),
    ///         })
    ///     ))
    /// }
    /// ```
    fn potential_capabilities(&self, params: &Value) -> Option<MediaCapabilities> {
        self.media_capabilities(params)
    }
}

/// Registry for streaming nodes
#[derive(Clone)]
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

    /// Collect all schemas from registered factories.
    ///
    /// This allows building a schema registry dynamically from factory metadata
    /// rather than maintaining a separate manual registry.
    pub fn collect_schemas(&self) -> Vec<crate::nodes::schema::NodeSchema> {
        self.factories
            .values()
            .filter_map(|factory| factory.schema())
            .collect()
    }

    // =========================================================================
    // Capability Resolution Methods (spec 023)
    // =========================================================================

    /// Get the factory for a node type.
    ///
    /// Used by `CapabilityResolver` to access factory methods during resolution.
    pub fn get_factory(&self, node_type: &str) -> Option<&Arc<dyn StreamingNodeFactory>> {
        self.factories.get(node_type)
    }

    /// Get capability behavior for a node type.
    ///
    /// Returns `Passthrough` if the node type is not registered.
    pub fn get_capability_behavior(&self, node_type: &str) -> CapabilityBehavior {
        self.factories
            .get(node_type)
            .map(|f| f.capability_behavior())
            .unwrap_or(CapabilityBehavior::Passthrough)
    }

    /// Get media capabilities for a node type with params.
    ///
    /// Returns `None` if the node type is not registered or has no declared capabilities.
    pub fn get_media_capabilities(&self, node_type: &str, params: &Value) -> Option<MediaCapabilities> {
        self.factories
            .get(node_type)
            .and_then(|f| f.media_capabilities(params))
    }

    /// Get potential capabilities for RuntimeDiscovered nodes (Phase 1).
    ///
    /// Returns broad capabilities for early validation before device initialization.
    /// For non-RuntimeDiscovered nodes, returns `media_capabilities(params)`.
    pub fn get_potential_capabilities(&self, node_type: &str, params: &Value) -> Option<MediaCapabilities> {
        self.factories
            .get(node_type)
            .and_then(|f| f.potential_capabilities(params))
    }
}

impl Default for StreamingNodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
