//! TextInput node - passthrough source for text data
//!
//! This is an I/O boundary node used in pipeline manifests to declare
//! that the pipeline accepts text input. The CLI feeds text data from
//! --input into this node, which passes it through unchanged.

use serde::Deserialize;

use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::streaming_node::{SyncStreamingNode, SyncNodeWrapper, StreamingNodeFactory, StreamingNode};
use remotemedia_core::Error;

/// Configuration for TextInput node
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TextInputConfig {
    /// Text encoding (informational only, data is always UTF-8 in RuntimeData)
    #[serde(default = "default_encoding")]
    pub encoding: String,
}

fn default_encoding() -> String {
    "utf-8".to_string()
}

/// TextInput streaming node - passes text through from CLI input
struct TextInputStreamingNode;

impl SyncStreamingNode for TextInputStreamingNode {
    fn node_type(&self) -> &str {
        "TextInput"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // Pass through - the CLI already converted input to RuntimeData::Text
        Ok(data)
    }
}

/// Factory for creating TextInput nodes
pub struct TextInputNodeFactory;

impl StreamingNodeFactory for TextInputNodeFactory {
    fn create(
        &self,
        _node_id: String,
        _params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        Ok(Box::new(SyncNodeWrapper(TextInputStreamingNode)))
    }

    fn node_type(&self) -> &str {
        "TextInput"
    }
}
