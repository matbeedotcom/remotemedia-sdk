//! Node type implementations
//!
//! This module contains the core node execution logic and lifecycle management.
//!
//! **Note**: The NodeExecutor trait here is DEPRECATED and kept only for backward compatibility.
//! All new code should use `executor::node_executor::NodeExecutor` instead.
//! Built-in nodes have been migrated to the new trait. This old trait definition
//! and NodeContext will be removed in v0.3.0.

use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// Sub-modules
pub mod audio;
pub mod calculator;
pub mod passthrough;
pub mod python_nodes;
pub mod python_streaming;
pub mod registration_macros;
pub mod registry;
pub mod remote_pipeline;
pub mod streaming_node;
pub mod streaming_registry;

// Video codec support (spec 012)
#[cfg(feature = "video")]
pub mod video;

// Temporarily disabled - incomplete implementation
// pub mod sync_av;
// pub mod video_processor;

pub mod whisper;
pub use whisper::RustWhisperNode;

#[cfg(feature = "silero-vad")]
pub mod silero_vad;
#[cfg(feature = "silero-vad")]
pub use silero_vad::SileroVADNode;

pub mod audio_buffer_accumulator;
pub use audio_buffer_accumulator::AudioBufferAccumulatorNode;

pub mod audio_chunker;
pub use audio_chunker::AudioChunkerNode;

pub mod audio_resample_streaming;
pub use audio_resample_streaming::ResampleStreamingNode;

pub mod text_collector;
pub use text_collector::TextCollectorNode;

pub mod video_flip;
pub use video_flip::VideoFlipNode;

// Low-latency streaming nodes (spec 007)
pub mod speculative_vad_gate;
pub use speculative_vad_gate::{SpeculativeVADGate, VADResult};

pub use registry::{CompositeRegistry, NodeFactory as NodeFactoryTrait, RuntimeHint};
pub use streaming_node::{
    AsyncNodeWrapper, AsyncStreamingNode, StreamingNode, StreamingNodeFactory,
    StreamingNodeRegistry, SyncNodeWrapper, SyncStreamingNode,
};

/// Node execution context containing runtime state
#[derive(Debug, Clone)]
pub struct NodeContext {
    /// Node ID
    pub node_id: String,

    /// Node type
    pub node_type: String,

    /// Node parameters from manifest
    pub params: Value,

    /// Session ID for stateful execution
    pub session_id: Option<String>,

    /// Additional metadata
    pub metadata: HashMap<String, Value>,
}

/// Node lifecycle trait
///
/// All executable nodes must implement this trait to participate
/// in the pipeline execution lifecycle.
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Initialize the node
    ///
    /// Called once before any processing. Use this to:
    /// - Load models/resources
    /// - Validate configuration
    /// - Set up state
    async fn initialize(&mut self, context: &NodeContext) -> Result<()>;

    /// Process a single data item
    ///
    /// Called for each item flowing through the pipeline.
    /// Return None to filter out the item.
    ///
    /// For streaming nodes (async generators), this returns a Vec with multiple items.
    /// For non-streaming nodes, this returns a single-item Vec or empty Vec.
    async fn process(&mut self, input: Value) -> Result<Vec<Value>>;

    /// Cleanup resources
    ///
    /// Called once when the node is done processing.
    /// Use this to:
    /// - Release resources
    /// - Save state
    /// - Close connections
    async fn cleanup(&mut self) -> Result<()>;

    /// Check if this is a streaming node
    ///
    /// Streaming nodes accumulate inputs and yield multiple outputs.
    /// The executor will feed all inputs first, then collect all outputs.
    fn is_streaming(&self) -> bool {
        false
    }

    /// Finish streaming and collect remaining outputs
    ///
    /// For streaming nodes, signals that no more inputs will be provided
    /// and collects any buffered outputs. For non-streaming nodes, this
    /// returns an empty vector.
    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        Ok(vec![])
    }

    /// Downcast support for accessing concrete types
    /// Implementers should simply return `self`
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Get node information
    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "UnknownNode".to_string(),
            version: "0.1.0".to_string(),
            description: None,
        }
    }
}

