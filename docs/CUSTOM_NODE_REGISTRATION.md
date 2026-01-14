# Custom Node Registration Guide

This guide explains how to register custom nodes (both Rust and Python) in the RemoteMedia SDK.

## Overview

The RemoteMedia SDK provides multiple ways to register custom nodes:

1. **Simple Python nodes** - Register via command-line or helper functions
2. **Custom Rust nodes** - Implement `StreamingNodeFactory` trait
3. **Mixed registries** - Combine default + custom nodes

## Quick Start

### Method 1: Command-Line Registration (Python Nodes)

The easiest way to register Python nodes is via command-line flags:

```bash
# WebRTC server with custom nodes
cargo run --bin webrtc_server --features grpc-signaling -- \
  --mode grpc \
  --custom-nodes "WhisperASR,GPT4TTS,CustomFilter"

# Or via environment variable
export WEBRTC_CUSTOM_NODES="WhisperASR,GPT4TTS"
cargo run --bin webrtc_server --features grpc-signaling
```

### Method 2: Programmatic Registration

For more control, create a custom registry in your code:

```rust
use remotemedia_runtime_core::transport::PipelineRunner;
use remotemedia_webrtc::custom_nodes::create_custom_registry;

// Create runner with custom Python nodes
let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry(&[
        ("WhisperASR", false),      // Single output per input
        ("GPT4TTS", true),          // Multi-output (streaming)
        ("CustomFilter", false),
    ])
})?;
```

## Python Node Registration

### Using Helper Functions

The `remotemedia-webrtc` crate provides convenient helpers:

```rust
use remotemedia_webrtc::custom_nodes::{create_custom_registry, PythonNodeFactory};
use remotemedia_runtime_core::transport::PipelineRunner;
use std::sync::Arc;

// Simple: Register multiple Python nodes at once
let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry(&[
        ("OmniASRNode", false),
        ("KokoroTTSNode", true),
        ("MyCustomNode", true),
    ])
})?;

// Advanced: Mix with custom factory implementations
let runner = PipelineRunner::with_custom_registry(|| {
    use remotemedia_webrtc::custom_nodes::create_custom_registry_with_factories;
    
    create_custom_registry_with_factories(vec![
        Arc::new(PythonNodeFactory::new("WhisperASR", false)),
        Arc::new(MyRustNodeFactory),  // Custom Rust implementation
    ])
})?;
```

### Manual Registration

For full control, register nodes manually:

```rust
use remotemedia_runtime_core::nodes::{
    StreamingNodeRegistry,
    streaming_registry::create_default_streaming_registry,
};
use remotemedia_webrtc::custom_nodes::PythonNodeFactory;
use std::sync::Arc;

let runner = PipelineRunner::with_custom_registry(|| {
    // Start with default nodes
    let mut registry = create_default_streaming_registry();
    
    // Add custom Python nodes
    registry.register(Arc::new(PythonNodeFactory::new("MyASR", false)));
    registry.register(Arc::new(PythonNodeFactory::new("MyTTS", true)));
    
    registry
})?;
```

## Custom Rust Node Registration

### Step 1: Implement Your Node

```rust
use remotemedia_runtime_core::nodes::{StreamingNode, StreamingNodeError};
use remotemedia_runtime_core::data::RuntimeData;
use serde_json::Value;
use async_trait::async_trait;

pub struct MyCustomRustNode {
    config: MyConfig,
}

impl MyCustomRustNode {
    pub fn new(params: &Value) -> Result<Self, StreamingNodeError> {
        let config = serde_json::from_value(params.clone())?;
        Ok(Self { config })
    }
}

#[async_trait]
impl StreamingNode for MyCustomRustNode {
    fn node_type(&self) -> &str {
        "MyCustomRustNode"
    }
    
    async fn process_streaming(
        &self,
        input: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<Vec<RuntimeData>, StreamingNodeError> {
        // Your processing logic here
        let output = self.process(input)?;
        Ok(vec![output])
    }
}
```

### Step 2: Create Factory

```rust
use remotemedia_runtime_core::nodes::{StreamingNodeFactory, AsyncNodeWrapper};
use std::sync::Arc;

pub struct MyCustomRustNodeFactory;

impl StreamingNodeFactory for MyCustomRustNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = MyCustomRustNode::new(params)?;
        Ok(Box::new(AsyncNodeWrapper(Arc::new(node))))
    }
    
    fn node_type(&self) -> &str {
        "MyCustomRustNode"
    }
    
    fn is_python_node(&self) -> bool {
        false
    }
}
```

### Step 3: Register Factory

```rust
use remotemedia_runtime_core::transport::PipelineRunner;
use remotemedia_runtime_core::nodes::streaming_registry::create_default_streaming_registry;
use std::sync::Arc;

let runner = PipelineRunner::with_custom_registry(|| {
    let mut registry = create_default_streaming_registry();
    
    // Register your Rust node
    registry.register(Arc::new(MyCustomRustNodeFactory));
    
    // Mix with Python nodes if needed
    registry.register(Arc::new(PythonNodeFactory::new("MyPythonNode", true)));
    
    registry
})?;
```

## Complete Example: WebRTC Server

Here's a complete example showing how to set up a WebRTC server with custom nodes:

