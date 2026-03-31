# Custom Node Registration Guide

This guide explains how to register custom nodes (both Rust and Python) in the RemoteMedia SDK.

## Overview

The RemoteMedia SDK provides multiple ways to register custom nodes:

1. **Python file-based registration** - Register from `.py` files at runtime
2. **Auto-registration via inventory** - Add crate dependency and nodes register automatically
3. **Builder API** - Fluent Rust API for programmatic registry construction
4. **Custom NodeProvider** - Create your own node crate with auto-registration
5. **Compile-time macros** - Legacy registration for known node names

## Quick Start

### Method 1: Python File-Based Registration (Recommended for Custom Nodes)

The easiest way to add custom Python nodes:

```python
from remotemedia import register_python_node

# Register all MultiprocessNode classes from a file
register_python_node("./my_nodes/custom_ml.py")

# Register with options
register_python_node(
    "./my_nodes/my_tts.py",
    node_type="MyTTS",        # Override class name
    multi_output=True,        # Node yields multiple outputs
    category="synthesis"
)
```

Your Python node file (`my_nodes/custom_ml.py`):

```python
from remotemedia.core.multiprocessing import MultiprocessNode

class MyMLNode(MultiprocessNode):
    async def initialize(self):
        # Load your model
        self.model = load_model()
    
    async def process(self, data):
        result = self.model.predict(data.audio)
        return RuntimeData(text=result)
```

### Method 2: Register a Class Directly

If you've already imported the class:

```python
from remotemedia import register_node_class
from my_project.nodes import MyCustomNode

register_node_class(MyCustomNode, multi_output=False, category="processing")
```

### Method 3: Configuration File

For multiple nodes:

```python
from remotemedia import register_python_nodes_from_config

register_python_nodes_from_config("nodes.yaml")
```

Example `nodes.yaml`:

```yaml
nodes:
  - file_path: ./nodes/transcription.py
    node_type: WhisperNode
    multi_output: false
    
  - file_path: ./nodes/synthesis.py  
    node_type: StreamingTTS
    multi_output: true
```

### Method 4: Using the @streaming_node Decorator

For nodes defined in your project:

```python
from remotemedia.nodes import streaming_node
from remotemedia.core.multiprocessing import MultiprocessNode

@streaming_node(
    node_type="SentimentAnalyzer",
    multi_output=False,
    accepts=["text"],
    produces=["text"],
    category="ml"
)
class SentimentAnalyzer(MultiprocessNode):
    async def process(self, data):
        result = self.analyze(data.text)
        return RuntimeData(text=result)
```

### Method 5: Builder API (Rust)

For programmatic Rust registration with a fluent API:

```rust
use remotemedia_core::nodes::StreamingNodeRegistry;
use std::sync::Arc;

let registry = StreamingNodeRegistry::builder()
    .with_defaults()                         // Include all default nodes
    .python("MyCustomASR")                   // Add single-output Python node
    .python_multi_output("MyStreamingTTS")   // Add multi-output Python node
    .python_batch(&["Node1", "Node2"])       // Batch register
    .factory(Arc::new(MyRustNodeFactory))    // Add custom factory
    .build();
```

### Method 6: Command-Line Registration (Legacy)

For WebRTC server with known nodes:

```bash
cargo run --bin webrtc_server --features grpc-signaling -- \
  --mode grpc \
  --custom-nodes "WhisperASR,GPT4TTS,CustomFilter"
```

## Python Node Registration

### From Python (Recommended)

Register Python nodes at runtime before creating pipelines:

```python
from remotemedia import register_python_node, register_node_class

# From file path
register_python_node("./my_nodes/asr.py")
register_python_node("./my_nodes/tts.py", multi_output=True)

# From class
from my_project import MyCustomNode
register_node_class(MyCustomNode)
```

### How It Works

When you call `register_python_node()`:

1. Python loads the `.py` file via importlib
2. Discovers all `MultiprocessNode` subclasses
3. Registers them in the internal `_NODE_REGISTRY`
4. When the Rust runtime creates a `PythonStreamingNode`, the multiprocess executor looks up the class in this registry

```
Python: register_python_node("./node.py")
         │
         ▼
Python: Loads file, finds MyNode(MultiprocessNode)
         │
         ▼
Python: _NODE_REGISTRY["MyNode"] = MyNode
         │
         │  Later, when pipeline runs:
         ▼
Rust:   Creates PythonStreamingNode("MyNode")
         │
         ▼
Rust:   Multiprocess executor spawns Python process
         │
         ▼
Python: Looks up MyNode in _NODE_REGISTRY, instantiates it
```

### From Rust (for Built-in Python Nodes)

For Python nodes known at compile time, use the `remotemedia-python-nodes` crate:

```rust
use remotemedia_python_nodes::{register_python_node, PythonNodeConfig};

// Register with default settings
register_python_node(PythonNodeConfig::new("WhisperXNode"));

// Register with full configuration  
register_python_node(
    PythonNodeConfig::new("KokoroTTSNode")
        .with_multi_output(true)
        .with_category("tts")
);
```

### Legacy Helper Functions

The `remotemedia-webrtc` crate still provides helpers for manual registration:

