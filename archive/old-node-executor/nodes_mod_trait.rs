//! Original NodeExecutor trait from runtime/src/nodes/mod.rs
//! 
//! This was the original trait definition used in v0.2.0 before consolidation.
//! Archived on 2025-10-27 as part of v0.2.1 trait consolidation.
//!
//! Lines extracted: ~18-105 from runtime/src/nodes/mod.rs

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
