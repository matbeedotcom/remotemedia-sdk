//! Python node registry for dynamic registration
//!
//! This module provides a global registry where Python nodes can register themselves.
//! The registry stores metadata about each node type, which is then used to create
//! factories dynamically.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// Configuration for a Python node
#[derive(Debug, Clone)]
pub struct PythonNodeConfig {
    /// The node type name used in manifests (e.g., "KokoroTTSNode")
    pub node_type: String,

    /// The Python class name to instantiate (e.g., "remotemedia.nodes.tts.KokoroTTSNode")
    /// If not specified, defaults to node_type
    pub python_class: String,

    /// Whether this node can produce multiple outputs per input
    pub is_multi_output: bool,

    /// Whether this is a Python node (always true for this registry)
    pub is_python_node: bool,

    /// Description of the node (for schema/documentation)
    pub description: Option<String>,

    /// Category for the node (e.g., "ml", "audio", "text")
    pub category: Option<String>,

    /// Input data types this node accepts (e.g., ["audio", "text"])
    pub accepts: Vec<String>,

    /// Output data types this node produces (e.g., ["audio", "json"])
    pub produces: Vec<String>,
}

impl Default for PythonNodeConfig {
    fn default() -> Self {
        Self {
            node_type: String::new(),
            python_class: String::new(),
            is_multi_output: false,
            is_python_node: true,
            description: None,
            category: None,
            accepts: Vec::new(),
            produces: Vec::new(),
        }
    }
}

impl PythonNodeConfig {
    /// Create a new config with just the node type (python_class defaults to node_type)
    pub fn new(node_type: impl Into<String>) -> Self {
        let node_type = node_type.into();
        Self {
            python_class: node_type.clone(),
            node_type,
            ..Default::default()
        }
    }

    /// Set the Python class name
    pub fn with_python_class(mut self, python_class: impl Into<String>) -> Self {
        self.python_class = python_class.into();
        self
    }

    /// Mark as multi-output streaming
    pub fn with_multi_output(mut self, is_multi_output: bool) -> Self {
        self.is_multi_output = is_multi_output;
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set category
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set accepted input types
    pub fn accepts(mut self, types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.accepts = types.into_iter().map(|t| t.into()).collect();
        self
    }

    /// Set produced output types
    pub fn produces(mut self, types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.produces = types.into_iter().map(|t| t.into()).collect();
        self
    }
}

/// Global registry of Python nodes
pub struct PythonNodeRegistry {
    nodes: RwLock<HashMap<String, PythonNodeConfig>>,
}

impl PythonNodeRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
        }
    }

    /// Register a Python node configuration
    pub fn register(&self, config: PythonNodeConfig) {
        let node_type = config.node_type.clone();
        let mut nodes = self.nodes.write().unwrap();
        tracing::debug!(
            node_type = %node_type,
            python_class = %config.python_class,
            multi_output = config.is_multi_output,
            "Registering Python node"
        );
        nodes.insert(node_type, config);
    }

    /// Get a registered node configuration
    pub fn get(&self, node_type: &str) -> Option<PythonNodeConfig> {
        let nodes = self.nodes.read().unwrap();
        nodes.get(node_type).cloned()
    }

    /// Get all registered node configurations
    pub fn get_all(&self) -> Vec<PythonNodeConfig> {
        let nodes = self.nodes.read().unwrap();
        nodes.values().cloned().collect()
    }

    /// Get the number of registered nodes
    pub fn len(&self) -> usize {
        let nodes = self.nodes.read().unwrap();
        nodes.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all registrations
    pub fn clear(&self) {
        let mut nodes = self.nodes.write().unwrap();
        nodes.clear();
    }

    /// Check if a node type is registered
    pub fn contains(&self, node_type: &str) -> bool {
        let nodes = self.nodes.read().unwrap();
        nodes.contains_key(node_type)
    }
}

impl Default for PythonNodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global Python node registry instance
pub static PYTHON_NODE_REGISTRY: LazyLock<PythonNodeRegistry> =
    LazyLock::new(PythonNodeRegistry::new);

/// Register a Python node with the global registry
///
/// # Example
///
/// ```ignore
/// use remotemedia_python_nodes::{register_python_node, PythonNodeConfig};
///
/// // Simple registration
/// register_python_node(PythonNodeConfig::new("MyNode"));
///
/// // With options
/// register_python_node(
///     PythonNodeConfig::new("KokoroTTSNode")
///         .with_python_class("remotemedia.nodes.tts.KokoroTTSNode")
///         .with_multi_output(true)
///         .with_category("tts")
///         .accepts(["text"])
///         .produces(["audio"])
/// );
/// ```
pub fn register_python_node(config: PythonNodeConfig) {
    PYTHON_NODE_REGISTRY.register(config);
}

