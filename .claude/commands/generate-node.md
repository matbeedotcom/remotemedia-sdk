---
name: Generate Node
description: Generate a Rust or Python node from a GitHub, HuggingFace, or local project
category: Nodes
tags: [node, generation, rust, python, model]
arguments:
  - name: source
    description: GitHub URL, HuggingFace URL, or local filesystem path to the model/project
    required: true
  - name: language
    description: Target language (rust or python). Auto-detected if not specified.
    required: false
---

# Node Generation Skill

You are generating a RemoteMedia SDK node from an external model or project.

**Source**: $ARGUMENTS.source
**Language**: $ARGUMENTS.language (if not specified, you will determine the best choice)

## Instructions

### Step 1: Analyze the Source

First, analyze the provided source to understand what it does:

1. **For GitHub URLs**:
   - Fetch the repository README and key source files
   - Identify the main entry points, dependencies, and usage patterns
   - Look for `requirements.txt`, `pyproject.toml`, `Cargo.toml`, or similar

2. **For HuggingFace URLs**:
   - Fetch the model card and configuration
   - Identify the model type (text generation, ASR, TTS, image, etc.)
   - Determine the appropriate pipeline or inference method

3. **For Local Paths**:
   - Read the project structure and key files
   - Identify dependencies and usage patterns

### Step 2: Determine Node Type

Based on the source analysis, determine:

1. **Language Choice** (if not specified):
   - Use **Rust** if:
     - The project is already Rust-based
     - Performance-critical audio/video processing
     - Simple transformations that don't need Python ML libraries
   - Use **Python** if:
     - ML model inference (PyTorch, TensorFlow, etc.)
     - Uses Python-specific libraries (transformers, whisperx, kokoro, etc.)
     - Complex async/streaming processing

2. **Node Characteristics**:
   - **Streaming vs Non-streaming**: Does it process data in chunks or all at once?
   - **Multi-output**: Can it yield multiple outputs from a single input?
   - **State management**: Does it need session-specific state?
   - **Input/Output types**: Audio, Text, Image, JSON, etc.

### Step 3: Generate the Node

#### For Python Nodes (MultiprocessNode)

Generate a node that follows this pattern:

```python
import logging
import numpy as np
from typing import AsyncGenerator, Optional, Union, Dict, Any
import asyncio

from remotemedia.core.multiprocessing.data import RuntimeData
from remotemedia.core import MultiprocessNode, NodeConfig

logger = logging.getLogger(__name__)

class {NodeName}(MultiprocessNode):
    """
    {Description of what the node does}.

    Inherits from MultiprocessNode for multiprocess execution support.

    Input: RuntimeData.{InputType}
    Output: RuntimeData.{OutputType} (or AsyncGenerator for streaming)
    """

    def __init__(
        self,
        node_id: str = None,
        # Add model-specific parameters here
        config: Union[NodeConfig, Dict[str, Any]] = None,
        **kwargs
    ):
        # Initialize MultiprocessNode base
        if config is not None:
            super().__init__(config, **kwargs)
            if isinstance(config, NodeConfig):
                params = config.params
            else:
                params = config.get('params', {})
            # Extract params from config
        else:
            from remotemedia.core.multiprocessing.node import NodeConfig
            minimal_config = NodeConfig(
                node_id=node_id or "{node_name}",
                node_type="{NodeName}",
                params={}
            )
            super().__init__(minimal_config, **kwargs)

        # Store configuration
        self.model = None
        self.is_streaming = {True/False}

    async def initialize(self) -> None:
        """Load the model/initialize resources."""
        logger.info(f"Initializing {NodeName}")
        try:
            # Import and load model here
            # Example:
            # from some_library import SomeModel
            # self.model = SomeModel(...)
            logger.info("{NodeName} initialized successfully")
        except ImportError as e:
            raise ImportError(f"Install required package: {e}")

    async def process(self, data: RuntimeData) -> {ReturnType}:
        """Process input data and return/yield output."""
        # Validate input type
        if not data.is_{input_type}():
            logger.warning(f"Expected {input_type} input, got {data.data_type()}")
            return

        # Extract data
        input_data = data.as_{input_type}()

        try:
            # Process the data
            # For streaming nodes, use async generator:
            # async for chunk in self._process_streaming(input_data):
            #     yield RuntimeData.{output_type}(chunk, ...)

            # For non-streaming:
            # result = await self._process(input_data)
            # return RuntimeData.{output_type}(result, ...)
            pass
        except Exception as e:
            logger.error(f"Processing error: {e}", exc_info=True)

    async def cleanup(self) -> None:
        """Clean up resources."""
        logger.info(f"Cleaning up {NodeName}")
        self.model = None
```

