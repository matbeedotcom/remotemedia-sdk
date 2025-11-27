---
name: Generate Rust Node
description: Generate a Rust StreamingNode from a GitHub URL, crate, or local project
category: Nodes
tags: [node, generation, rust, native]
arguments:
  - name: source
    description: GitHub URL, crates.io crate name, or local filesystem path
    required: true
---

# Rust Node Generation

Generate a Rust `StreamingNode` for the RemoteMedia SDK runtime.

**Source**: $ARGUMENTS.source

## Analysis Steps

1. **Analyze the source**:
   - For GitHub URLs: Fetch README, `Cargo.toml`, main source files
   - For crate names: Fetch from crates.io/docs.rs
   - For local paths: Read project structure

2. **Determine node characteristics**:
   - Sync vs Async processing
   - Single output vs streaming multi-output
   - Session state requirements
   - Input/Output RuntimeData variants

3. **Generate the node** following the appropriate pattern:

### Synchronous Node (Simple Processing)

```rust
use crate::data::RuntimeData;
use crate::error::Error;
use crate::nodes::SyncStreamingNode;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct {NodeName}Config {
    // Configuration fields
}

pub struct {NodeName} {
    id: String,
    config: {NodeName}Config,
}

impl {NodeName} {
    pub fn new(id: String, config: {NodeName}Config) -> Self {
        Self { id, config }
    }
}

impl SyncStreamingNode for {NodeName} {
    fn node_type(&self) -> &str {
        "{NodeName}"
    }

    fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        match &data {
            RuntimeData::{InputVariant}(input) => {
                // Process and return
                Ok(RuntimeData::{OutputVariant}(output))
            }
            _ => Err(Error::InvalidInput {
                message: "Expected {InputType}".into(),
                node_id: self.id.clone(),
                context: format!("Got {:?}", data.data_type()),
            }),
        }
    }
}
```

### Async Node (Model Loading / IO)

```rust
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct {NodeName}Config {
    // Configuration fields
}

impl Default for {NodeName}Config {
    fn default() -> Self {
        Self {
            // defaults
        }
    }
}

pub struct {NodeName} {
    config: {NodeName}Config,
    // Lazy-loaded model
    model: OnceCell<Arc<Mutex<ModelType>>>,
}

impl {NodeName} {
    pub fn new(config: {NodeName}Config) -> Self {
        Self {
            config,
            model: OnceCell::new(),
        }
    }
}

#[async_trait]
impl AsyncStreamingNode for {NodeName} {
    fn node_type(&self) -> &str {
        "{NodeName}"
    }

    async fn initialize(&self) -> Result<(), Error> {
        // Load model here
        self.model.get_or_init(|| async {
            // Initialize model
            Arc::new(Mutex::new(model))
        }).await;
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let model = self.model.get()
            .ok_or_else(|| Error::Execution("Model not initialized".into()))?;

        match &data {
            RuntimeData::{InputVariant}(input) => {
                // Process with model
                Ok(RuntimeData::{OutputVariant}(output))
            }
            _ => Err(Error::InvalidInput {
                message: "Expected {InputType}".into(),
                node_id: String::new(),
                context: format!("Got {:?}", data.data_type()),
            }),
        }
    }
}
```

### Streaming Multi-Output Node

```rust
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

// Per-session state
#[derive(Debug, Clone, Default)]
struct SessionState {
    buffer: Vec<f32>,
    // Other state fields
}

pub struct {NodeName} {
    config: {NodeName}Config,
    states: Arc<Mutex<HashMap<String, SessionState>>>,
}

#[async_trait]
impl AsyncStreamingNode for {NodeName} {
    fn node_type(&self) -> &str {
        "{NodeName}"
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let session = session_id.unwrap_or_else(|| "default".to_string());
        let mut count = 0;

        // Get or create session state
        let mut states = self.states.lock().await;
        let state = states.entry(session).or_insert_with(SessionState::default);

        // Process and emit multiple outputs
        for chunk in chunks {
            callback(RuntimeData::{OutputVariant}(chunk))?;
            count += 1;
        }

        Ok(count)
    }
}
```

## Factory and Registration

```rust
pub struct {NodeName}Factory;

impl crate::nodes::StreamingNodeFactory for {NodeName}Factory {
    fn create(
        &self,
        node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn crate::nodes::StreamingNode>, Error> {
        let config = if params.is_null() {
            {NodeName}Config::default()
        } else {
            serde_json::from_value(params.clone())
                .map_err(|e| Error::Execution(format!("Config parse error: {}", e)))?
        };

        // For SyncStreamingNode:
        // Ok(Box::new(crate::nodes::SyncNodeWrapper({NodeName}::new(node_id, config))))

        // For AsyncStreamingNode:
        Ok(Box::new(crate::nodes::AsyncNodeWrapper(Arc::new({NodeName}::new(config)))))
    }

    fn node_type(&self) -> &str {
        "{NodeName}"
    }

    // Set to true for multi-output streaming nodes
    fn is_multi_output_streaming(&self) -> bool {
        false
    }
}
```

## Output Requirements

Provide:

1. **Complete node source** at `runtime-core/src/nodes/{snake_case_name}.rs`

2. **Module declaration** in `runtime-core/src/nodes/mod.rs`:
   ```rust
   mod {snake_case_name};
   pub use {snake_case_name}::{NodeName, {NodeName}Config, {NodeName}Factory};
   ```

3. **Registry update** in `runtime-core/src/nodes/streaming_registry.rs`:
   ```rust
   registry.register(Arc::new({NodeName}Factory));
   ```

4. **Cargo.toml dependencies** if needed

5. **Example manifest**:
   ```yaml
   nodes:
     - id: {node_id}
       node_type: {NodeName}
       executor: native
       params:
         # parameters
   ```

## RuntimeData Variants

```rust
pub enum RuntimeData {
    Audio(AudioData),      // Audio samples
    Text(String),          // Text content
    Json(serde_json::Value), // JSON data
    Image(ImageData),      // Image pixels
    Tensor(TensorData),    // ML tensors
    Empty,                 // No data
}

pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub format: AudioFormat,
}
```
