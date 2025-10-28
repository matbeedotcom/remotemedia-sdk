//! ARCHIVED: Old NodeRegistry implementation (Phase 5 dual-registry cleanup)
//!
//! This file contains the original NodeRegistry that used simple factory functions
//! and the nodes::NodeExecutor trait. It was replaced by nodes::registry::NodeRegistry
//! which supports runtime hint resolution (Rust vs Python implementations).
//!
//! **Archived on**: 2025-10-28
//! **Reason**: Dual-registry architecture cleanup
//! **Replaced by**: runtime/src/nodes/registry.rs (NodeRegistry with RuntimeHint support)
//!
//! # Architecture Evolution
//!
//! ## Old System (this file)
//! - Simple HashMap<String, NodeFactory>
//! - NodeFactory = Box<dyn Fn() -> Box<dyn NodeExecutor>>
//! - Used by: version.rs for listing node types
//! - Trait: nodes::NodeExecutor (simple trait with initialize/process/cleanup)
//!
//! ## New System (runtime/src/nodes/registry.rs)
//! - Separate rust_factories and python_factories
//! - RuntimeHint for Rust/Python selection
//! - NodeFactory trait with is_rust_native() method
//! - Trait: executor::node_executor::NodeExecutor (async trait with context)
//!
//! # Built-in Nodes Registered Here
//!
//! The Default implementation registered:
//! - PassThrough / PassThroughNode
//! - Echo
//! - CalculatorNode
//! - MultiplyNode
//! - AddNode
//! - RustWhisperTranscriber (if whisper feature enabled)
//!
//! These nodes are now registered in the new registry system.
//!
//! # Usage History
//!
//! **Files that used this registry**:
//! 1. runtime/src/grpc_service/version.rs
//!    - VersionManager::from_registry(&NodeRegistry::default())
//!    - Used node_types() to populate supported_node_types
//!    - Now uses new registry's list_node_types()
//!
//! 2. runtime/src/executor/mod.rs
//!    - Executor::with_config() created NodeRegistry::default()
//!    - Stored as self.registry field
//!    - Was NEVER ACTUALLY USED for node creation!
//!    - Node creation used executor::node_executor path, not this registry
//!
//! 3. runtime/src/nodes/mod.rs (tests)
//!    - Unit tests created NodeRegistry::default()
//!    - Tested factory registration and node creation
//!
//! # Migration Path
//!
//! To fully remove this old registry:
//! 1. ✅ Update version.rs to use new registry
//! 2. ✅ Remove NodeRegistry field from Executor
//! 3. ✅ Archive this file
//! 4. ✅ Remove old NodeRegistry from nodes/mod.rs
//! 5. ✅ Update all imports

use std::collections::HashMap;
use serde_json::Value;

/// ARCHIVED: Node factory for creating node instances
pub type NodeFactory = Box<dyn Fn() -> Box<dyn NodeExecutor> + Send + Sync>;

/// ARCHIVED: Simple trait for node execution (replaced by executor::node_executor::NodeExecutor)
pub trait NodeExecutor: Send + Sync {
    fn initialize(&mut self, context: &NodeContext) -> Result<(), String>;
    fn process(&mut self, input: Value) -> Result<Vec<Value>, String>;
    fn cleanup(&mut self) -> Result<(), String>;
    fn is_streaming(&self) -> bool { false }
    fn finish_streaming(&mut self) -> Result<Vec<Value>, String> { Ok(vec![]) }
    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "UnknownNode".to_string(),
            version: "0.1.0".to_string(),
            description: None,
        }
    }
}

/// ARCHIVED: Node context
#[derive(Debug, Clone)]
pub struct NodeContext {
    pub node_id: String,
    pub node_type: String,
    pub params: Value,
    pub session_id: Option<String>,
    pub metadata: HashMap<String, Value>,
}

/// ARCHIVED: Node information
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

/// ARCHIVED: Registry for node types
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
    pub fn create(&self, node_type: &str) -> Result<Box<dyn NodeExecutor>, String> {
        self.factories
            .get(node_type)
            .map(|factory| factory())
            .ok_or_else(|| format!("Unknown node type: {}", node_type))
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
        
        // Built-in nodes were registered here:
        // - PassThrough / PassThroughNode
        // - Echo
        // - CalculatorNode
        // - MultiplyNode
        // - AddNode
        // - RustWhisperTranscriber (conditional)
        
        // Now handled by nodes::registry::NodeRegistry
        registry
    }
}