```rust
use remotemedia_webrtc::custom_nodes::{create_custom_registry, PythonNodeFactory};
use remotemedia_runtime_core::transport::PipelineRunner;

let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry(&[
        ("OmniASRNode", false),
        ("KokoroTTSNode", true),
    ])
})?;
```

## Custom Rust Node Registration

### Method 1: Create a Node Crate with Auto-Registration (Recommended)

For reusable node libraries, create a crate with a `NodeProvider`:

```rust
// my-nodes/src/lib.rs
use remotemedia_core::nodes::{
    NodeProvider, StreamingNodeRegistry, StreamingNodeFactory, StreamingNode
};
use std::sync::Arc;

// Your node implementation
pub struct MyCustomNode { /* ... */ }

impl StreamingNode for MyCustomNode {
    fn node_type(&self) -> &str { "MyCustomNode" }
    // ...
}

// Factory
pub struct MyCustomNodeFactory;

impl StreamingNodeFactory for MyCustomNodeFactory {
    fn node_type(&self) -> &str { "MyCustomNode" }
    fn create(&self, node_id: String, params: &Value, session_id: Option<String>) 
        -> Result<Box<dyn StreamingNode>, Error> 
    {
        Ok(Box::new(MyCustomNode::new(params)?))
    }
}

// Provider - auto-registers when crate is linked
pub struct MyNodesProvider;

impl NodeProvider for MyNodesProvider {
    fn register(&self, registry: &mut StreamingNodeRegistry) {
        registry.register(Arc::new(MyCustomNodeFactory));
    }
    
    fn provider_name(&self) -> &'static str { "my-nodes" }
    fn priority(&self) -> i32 { 100 }  // Lower = earlier
}

// Auto-register via inventory
inventory::submit! { &MyNodesProvider as &'static dyn NodeProvider }
```

Users just add your crate as a dependency:

```toml
[dependencies]
remotemedia-core = "0.4"
my-nodes = "1.0"  # Your crate - nodes auto-register!
```

### Method 2: Manual Registration

For one-off nodes in your application:

#### Step 1: Implement Your Node

```rust
use remotemedia_core::nodes::{StreamingNode, StreamingNodeError};
use remotemedia_core::data::RuntimeData;
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
        let output = self.process(input)?;
        Ok(vec![output])
    }
}
```

#### Step 2: Create Factory

```rust
use remotemedia_core::nodes::{StreamingNodeFactory, AsyncNodeWrapper};
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

#### Step 3: Register Factory

```rust
use remotemedia_core::transport::PipelineRunner;
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use std::sync::Arc;

let runner = PipelineRunner::with_custom_registry(|| {
    let mut registry = create_default_streaming_registry();
    
    // Register your Rust node
    registry.register(Arc::new(MyCustomRustNodeFactory));
    
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
from remotemedia.core.multiprocessing import MultiprocessNode
from remotemedia.data import RuntimeData

class MyCustomNode(MultiprocessNode):
    """Custom ASR node using Whisper"""
    
    async def initialize(self):
        """Called once when node starts"""
        self.model = load_whisper_model(self.params.get("model", "large-v3"))
    
    async def process(self, data: RuntimeData) -> RuntimeData:
        """Single output per input"""
        transcription = self.model.transcribe(data.audio)
        return RuntimeData(text=transcription)
```

For multi-output (streaming) nodes, use a generator:

```python
from remotemedia.nodes import streaming_node

@streaming_node(node_type="StreamingTTS", multi_output=True)
class StreamingTTSNode(MultiprocessNode):
    async def initialize(self):
        self.tts = load_tts_model()
    
    async def process(self, data: RuntimeData):
        """Yield multiple outputs per input"""
        for audio_chunk in self.tts.synthesize_streaming(data.text):
            yield RuntimeData(audio=audio_chunk)
```

### Complete Example: Custom ML Node

```python
# my_nodes/sentiment.py
from remotemedia.core.multiprocessing import MultiprocessNode
from remotemedia.nodes import streaming_node

@streaming_node(
    node_type="SentimentAnalyzer",
    accepts=["text"],
    produces=["text"],
    category="ml"
)
class SentimentAnalyzer(MultiprocessNode):
    async def initialize(self):
        from transformers import pipeline
        self.model = pipeline("sentiment-analysis")
    
    async def process(self, data):
        result = self.model(data.text)[0]
        return RuntimeData(text=f"{result['label']}: {result['score']:.2f}")
```

Then register it:

```python
from remotemedia import register_python_node

register_python_node("./my_nodes/sentiment.py")
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

- [Node Registration Patterns](./NODE_REGISTRATION_PATTERNS.md) - Advanced patterns and design decisions
- [Native Acceleration](./NATIVE_ACCELERATION.md) - Rust node performance optimization
- [Python Client](../clients/python/README.md) - Python SDK documentation
- [Python Nodes Crate](../crates/python-nodes/README.md) - Dynamic Python node infrastructure

## Examples

Complete working examples can be found in:

- `clients/python/remotemedia/nodes/loader.py` - Python file-based registration
- `clients/python/remotemedia/nodes/registration.py` - `@streaming_node` decorator
- `crates/python-nodes/src/provider.rs` - `NodeProvider` implementation
- `crates/core/src/nodes/core_provider.rs` - Core nodes provider
- `crates/candle-nodes/src/registry.rs` - Candle ML nodes provider