**Key patterns for Python nodes:**
- Use `RuntimeData` for input/output (`.is_audio()`, `.as_numpy()`, `.audio()`, `.text()`, etc.)
- Use `async def process()` with `AsyncGenerator` for streaming
- Load models in `initialize()`, not `__init__()`
- Clean up in `cleanup()`
- Use `self.is_streaming = True` for streaming nodes

#### For Rust Nodes (StreamingNode)

Generate a node that follows this pattern:

```rust
use crate::data::RuntimeData;
use crate::error::{Error, Result};
use crate::nodes::AsyncStreamingNode;  // or SyncStreamingNode
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::Deserialize;

/// Configuration for {NodeName}
#[derive(Debug, Clone, Deserialize)]
pub struct {NodeName}Config {
    // Add configuration fields
    #[serde(default = "default_value")]
    pub some_param: Type,
}

impl Default for {NodeName}Config {
    fn default() -> Self {
        Self {
            some_param: default_value(),
        }
    }
}

/// {Description of what the node does}
pub struct {NodeName} {
    config: {NodeName}Config,
    // Add state fields if needed
    // For session-scoped state:
    // states: Arc<Mutex<std::collections::HashMap<String, NodeState>>>,
}

impl {NodeName} {
    pub fn new(config: {NodeName}Config) -> Self {
        Self {
            config,
        }
    }
}

#[async_trait]
impl AsyncStreamingNode for {NodeName} {
    fn node_type(&self) -> &str {
        "{NodeName}"
    }

    async fn initialize(&self) -> Result<(), Error> {
        // Load model/resources here if needed
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // Extract and validate input
        match &data {
            RuntimeData::{InputVariant}(input) => {
                // Process the data

                // Return output
                Ok(RuntimeData::{OutputVariant}(output))
            }
            _ => Err(Error::InvalidInput {
                message: "Expected {InputType} input".into(),
                node_id: String::new(),
                context: format!("Received {:?}", data.data_type()),
            }),
        }
    }

    // For multi-output streaming nodes, implement:
    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let mut count = 0;
        // Process and yield multiple outputs via callback
        // callback(RuntimeData::{OutputVariant}(chunk))?;
        // count += 1;
        Ok(count)
    }
}

// Factory for registry
pub struct {NodeName}Factory;

impl crate::nodes::StreamingNodeFactory for {NodeName}Factory {
    fn create(
        &self,
        _node_id: String,
        params: &serde_json::Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn crate::nodes::StreamingNode>, Error> {
        let config = if params.is_null() {
            {NodeName}Config::default()
        } else {
            serde_json::from_value(params.clone())
                .map_err(|e| Error::Execution(format!("Failed to parse config: {}", e)))?
        };
        let node = {NodeName}::new(config);
        Ok(Box::new(crate::nodes::AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "{NodeName}"
    }
}
```

**Key patterns for Rust nodes:**
- Use `SyncStreamingNode` for simple synchronous processing
- Use `AsyncStreamingNode` for async processing with model loading
- Implement `process_streaming()` for multi-output nodes
- Use factory pattern for registry integration
- Handle `RuntimeData` variants: `Audio`, `Text`, `Json`, `Image`, `Tensor`, etc.

### Step 4: Generate Registration Code

For Python nodes, add to the appropriate `__init__.py`:
```python
from .{module_name} import {NodeName}
```

For Rust nodes, add to `runtime-core/src/nodes/streaming_registry.rs`:
```rust
registry.register(Arc::new({NodeName}Factory));
```

### Step 5: Generate Manifest Example

Provide an example manifest YAML for using the node:

```yaml
nodes:
  - id: {node_id}
    node_type: {NodeName}
    executor: {multiprocess|native}  # multiprocess for Python, native for Rust
    params:
      # Node-specific parameters
```

## Output Format

Generate the complete node implementation with:

1. **File path** where the node should be created
2. **Complete source code** for the node
3. **Dependencies** that need to be installed
4. **Registration code** for integrating into the pipeline
5. **Example manifest** for using the node
6. **Example usage** showing how to use the node in a pipeline

## RuntimeData Reference

Available `RuntimeData` types:

**Python:**
- `RuntimeData.audio(samples: np.ndarray, sample_rate: int, channels: int = 1)`
- `RuntimeData.text(content: str)`
- `RuntimeData.json(data: dict)`
- `RuntimeData.image(data: np.ndarray, width: int, height: int, format: str = "RGB")`
- Methods: `.is_audio()`, `.is_text()`, `.as_numpy()`, `.as_text()`, `.data_type()`

**Rust:**
- `RuntimeData::Audio(AudioData { samples, sample_rate, channels, format })`
- `RuntimeData::Text(String)`
- `RuntimeData::Json(serde_json::Value)`
- `RuntimeData::Image(ImageData { ... })`
- `RuntimeData::Tensor(TensorData { ... })`
