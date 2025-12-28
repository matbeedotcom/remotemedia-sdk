//! CLI-specific pipeline nodes
//!
//! This module contains nodes that are specific to the CLI application,
//! such as output formatters for different file formats.

pub mod srt_output;

pub use srt_output::SrtOutputNode;

use remotemedia_runtime_core::nodes::registry::{NodeFactory, NodeRegistry};
use remotemedia_runtime_core::Result;
use serde_json::Value;
use std::sync::Arc;

/// Register CLI-specific nodes with a registry
pub fn register_cli_nodes(registry: &mut NodeRegistry) {
    registry.register_rust(Arc::new(SrtOutputNodeFactory));
}

/// Factory for creating SrtOutputNode instances
struct SrtOutputNodeFactory;

impl NodeFactory for SrtOutputNodeFactory {
    fn create(
        &self,
        params: Value,
    ) -> Result<Box<dyn remotemedia_runtime_core::executor::node_executor::NodeExecutor>> {
        Ok(Box::new(SrtOutputNode::from_params(params)))
    }

    fn node_type(&self) -> &str {
        "SrtOutput"
    }

    fn is_rust_native(&self) -> bool {
        true
    }
}