/// Node information
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

/// Node factory for creating node instances
pub type NodeFactory = Box<dyn Fn() -> Box<dyn NodeExecutor> + Send + Sync>;

/// Registry for node types
pub struct NodeRegistry {
    factories: HashMap<String, NodeFactory>,
}

impl NodeRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a node type
    pub fn register<F>(&mut self, node_type: &str, factory: F)
    where
        F: Fn() -> Box<dyn NodeExecutor> + Send + Sync + 'static,
    {
        self.factories
            .insert(node_type.to_string(), Box::new(factory));
    }

    /// Create a node instance
    pub fn create(&self, node_type: &str) -> Result<Box<dyn NodeExecutor>> {
        self.factories
            .get(node_type)
            .map(|factory| factory())
            .ok_or_else(|| Error::Manifest(format!("Unknown node type: {}", node_type)))
    }

    /// Check if a node type is registered
    pub fn has_node_type(&self, node_type: &str) -> bool {
        self.factories.contains_key(node_type)
    }

    /// Get all registered node types
    pub fn node_types(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

impl Default for NodeRegistry {
    fn default() -> Self {
        // DEPRECATED: This old registry is no longer used.
        // Built-in nodes are now registered via create_builtin_registry()
        // which uses the new trait and factory pattern.
        // Keeping this empty to maintain API compatibility.
        Self::new()
    }
}

// ============================================================================
// Built-in Node Implementations
// ============================================================================

/// Simple pass-through node for testing
pub struct PassThroughNode;

#[async_trait]
impl NodeExecutor for PassThroughNode {
    async fn initialize(&mut self, _context: &NodeContext) -> Result<()> {
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        Ok(vec![input])
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Echo node that wraps input in a JSON object
pub struct EchoNode {
    counter: usize,
}

impl EchoNode {
    pub fn new() -> Self {
        Self { counter: 0 }
    }
}

/// Calculator node for basic arithmetic operations
pub struct CalculatorNode {
    operation: String,
    operand: f64,
}

impl CalculatorNode {
    pub fn new() -> Self {
        Self {
            operation: "add".to_string(),
            operand: 0.0,
        }
    }
}

#[async_trait]
impl NodeExecutor for EchoNode {
    async fn initialize(&mut self, _context: &NodeContext) -> Result<()> {
        self.counter = 0;
        tracing::info!("EchoNode initialized");
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        self.counter += 1;
        return Ok(vec![serde_json::json!({
            "input": input,
            "counter": self.counter,
            "node": "Echo"
        })]);
    }

    async fn cleanup(&mut self) -> Result<()> {
        tracing::info!("EchoNode processed {} items", self.counter);
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[async_trait]
impl NodeExecutor for CalculatorNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        // Extract parameters from context
        if let Some(operation) = context.params.get("operation") {
            if let Some(op_str) = operation.as_str() {
                self.operation = op_str.to_string();
            }
        }

        if let Some(operand) = context.params.get("operand") {
            if let Some(op_num) = operand.as_f64() {
                self.operand = op_num;
            } else if let Some(op_int) = operand.as_i64() {
                self.operand = op_int as f64;
            }
        }

        tracing::info!(
            "CalculatorNode initialized: operation={}, operand={}",
            self.operation,
            self.operand
        );
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        // Convert input to number
        let num = match input {
            Value::Number(n) => n.as_f64().unwrap_or(0.0),
            Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
            _ => return Ok(vec![input]), // Pass through non-numeric values
        };

        // Perform operation
        let result = match self.operation.as_str() {
            "add" => num + self.operand,
            "subtract" => num - self.operand,
            "multiply" => num * self.operand,
            "divide" => {
                if self.operand != 0.0 {
                    num / self.operand
                } else {
                    return Err(Error::Execution("Division by zero".to_string()));
                }
            }
            _ => num, // Unknown operation, pass through
        };

        // Convert result back to JSON value
        let output = if result.fract() == 0.0 && result.abs() < (i64::MAX as f64) {
            // Return as integer if it's a whole number
            Value::Number(serde_json::Number::from(result as i64))
        } else {
            // Return as float
            serde_json::Number::from_f64(result)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        };

        Ok(vec![output])
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ============================================================================
// Simple Math Nodes (optimized Rust implementations)
// ============================================================================

/// Multiply node - multiplies input by a factor
#[derive(Debug)]
pub struct MultiplyNode {
    factor: f64,
}

impl MultiplyNode {
    pub fn new() -> Self {
        Self { factor: 2.0 }
    }
}

#[async_trait]
impl NodeExecutor for MultiplyNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        // Extract factor from parameters
        if let Some(factor) = context.params.get("factor") {
            if let Some(f) = factor.as_f64() {
                self.factor = f;
            } else if let Some(i) = factor.as_i64() {
                self.factor = i as f64;
            }
        }

        tracing::info!("MultiplyNode initialized with factor={}", self.factor);
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        let output = match input {
            // Handle single number
            Value::Number(n) => {
                let num = n.as_f64().unwrap_or(0.0);
                let result = num * self.factor;

                if result.fract() == 0.0 && result.abs() < (i64::MAX as f64) {
                    Value::Number(serde_json::Number::from(result as i64))
                } else {
                    serde_json::Number::from_f64(result)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }
            }
            // Handle array of numbers
            Value::Array(arr) => {
                let results: Vec<Value> = arr
                    .into_iter()
                    .map(|v| {
                        if let Value::Number(n) = v {
                            let num = n.as_f64().unwrap_or(0.0);
                            let result = num * self.factor;

                            if result.fract() == 0.0 && result.abs() < (i64::MAX as f64) {
                                Value::Number(serde_json::Number::from(result as i64))
                            } else {
                                serde_json::Number::from_f64(result)
                                    .map(Value::Number)
                                    .unwrap_or(Value::Null)
                            }
                        } else {
                            v // Pass through non-numeric values
                        }
                    })
                    .collect();
                Value::Array(results)
            }
            // Pass through other types unchanged
            other => other,
        };

        Ok(vec![output])
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Add node - adds a constant to input
#[derive(Debug)]
pub struct AddNode {
    addend: f64,
}

impl AddNode {
    pub fn new() -> Self {
        Self { addend: 0.0 }
    }
}

#[async_trait]
impl NodeExecutor for AddNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        // Extract addend from parameters
        if let Some(addend) = context.params.get("addend") {
            if let Some(a) = addend.as_f64() {
                self.addend = a;
            } else if let Some(i) = addend.as_i64() {
                self.addend = i as f64;
            }
        }

        tracing::info!("AddNode initialized with addend={}", self.addend);
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        let output = match input {
            // Handle single number
            Value::Number(n) => {
                let num = n.as_f64().unwrap_or(0.0);
                let result = num + self.addend;

                if result.fract() == 0.0 && result.abs() < (i64::MAX as f64) {
                    Value::Number(serde_json::Number::from(result as i64))
                } else {
                    serde_json::Number::from_f64(result)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }
            }
            // Handle array of numbers
            Value::Array(arr) => {
                let results: Vec<Value> = arr
                    .into_iter()
                    .map(|v| {
                        if let Value::Number(n) = v {
                            let num = n.as_f64().unwrap_or(0.0);
                            let result = num + self.addend;

                            if result.fract() == 0.0 && result.abs() < (i64::MAX as f64) {
                                Value::Number(serde_json::Number::from(result as i64))
                            } else {
                                serde_json::Number::from_f64(result)
                                    .map(Value::Number)
                                    .unwrap_or(Value::Null)
                            }
                        } else {
                            v // Pass through non-numeric values
                        }
                    })
                    .collect();
                Value::Array(results)
            }
            // Pass through other types unchanged
            other => other,
        };

        Ok(vec![output])
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_passthrough_node() {
        let mut node = PassThroughNode;
        let context = NodeContext {
            node_id: "test".to_string(),
            node_type: "PassThrough".to_string(),
            params: Value::Null,
            session_id: None,
            metadata: HashMap::new(),
        };

        node.initialize(&context).await.unwrap();

        let input = serde_json::json!({"test": "data"});
        let output = node.process(input.clone()).await.unwrap();

        assert_eq!(output, vec![input]);

        node.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_echo_node() {
        let mut node = EchoNode::new();
        let context = NodeContext {
            node_id: "echo1".to_string(),
            node_type: "Echo".to_string(),
            params: Value::Null,
            session_id: None,
            metadata: HashMap::new(),
        };

        node.initialize(&context).await.unwrap();

        let input = serde_json::json!("hello");
        let output = node.process(input.clone()).await.unwrap();
        assert!(!output.is_empty());
        let output_obj = &output[0];

        assert_eq!(output_obj["input"], input);
        assert_eq!(output_obj["counter"], 1);
        assert_eq!(output_obj["node"], "Echo");

        // Process another item
        let output2 = node.process(serde_json::json!("world")).await.unwrap();
        assert!(!output2.is_empty());
        assert_eq!(output2[0]["counter"], 2);

        node.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_node_registry() {
        use crate::nodes::registry::RuntimeHint;

        // Use create_builtin_registry() which registers built-in nodes
        let registry = create_builtin_registry();

        assert!(registry.has_rust_impl("PassThrough"));
        assert!(registry.has_rust_impl("Echo"));
        assert!(!registry.has_rust_impl("NonExistent"));

        // Verify we can create a node (no info() method on NodeExecutor trait)
        let node_result =
            registry.create_node("PassThrough", RuntimeHint::Rust, serde_json::Value::Null);
        assert!(node_result.is_ok());
    }

    #[tokio::test]
    async fn test_registry_create_unknown() {
        use crate::nodes::registry::RuntimeHint;

        let registry = create_builtin_registry();
        let result =
            registry.create_node("UnknownNode", RuntimeHint::Rust, serde_json::Value::Null);
        assert!(result.is_err());
    }
}

// ============================================================================
// Node Factories for New Trait (executor::node_executor::NodeExecutor)
// ============================================================================

/// Create a registry with all built-in test nodes using the new trait
///
/// Registers: PassThroughNode, Echo, CalculatorNode, AddNode, MultiplyNode
pub fn create_builtin_registry() -> registry::NodeRegistry {
    let mut reg = registry::NodeRegistry::new();

    // Register simple test nodes
    reg.register_rust(Arc::new(PassThroughNodeFactory));
    reg.register_rust(Arc::new(EchoNodeFactory));
    reg.register_rust(Arc::new(CalculatorNodeFactory));
    reg.register_rust(Arc::new(AddNodeFactory));
    reg.register_rust(Arc::new(MultiplyNodeFactory));

    reg
}

// Factory implementations for test nodes (new trait)
struct PassThroughNodeFactory;
struct EchoNodeFactory;
struct CalculatorNodeFactory;
struct AddNodeFactory;
struct MultiplyNodeFactory;

impl NodeFactoryTrait for PassThroughNodeFactory {
    fn create(
        &self,
        _params: Value,
    ) -> Result<Box<dyn crate::executor::node_executor::NodeExecutor>> {
        Ok(Box::new(PassThroughNodeNew))
    }
    fn node_type(&self) -> &str {
        "PassThrough"
    }
}

impl NodeFactoryTrait for EchoNodeFactory {
    fn create(
        &self,
        _params: Value,
    ) -> Result<Box<dyn crate::executor::node_executor::NodeExecutor>> {
        Ok(Box::new(EchoNodeNew::new()))
    }
    fn node_type(&self) -> &str {
        "Echo"
    }
}

impl NodeFactoryTrait for CalculatorNodeFactory {
    fn create(
        &self,
        _params: Value,
    ) -> Result<Box<dyn crate::executor::node_executor::NodeExecutor>> {
        Ok(Box::new(CalculatorNodeNew::new()))
    }
    fn node_type(&self) -> &str {
        "CalculatorNode"
    }
}

impl NodeFactoryTrait for AddNodeFactory {
    fn create(
        &self,
        _params: Value,
    ) -> Result<Box<dyn crate::executor::node_executor::NodeExecutor>> {
        Ok(Box::new(AddNodeNew::new()))
    }
    fn node_type(&self) -> &str {
        "AddNode"
    }
}

impl NodeFactoryTrait for MultiplyNodeFactory {
    fn create(
        &self,
        _params: Value,
    ) -> Result<Box<dyn crate::executor::node_executor::NodeExecutor>> {
        Ok(Box::new(MultiplyNodeNew::new()))
    }
    fn node_type(&self) -> &str {
        "MultiplyNode"
    }
}

// New trait implementations (wrapping old trait implementations)
struct PassThroughNodeNew;
struct EchoNodeNew {
    inner: EchoNode,
}
struct CalculatorNodeNew {
    inner: CalculatorNode,
}
struct AddNodeNew {
    inner: AddNode,
}
struct MultiplyNodeNew {
    inner: MultiplyNode,
}

impl EchoNodeNew {
    fn new() -> Self {
        Self {
            inner: EchoNode::new(),
        }
    }
}
impl CalculatorNodeNew {
    fn new() -> Self {
        Self {
            inner: CalculatorNode::new(),
        }
    }
}
impl AddNodeNew {
    fn new() -> Self {
        Self {
            inner: AddNode::new(),
        }
    }
}
impl MultiplyNodeNew {
    fn new() -> Self {
        Self {
            inner: MultiplyNode::new(),
        }
    }
}

// Implement new trait by delegating to old implementations
#[async_trait]
impl crate::executor::node_executor::NodeExecutor for PassThroughNodeNew {
    async fn initialize(
        &mut self,
        ctx: &crate::executor::node_executor::NodeContext,
    ) -> Result<()> {
        let old_ctx = NodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: None,
            metadata: ctx.metadata.clone(),
        };
        PassThroughNode.initialize(&old_ctx).await
    }
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        PassThroughNode.process(input).await
    }
    async fn cleanup(&mut self) -> Result<()> {
        PassThroughNode.cleanup().await
    }
}

#[async_trait]
impl crate::executor::node_executor::NodeExecutor for EchoNodeNew {
    async fn initialize(
        &mut self,
        ctx: &crate::executor::node_executor::NodeContext,
    ) -> Result<()> {
        let old_ctx = NodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: None,
            metadata: ctx.metadata.clone(),
        };
        self.inner.initialize(&old_ctx).await
    }
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        self.inner.process(input).await
    }
    async fn cleanup(&mut self) -> Result<()> {
        self.inner.cleanup().await
    }
}

#[async_trait]
impl crate::executor::node_executor::NodeExecutor for CalculatorNodeNew {
    async fn initialize(
        &mut self,
        ctx: &crate::executor::node_executor::NodeContext,
    ) -> Result<()> {
        let old_ctx = NodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: None,
            metadata: ctx.metadata.clone(),
        };
        self.inner.initialize(&old_ctx).await
    }
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        self.inner.process(input).await
    }
    async fn cleanup(&mut self) -> Result<()> {
        self.inner.cleanup().await
    }
}

#[async_trait]
impl crate::executor::node_executor::NodeExecutor for AddNodeNew {
    async fn initialize(
        &mut self,
        ctx: &crate::executor::node_executor::NodeContext,
    ) -> Result<()> {
        let old_ctx = NodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: None,
            metadata: ctx.metadata.clone(),
        };
        self.inner.initialize(&old_ctx).await
    }
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        self.inner.process(input).await
    }
    async fn cleanup(&mut self) -> Result<()> {
        self.inner.cleanup().await
    }
}

#[async_trait]
impl crate::executor::node_executor::NodeExecutor for MultiplyNodeNew {
    async fn initialize(
        &mut self,
        ctx: &crate::executor::node_executor::NodeContext,
    ) -> Result<()> {
        let old_ctx = NodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: None,
            metadata: ctx.metadata.clone(),
        };
        self.inner.initialize(&old_ctx).await
    }
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        self.inner.process(input).await
    }
    async fn cleanup(&mut self) -> Result<()> {
        self.inner.cleanup().await
    }
}
