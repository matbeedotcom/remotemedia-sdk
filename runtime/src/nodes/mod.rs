//! Node type implementations
//!
//! This module contains the core node execution logic and lifecycle management.

use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

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
    async fn process(&mut self, input: Value) -> Result<Option<Value>>;

    /// Cleanup resources
    ///
    /// Called once when the node is done processing.
    /// Use this to:
    /// - Release resources
    /// - Save state
    /// - Close connections
    async fn cleanup(&mut self) -> Result<()>;

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
        self.factories.insert(node_type.to_string(), Box::new(factory));
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
        let mut registry = Self::new();

        // Register built-in nodes
        registry.register("PassThrough", || Box::new(PassThroughNode));
        registry.register("PassThroughNode", || Box::new(PassThroughNode)); // Python compatibility
        registry.register("Echo", || Box::new(EchoNode::new()));
        registry.register("CalculatorNode", || Box::new(CalculatorNode::new()));

        registry
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

    async fn process(&mut self, input: Value) -> Result<Option<Value>> {
        Ok(Some(input))
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "PassThrough".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Passes input directly to output without modification".to_string()),
        }
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
        tracing::debug!("EchoNode initialized");
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Option<Value>> {
        self.counter += 1;
        Ok(Some(serde_json::json!({
            "input": input,
            "counter": self.counter,
            "node": "Echo"
        })))
    }

    async fn cleanup(&mut self) -> Result<()> {
        tracing::debug!("EchoNode processed {} items", self.counter);
        Ok(())
    }

    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "Echo".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Echoes input with metadata".to_string()),
        }
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

        tracing::debug!(
            "CalculatorNode initialized: operation={}, operand={}",
            self.operation,
            self.operand
        );
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Option<Value>> {
        // Convert input to number
        let num = match input {
            Value::Number(n) => n.as_f64().unwrap_or(0.0),
            Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
            _ => return Ok(Some(input)), // Pass through non-numeric values
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

        Ok(Some(output))
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "CalculatorNode".to_string(),
            version: "0.1.0".to_string(),
            description: Some(format!(
                "Performs {} operation with operand {}",
                self.operation, self.operand
            )),
        }
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

        assert_eq!(output, Some(input));

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
        let output = node.process(input.clone()).await.unwrap().unwrap();

        assert_eq!(output["input"], input);
        assert_eq!(output["counter"], 1);
        assert_eq!(output["node"], "Echo");

        // Process another item
        let output2 = node.process(serde_json::json!("world")).await.unwrap().unwrap();
        assert_eq!(output2["counter"], 2);

        node.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_node_registry() {
        let registry = NodeRegistry::default();

        assert!(registry.has_node_type("PassThrough"));
        assert!(registry.has_node_type("Echo"));
        assert!(!registry.has_node_type("NonExistent"));

        let node = registry.create("PassThrough").unwrap();
        let info = node.info();
        assert_eq!(info.name, "PassThrough");
    }

    #[tokio::test]
    async fn test_registry_create_unknown() {
        let registry = NodeRegistry::default();
        let result = registry.create("UnknownNode");
        assert!(result.is_err());
    }
}
