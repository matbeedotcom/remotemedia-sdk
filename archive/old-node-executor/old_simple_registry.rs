//! ARCHIVED: Simple NodeRegistry and NodeExecutor trait (v0.2.0-v0.2.1)
//!
//! This file contains the original simple registry system that used a HashMap
//! of factory functions to create nodes. It was replaced by the multi-tier
//! CompositeRegistry system that supports Rust/Python runtime selection.
//!
//! **Archived on**: 2025-10-28
//! **Reason**: Trait consolidation - all nodes now use executor::node_executor::NodeExecutor
//! **Replaced by**: 
//!   - Trait: executor::node_executor::NodeExecutor
//!   - Registry: nodes::registry::NodeRegistry + CompositeRegistry
//!
//! # What Was Here
//!
//! ## Old NodeExecutor Trait
//! Simple async trait for node execution with initialize/process/cleanup lifecycle:
//!
//! ```rust,ignore
//! #[async_trait]
//! pub trait NodeExecutor: Send + Sync {
//!     async fn initialize(&mut self, context: &NodeContext) -> Result<()>;
//!     async fn process(&mut self, input: Value) -> Result<Vec<Value>>;
//!     async fn cleanup(&mut self) -> Result<()>;
//!     fn is_streaming(&self) -> bool { false }
//!     async fn finish_streaming(&mut self) -> Result<Vec<Value>> { Ok(vec![]) }
//!     fn info(&self) -> NodeInfo { ... }
//! }
//! ```
//!
//! ## Old NodeRegistry
//! Simple HashMap-based registry for built-in nodes:
//!
//! ```rust,ignore
//! pub struct NodeRegistry {
//!     factories: HashMap<String, NodeFactory>,
//! }
//! 
//! pub type NodeFactory = Box<dyn Fn() -> Box<dyn NodeExecutor> + Send + Sync>;
//! ```
//!
//! ## Built-in Nodes Registered
//! - PassThrough / PassThroughNode
//! - Echo
//! - CalculatorNode  
//! - MultiplyNode
//! - AddNode
//! - RustWhisperTranscriber (if whisper feature enabled)
//!
//! # Why Archived
//!
//! **Trait Consolidation (2025-10-28)**:
//! - All nodes migrated to `executor::node_executor::NodeExecutor`
//! - Old trait had duplicate definition (nearly identical to new trait)
//! - Only difference: parameter name (`context` vs `ctx`) and `info()` method
//! - Consolidation simplifies codebase and removes confusion
//!
//! **Registry Evolution**:
//! - Old: Single HashMap registry
//! - New: Multi-tier CompositeRegistry with Rust/Python selection
//! - New system supports user/audio/system registry layers
//! - New system supports RuntimeHint for implementation selection
//!
//! # Migration Performed
//!
//! All built-in nodes migrated in one commit:
//!
//! ```rust
//! // Before
//! impl nodes::NodeExecutor for PassThroughNode {
//!     async fn initialize(&mut self, context: &NodeContext) -> Result<()> { ... }
//! }
//!
//! // After
//! impl executor::node_executor::NodeExecutor for PassThroughNode {
//!     async fn initialize(&mut self, ctx: &NodeContext) -> Result<()> { ... }
//! }
//! ```
//!
//! Changes per node:
//! 1. Change trait path: `NodeExecutor` → `executor::node_executor::NodeExecutor`
//! 2. Change parameter name: `context` → `ctx`
//! 3. Remove `info()` method (not in new trait)
//! 4. Change context type: `NodeContext` → `executor::node_executor::NodeContext`
//!
//! # Current Architecture (Post-Migration)
//!
//! **Single Trait**:
//! - `executor::node_executor::NodeExecutor` - Used by ALL nodes
//!
//! **Multi-Tier Registry**:
//! - `nodes::registry::NodeRegistry` - Phase 5 registry with Rust/Python factories
//! - `CompositeRegistry` - Chains multiple registries (user > audio > system)
//!
//! **No More**:
//! - ❌ `nodes::NodeExecutor` trait (archived here)
//! - ❌ Simple HashMap registry (archived here)
//! - ❌ `NodeInfo` struct (removed, not needed)
//!
//! # Files That Used Old System
//!
//! **Before Migration**:
//! - `runtime/src/nodes/mod.rs` - Trait definition + registry + 6 built-in nodes
//! - `runtime/src/executor/mod.rs` - Stored builtin_nodes: NodeRegistry
//! - `runtime/src/grpc_service/version.rs` - Used NodeRegistry::default() for node types
//!
//! **After Migration**:
//! - All nodes use `executor::node_executor::NodeExecutor`
//! - Executor uses `CompositeRegistry` only
//! - Version manager accepts node type list directly
//!
//! # Restoration (Not Recommended)
//!
//! To restore old trait system (not recommended):
//!
//! 1. Copy trait definition back to `nodes/mod.rs`
//! 2. Create adapter to bridge traits (see `cpython_node.rs` in this directory)
//! 3. Update all node implementations to use old trait
//! 4. Revert executor to store both registries
//!
//! **Why not restore**: 
//! - Adds complexity (dual trait system)
//! - Requires adapter layer
//! - Confuses which trait to use
//! - Migration already complete and tested

use std::collections::HashMap;
use serde_json::Value;

// ============================================================================
// ARCHIVED: Old Trait Definition
// ============================================================================

/// ARCHIVED: Node execution context (replaced by executor::node_executor::NodeContext)
#[derive(Debug, Clone)]
pub struct NodeContext {
    pub node_id: String,
    pub node_type: String,
    pub params: Value,
    pub session_id: Option<String>,
    pub metadata: HashMap<String, Value>,
}

/// ARCHIVED: Node information (removed, not in new trait)
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

/// ARCHIVED: Old NodeExecutor trait
/// Replaced by executor::node_executor::NodeExecutor
pub trait NodeExecutor: Send + Sync {
    fn initialize(&mut self, context: &NodeContext);
    fn process(&mut self, input: Value) -> Vec<Value>;
    fn cleanup(&mut self);
    fn is_streaming(&self) -> bool { false }
    fn finish_streaming(&mut self) -> Vec<Value> { vec![] }
    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: "UnknownNode".to_string(),
            version: "0.1.0".to_string(),
            description: None,
        }
    }
}

// ============================================================================
// ARCHIVED: Old Simple Registry
// ============================================================================

/// ARCHIVED: Node factory for old registry
pub type NodeFactory = Box<dyn Fn() -> Box<dyn NodeExecutor> + Send + Sync>;

/// ARCHIVED: Simple HashMap-based registry
pub struct NodeRegistry {
    factories: HashMap<String, NodeFactory>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, node_type: &str, factory: F)
    where
        F: Fn() -> Box<dyn NodeExecutor> + Send + Sync + 'static,
    {
        self.factories.insert(node_type.to_string(), Box::new(factory));
    }

    pub fn create(&self, node_type: &str) -> Option<Box<dyn NodeExecutor>> {
        self.factories.get(node_type).map(|factory| factory())
    }

    pub fn has_node_type(&self, node_type: &str) -> bool {
        self.factories.contains_key(node_type)
    }

    pub fn node_types(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

impl Default for NodeRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        
        // Built-in nodes were registered here in Default::default()
        // Now handled by nodes::registry factories
        
        registry
    }
}
