# Custom Nodes Quick Reference

## TL;DR

Register custom nodes in 3 ways:

### 1. Command Line (Easiest)

```bash
cargo run --bin webrtc_server --features grpc-signaling -- \
  --custom-nodes "Node1,Node2,Node3"
```

### 2. Code - Simple Python Nodes

```rust
use remotemedia_runtime_core::transport::PipelineRunner;
use remotemedia_webrtc::custom_nodes::create_custom_registry;

let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry(&[
        ("WhisperASR", false),
        ("GPT4TTS", true),
    ])
})?;
```

### 3. Code - Mixed Rust + Python

```rust
use remotemedia_webrtc::custom_nodes::create_custom_registry_with_factories;

let runner = PipelineRunner::with_custom_registry(|| {
    create_custom_registry_with_factories(vec![
        Arc::new(MyRustNodeFactory),
        Arc::new(PythonNodeFactory::new("MyPythonNode", true)),
    ])
})?;
```

## API Reference

### `PipelineRunner`

```rust
// Default (built-in nodes only)
let runner = PipelineRunner::new()?;

// With custom nodes
let runner = PipelineRunner::with_custom_registry(factory_fn)?;
```

### `create_custom_registry()`

```rust
fn create_custom_registry(python_nodes: &[(&str, bool)]) -> StreamingNodeRegistry
```

- **Input**: Array of `(node_type_name, is_multi_output)` tuples
- **Output**: Registry with default + custom Python nodes
- **Example**:
  ```rust
  create_custom_registry(&[
      ("NodeA", false),  // Single output
      ("NodeB", true),   // Multi-output
  ])
  ```

### `PythonNodeFactory`

```rust
PythonNodeFactory::new(node_type_name: &str, is_multi_output: bool)
```

- **Purpose**: Create factory for Python multiprocess node
- **Example**:
  ```rust
  Arc::new(PythonNodeFactory::new("WhisperASR", false))
  ```

## Multi-Output Flag

| Set to `true` | Set to `false` |
|--------------|----------------|
| TTS nodes (yield audio chunks) | Filters |
| Generators | Converters |
| VAD (yields events) | Classifiers |
| Text chunkers | Resamplers |

## Files to Know

| File | Purpose |
|------|---------|
| `runtime-core/src/transport/runner.rs` | Core `PipelineRunner` API |
| `transports/webrtc/src/custom_nodes.rs` | Helper functions |
| `transports/webrtc/src/bin/webrtc_server.rs` | CLI integration |
| `docs/CUSTOM_NODE_REGISTRATION.md` | Full guide |
| `examples/custom_nodes_server.rs` | Working examples |

## Common Patterns

### Pattern 1: Development Server

```bash
# Quick testing with custom nodes
export WEBRTC_CUSTOM_NODES="TestNode1,TestNode2"
cargo run --bin webrtc_server --features grpc-signaling
```

### Pattern 2: Production Server

```rust
// In your main.rs
let custom_nodes = config.custom_nodes; // From config file
let runner = Arc::new(PipelineRunner::with_custom_registry(move || {
    let specs: Vec<(&str, bool)> = custom_nodes
        .iter()
        .map(|n| (n.as_str(), true))
        .collect();
    create_custom_registry(&specs)
})?);
```

### Pattern 3: Rust Node Development

```rust
// 1. Implement your node
struct MyNode { /* ... */ }
impl StreamingNode for MyNode { /* ... */ }

// 2. Create factory
struct MyNodeFactory;
impl StreamingNodeFactory for MyNodeFactory {
    fn create(...) -> Result<Box<dyn StreamingNode>> {
        Ok(Box::new(AsyncNodeWrapper(Arc::new(MyNode::new(params)?))))
    }
    fn node_type(&self) -> &str { "MyNode" }
}

// 3. Register
let runner = PipelineRunner::with_custom_registry(|| {
    let mut registry = create_default_streaming_registry();
    registry.register(Arc::new(MyNodeFactory));
    registry
})?;
```

## Troubleshooting

| Error | Solution |
|-------|----------|
| "No streaming node factory registered for type 'X'" | Register the node before creating sessions |
| Python node not starting | Check class name matches exactly, enable `RUST_LOG=debug` |
| Multi-output not working | Set `is_multi_output` to `true` in factory |
| Import errors | Use `remotemedia_runtime_core::nodes::python_streaming::PythonStreamingNode` |

## See Also

- **Full Guide**: `docs/CUSTOM_NODE_REGISTRATION.md`
- **Implementation Details**: `docs/CUSTOM_NODES_IMPLEMENTATION_SUMMARY.md`
- **Python Node Development**: `python-client/README.md`
- **Working Example**: `cargo run --example custom_nodes_server`