/// Get all registered Python nodes
pub fn get_registered_nodes() -> Vec<PythonNodeConfig> {
    PYTHON_NODE_REGISTRY.get_all()
}

/// Clear all registered Python nodes (mainly for testing)
pub fn clear_registry() {
    PYTHON_NODE_REGISTRY.clear();
}

/// Register the default set of Python nodes
///
/// This registers the common Python nodes that ship with RemoteMedia.
/// Called automatically by the PythonNodesProvider.
pub fn register_default_python_nodes() {
    // Whisper transcription nodes
    register_python_node(
        PythonNodeConfig::new("WhisperXNode")
            .with_python_class("WhisperXTranscriber")
            .with_description("Speech-to-text with word-level timestamps using WhisperX")
            .with_category("ml")
            .accepts(["audio"])
            .produces(["json"]),
    );

    register_python_node(
        PythonNodeConfig::new("HFWhisperNode")
            .with_python_class("WhisperTranscriptionNode")
            .with_multi_output(true)
            .with_description("Speech-to-text with word-level timestamps using HuggingFace Whisper")
            .with_category("ml")
            .accepts(["audio"])
            .produces(["json"]),
    );

    // TTS nodes
    register_python_node(
        PythonNodeConfig::new("KokoroTTSNode")
            .with_multi_output(true)
            .with_description("Text-to-speech using Kokoro TTS")
            .with_category("tts")
            .accepts(["text"])
            .produces(["audio"]),
    );

    register_python_node(
        PythonNodeConfig::new("VibeVoiceTTSNode")
            .with_multi_output(true)
            .with_description("Text-to-speech using VibeVoice TTS")
            .with_category("tts")
            .accepts(["text"])
            .produces(["audio"]),
    );

    // ML nodes
    register_python_node(
        PythonNodeConfig::new("LFM2AudioNode")
            .with_multi_output(true)
            .with_description("Speech-to-speech generation using LFM2")
            .with_category("ml")
            .accepts(["audio"])
            .produces(["audio", "text"]),
    );

    register_python_node(
        PythonNodeConfig::new("SimplePyTorchNode")
            .with_description("Simple PyTorch inference node for testing")
            .with_category("ml"),
    );

    // Test nodes
    register_python_node(
        PythonNodeConfig::new("ExpanderNode")
            .with_multi_output(true)
            .with_category("test"),
    );

    register_python_node(
        PythonNodeConfig::new("RangeGeneratorNode")
            .with_multi_output(true)
            .with_category("test"),
    );

    register_python_node(
        PythonNodeConfig::new("TransformAndExpandNode")
            .with_multi_output(true)
            .with_category("test"),
    );

    register_python_node(
        PythonNodeConfig::new("ChainedTransformNode")
            .with_multi_output(true)
            .with_category("test"),
    );

    register_python_node(
        PythonNodeConfig::new("ConditionalExpanderNode")
            .with_multi_output(true)
            .with_category("test"),
    );

    register_python_node(
        PythonNodeConfig::new("FilterNode")
            .with_multi_output(true)
            .with_category("test"),
    );

    tracing::info!(
        count = PYTHON_NODE_REGISTRY.len(),
        "Registered default Python nodes"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let registry = PythonNodeRegistry::new();

        registry.register(
            PythonNodeConfig::new("TestNode")
                .with_python_class("test.TestNode")
                .with_multi_output(true),
        );

        let config = registry.get("TestNode").unwrap();
        assert_eq!(config.node_type, "TestNode");
        assert_eq!(config.python_class, "test.TestNode");
        assert!(config.is_multi_output);
        assert!(config.is_python_node);
    }

    #[test]
    fn test_config_builder() {
        let config = PythonNodeConfig::new("MyNode")
            .with_python_class("my.module.MyNode")
            .with_multi_output(true)
            .with_description("A test node")
            .with_category("test")
            .accepts(["audio", "text"])
            .produces(["json"]);

        assert_eq!(config.node_type, "MyNode");
        assert_eq!(config.python_class, "my.module.MyNode");
        assert!(config.is_multi_output);
        assert_eq!(config.description, Some("A test node".to_string()));
        assert_eq!(config.category, Some("test".to_string()));
        assert_eq!(config.accepts, vec!["audio", "text"]);
        assert_eq!(config.produces, vec!["json"]);
    }

    #[test]
    fn test_registry_len_and_clear() {
        let registry = PythonNodeRegistry::new();
        assert!(registry.is_empty());

        registry.register(PythonNodeConfig::new("Node1"));
        registry.register(PythonNodeConfig::new("Node2"));
        assert_eq!(registry.len(), 2);

        registry.clear();
        assert!(registry.is_empty());
    }
}