```rust
use remotemedia_webrtc::{WebRtcTransportConfig, custom_nodes::create_custom_registry};
use remotemedia_runtime_core::transport::PipelineRunner;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define your custom Python nodes
    let custom_nodes = vec!["WhisperASR", "GPT4TTS", "CustomFilter"];
    
    // Create PipelineRunner with custom registry
    let runner = Arc::new(PipelineRunner::with_custom_registry(move || {
        let node_specs: Vec<(&str, bool)> = custom_nodes
            .iter()
            .map(|name| (name.as_str(), true))
            .collect();
        
        create_custom_registry(&node_specs)
    })?);
    
    // Configure WebRTC transport
    let config = WebRtcTransportConfig {
        signaling_url: "ws://localhost:8080".to_string(),
        stun_servers: vec!["stun:stun.l.google.com:19302".to_string()],
        max_peers: 10,
        ..Default::default()
    };
    
    // Create and start transport
    let transport = WebRtcTransport::new(config)?;
    transport.start().await?;
    
    // Server is now running with custom nodes
    Ok(())
}
```

## Multi-Output vs Single-Output Nodes

When registering Python nodes, you need to specify whether they produce multiple outputs per input:

```rust
create_custom_registry(&[
    // Single output per input
    ("AudioResample", false),
    ("FormatConverter", false),
    
    // Multiple outputs per input (streaming/generative)
    ("StreamingTTS", true),      // Yields audio chunks
    ("VADDetector", true),       // Yields events + pass-through
    ("TextChunker", true),       // Splits into sentences
])
```

**Single-output nodes** (`false`):
- Produce exactly one output for each input
- Examples: filters, converters, classifiers

**Multi-output nodes** (`true`):
- Produce zero or more outputs per input
- Examples: TTS (yields multiple audio chunks), VAD (yields events), generators

## Best Practices

### 1. Use Helper Functions for Simple Cases

```rust
// ✅ Good: Simple and readable
let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry(&[("MyNode", true)])
})?;

// ❌ Avoid: Unnecessary complexity for simple cases
let runner = PipelineRunner::with_custom_registry(|| {
    let mut registry = StreamingNodeRegistry::new();
    registry.register(Arc::new(/* ... verbose factory ... */));
    registry
})?;
```

### 2. Start with Default Registry

Always extend the default registry rather than creating from scratch:

```rust
// ✅ Good: Includes all built-in nodes
let mut registry = create_default_streaming_registry();
registry.register(Arc::new(MyFactory));

// ❌ Bad: Missing all built-in nodes
let mut registry = StreamingNodeRegistry::new();
registry.register(Arc::new(MyFactory));
```

### 3. Use Descriptive Node Names

```rust
// ✅ Good: Clear purpose
create_custom_registry(&[
    ("WhisperLargeV3ASR", false),
    ("GPT4TurboTTS", true),
])

// ❌ Bad: Ambiguous
create_custom_registry(&[
    ("Node1", false),
    ("CustomNode", true),
])
```

### 4. Document Multi-Output Behavior

```rust
// ✅ Good: Clear documentation
registry.register(Arc::new(PythonNodeFactory::new(
    "StreamingTTS",
    true,  // Yields multiple 100ms audio chunks per text input
)));

// Add comments explaining expected output count
```

## Python Node Implementation

Your Python nodes should inherit from `MultiprocessNode`:

```python
from remotemedia.core.multiprocess_node import MultiprocessNode
from remotemedia.data import RuntimeData

class MyCustomNode(MultiprocessNode):
    """Custom ASR node using Whisper"""
    
    def __init__(self, node_id: str, params: dict):
        super().__init__(node_id, params)
        self.model = load_whisper_model(params.get("model", "large-v3"))
    
    async def process(self, data: RuntimeData) -> RuntimeData:
        # Single output
        transcription = self.model.transcribe(data.audio)
        return RuntimeData(text=transcription)
    
    # For multi-output (streaming):
    async def process(self, data: RuntimeData):
        # Yield multiple outputs
        for chunk in self.model.transcribe_streaming(data.audio):
            yield RuntimeData(text=chunk)
```

## Troubleshooting

### Node Not Found Error

```
Error: No streaming node factory registered for type 'MyNode'
```

**Solution**: Make sure you registered the node before creating sessions:

```rust
// Register BEFORE creating runner
let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry(&[("MyNode", true)])
})?;

// Then use runner
let session = runner.create_stream_session(manifest).await?;
```

### Python Node Not Starting

If your Python node doesn't initialize:

1. Check the Python class name matches exactly
2. Ensure the Python package is installed
3. Check logs for Python-side errors

```bash
# Enable debug logging
RUST_LOG=debug cargo run --bin webrtc_server
```

### Multi-Output Not Working

If a multi-output node only produces one result:

```rust
// ✅ Correct: Mark as multi-output
registry.register(Arc::new(PythonNodeFactory::new("StreamingTTS", true)));

// ❌ Wrong: Single-output flag
registry.register(Arc::new(PythonNodeFactory::new("StreamingTTS", false)));
```

## See Also

- [Node Registration Patterns](./NODE_REGISTRATION_PATTERNS.md) - Advanced patterns
- [Native Acceleration](./NATIVE_ACCELERATION.md) - Rust node performance
- [Python Integration](../python-client/README.md) - Python node development
- [WebRTC Transport](../transports/webrtc/README.md) - WebRTC-specific features

## Examples

Complete working examples can be found in:

- `transports/webrtc/src/bin/webrtc_server.rs` - Command-line registration
- `transports/webrtc/src/custom_nodes.rs` - Helper functions
- `runtime-core/examples/node_registration_example.rs` - Registration patterns
- `runtime-core/src/nodes/streaming_registry.rs` - Built-in node registration

