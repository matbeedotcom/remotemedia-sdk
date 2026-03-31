# Node Registration Patterns

**Goal**: Make it easy for library developers and end-users to register custom nodes in the RemoteMedia SDK with minimal boilerplate.

## Implementation Status

| Level | Feature | Status |
|-------|---------|--------|
| Level 1 | Helper Macros | ✅ Implemented |
| Level 2 | Builder API | ⏳ Planned |
| Level 3 | Auto-Discovery | ✅ Implemented |
| Level 4 | Plugin System | ⏳ Planned |

## Current Architecture

The SDK now uses a **NodeProvider + inventory** system for automatic node registration:

### Auto-Registration Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Application links crates                                     │
│   Cargo.toml:                                               │
│     remotemedia-core                                         │
│     remotemedia-python-nodes                                │
│     remotemedia-candle-nodes                                │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│ inventory crate collects all NodeProvider implementations   │
│   - CoreNodesProvider (priority: 1000)                      │
│   - PythonNodesProvider (priority: 500)                     │
│   - CandleNodesProvider (priority: 100)                     │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│ create_default_streaming_registry()                          │
│   - Sorts providers by priority                              │
│   - Calls provider.register() for each                       │
│   - Returns populated StreamingNodeRegistry                  │
└─────────────────────────────────────────────────────────────┘
```

### Python Node Registration Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Developer creates ./my_nodes/custom.py                       │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│ Python: register_python_node("./my_nodes/custom.py")        │
│   - Loads file via importlib                                │
│   - Discovers MultiprocessNode subclasses                   │
│   - Registers in _NODE_REGISTRY                             │
└─────────────────────┬───────────────────────────────────────┘
                      │  When pipeline runs
                      ▼
┌─────────────────────────────────────────────────────────────┐
│ Rust: PythonStreamingNode("CustomNode")                      │
│   - Multiprocess executor spawns Python process             │
│   - Python looks up class in _NODE_REGISTRY                 │
│   - Instantiates and runs node                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Historical Context: Pain Points (Solved)

### Previous Registration (Rust)

```rust
// 40+ lines of boilerplate per node
struct MyAudioNodeFactory;
impl NodeFactory for MyAudioNodeFactory { /* ... */ }
registry.register_rust(Arc::new(MyAudioNodeFactory));
```

**Solved by**: `NodeProvider` trait with `inventory` auto-collection.

### Previous Registration (Python)

```rust
// Rust side needed factory boilerplate for each Python node
struct MyPythonNodeFactory;
impl NodeFactory for MyPythonNodeFactory { /* ... */ }
registry.register_python(Arc::new(MyPythonNodeFactory));
```

**Solved by**: Dynamic Python registration via `register_python_node()`.

---

## Available Solutions

### Level 1: Helper Macros (Simple, Immediate Value)

**Implementation Complexity:** Low (1-2 hours)  
**User Experience:** Much better  
**Maintenance:** Low

#### Rust Side

```rust
// Define your node (same as before)
struct MyAudioNode {
    sample_rate: u32,
}

#[async_trait]
impl NodeHandler for MyAudioNode {
    async fn initialize(&mut self, params: &Value) -> Result<()> {
        self.sample_rate = params.get("sample_rate")
            .and_then(|v| v.as_u64())
            .unwrap_or(16000) as u32;
        Ok(())
    }
    
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        // Process audio...
        Ok(vec![input])
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

// NEW: One-line registration with helper macro
register_rust_node!(registry, MyAudioNode, |params| {
    MyAudioNode { sample_rate: 16000 }
});

// Or even simpler if node implements Default:
register_rust_node_default!(registry, MyAudioNode);
```

**Macro Implementation:**

```rust
/// Register a Rust node with custom factory closure
#[macro_export]
macro_rules! register_rust_node {
    ($registry:expr, $node_type:ty, $factory:expr) => {{
        struct Factory;
        impl NodeFactory for Factory {
            fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>> {
                let handler: Box<dyn NodeHandler> = Box::new(($factory)(params)?);
                Ok(Box::new(RustNodeExecutor::new(
                    stringify!($node_type),
                    handler
                )))
            }
            
            fn node_type(&self) -> &str {
                stringify!($node_type)
            }
            
            fn is_rust_native(&self) -> bool {
                true
            }
        }
        $registry.register_rust(Arc::new(Factory));
    }};
}

/// Register a Rust node that implements Default
#[macro_export]
macro_rules! register_rust_node_default {
    ($registry:expr, $node_type:ty) => {{
        register_rust_node!($registry, $node_type, |_params| {
            Ok(<$node_type>::default())
        });
    }};
}
```

#### Python Side

```rust
/// Register a Python multiprocess node
#[macro_export]
macro_rules! register_python_node {
    ($registry:expr, $node_name:literal) => {{
        struct Factory;
        impl NodeFactory for Factory {
            fn create(&self, _params: Value) -> Result<Box<dyn NodeExecutor>> {
                Ok(Box::new(PythonNodeExecutor::new($node_name)))
            }
            
            fn node_type(&self) -> &str {
                $node_name
            }
            
            fn is_rust_native(&self) -> bool {
                false
            }
        }
        $registry.register_python(Arc::new(Factory));
    }};
}
```

**Usage:**

```rust
// In your registry setup
let mut registry = NodeRegistry::new();

// Rust nodes
register_rust_node_default!(registry, FastResampleNode);
register_rust_node_default!(registry, AudioChunkerNode);

// Python nodes (just the class name)
register_python_node!(registry, "OmniASRNode");
register_python_node!(registry, "KokoroTTSNode");
register_python_node!(registry, "SimplePyTorchNode");
```

**Benefits:**
- Reduces 40 lines → 1 line per node
- Type-safe (uses stringify! for consistency)
- No manual factory boilerplate
- Easy to audit all registered nodes

---

### Level 2: Fluent Builder API (Better Ergonomics)

**Implementation Complexity:** Medium (3-4 hours)  
**User Experience:** Excellent  
**Maintenance:** Medium

```rust
let registry = NodeRegistry::builder()
    // Rust nodes with auto-discovery from type
    .rust::<FastResampleNode>()
    .rust::<AudioChunkerNode>()
    .rust_with::<SileroVADNode>(|params| {
        SileroVADNode::new(params.get("model_path"))
    })
    
    // Python nodes by name
    .python("OmniASRNode")
    .python("KokoroTTSNode")
    
    // Bulk registration from array
    .python_batch(&[
        "SimplePyTorchNode",
        "ExpanderNode",
        "FilterNode",
    ])
    
    // Conditional registration (feature flags)
    .rust_if_enabled::<CudaAcceleratorNode>("cuda")
    
    .build();
```

**Implementation:**

```rust
pub struct NodeRegistryBuilder {
    registry: NodeRegistry,
}

impl NodeRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: NodeRegistry::new(),
        }
    }
    
    /// Register Rust node (requires Default)
    pub fn rust<T>(mut self) -> Self 
    where
        T: NodeHandler + Default + 'static,
    {
        let node_type = std::any::type_name::<T>();
        register_rust_node_default!(self.registry, T);
        self
    }
    
    /// Register Rust node with custom factory
    pub fn rust_with<T, F>(mut self, factory: F) -> Self
    where
        T: NodeHandler + 'static,
        F: Fn(Value) -> Result<T> + Send + Sync + 'static,
    {
        register_rust_node!(self.registry, T, factory);
        self
    }
    
    /// Register Python node by name
    pub fn python(mut self, node_type: &'static str) -> Self {
        register_python_node!(self.registry, node_type);
        self
    }
    
    /// Register multiple Python nodes
    pub fn python_batch(mut self, node_types: &[&'static str]) -> Self {
        for &node_type in node_types {
            register_python_node!(self.registry, node_type);
        }
        self
    }
    
    /// Conditional registration based on feature flag
    #[cfg(feature = "conditional-registration")]
    pub fn rust_if_enabled<T>(self, feature: &str) -> Self
    where
        T: NodeHandler + Default + 'static,
    {
        if cfg!(feature = feature) {
            self.rust::<T>()
        } else {
            self
        }
    }
    
    pub fn build(self) -> NodeRegistry {
        self.registry
    }
}

impl NodeRegistry {
    pub fn builder() -> NodeRegistryBuilder {
        NodeRegistryBuilder::new()
    }
}
```

---

### Level 3: Attribute Macros + Auto-Discovery (Most Ergonomic)

**Implementation Complexity:** High (8-12 hours)  
**User Experience:** Excellent  
**Maintenance:** Medium-High

#### Rust Side with Attributes

```rust
use remotemedia_runtime::node;

#[node(
    name = "FastResample",
    category = "audio",
    description = "High-quality audio resampling using rubato"
)]
struct FastResampleNode {
    input_rate: u32,
    output_rate: u32,
    resampler: Option<FftFixedIn<f32>>,
}

#[node_impl]
impl FastResampleNode {
    #[node_init]
    async fn initialize(&mut self, params: &Value) -> Result<()> {
        // Extract params
        self.input_rate = params.get("input_rate")?.as_u64()? as u32;
        self.output_rate = params.get("output_rate")?.as_u64()? as u32;
        
        // Create resampler
        self.resampler = Some(FftFixedIn::new(
            self.input_rate as usize,
            self.output_rate as usize,
            1024,
            2,
        )?);
        
        Ok(())
    }
    
    #[node_process]
    async fn process(&mut self, input: RuntimeData) -> Result<Vec<RuntimeData>> {
        // Process audio...
        Ok(vec![input])
    }
}

// That's it! Node is automatically registered at compile time.
// No manual factory, no registration call.
```

**Auto-Registration:**

```rust
// In your lib.rs or main.rs
use remotemedia_runtime::collect_nodes;

// Automatically collects all #[node] annotated types at compile time
let registry = collect_nodes!();

// Or with manual additions:
let registry = collect_nodes!()
    .add_python("OmniASRNode")
    .add_python("KokoroTTSNode");
```

#### Python Side with Auto-Discovery

**Option A: Decorator-based (similar to Flask routes)**

```python
from remotemedia.nodes import node, NodeRegistry

# Global registry (can be application-specific)
registry = NodeRegistry()

@node(
    name="OmniASR",
    category="speech",
    description="Multilingual speech recognition",
    auto_register=True  # Automatically add to registry on import
)
class OmniASRNode(MultiprocessNode):
    async def initialize(self):
        # Load model...
        pass
    
    async def process(self, data: RuntimeData):
        # Transcribe audio...
        pass

# That's it! Node is registered on import.
```

**Option B: Directory scanning (similar to Django apps)**

```python
# In your config or startup
from remotemedia.nodes import discover_nodes

# Auto-discover all MultiprocessNode subclasses in these paths
registry = discover_nodes([
    "remotemedia.nodes",
    "my_custom_nodes",
    "/path/to/project/nodes",
])

# Or with filters
registry = discover_nodes(
    paths=["remotemedia.nodes"],
    include_patterns=["*_node.py"],
    exclude_patterns=["test_*.py"],
)
```

**Implementation (Python Discovery):**

```python
import importlib
import inspect
import pkgutil
from pathlib import Path
from typing import List, Type, Dict

def discover_nodes(
    paths: List[str],
    include_patterns: List[str] = None,
    exclude_patterns: List[str] = None,
) -> Dict[str, Type[MultiprocessNode]]:
    """
    Auto-discover MultiprocessNode subclasses in given paths.
    
    Args:
        paths: Module names or filesystem paths to search
        include_patterns: Glob patterns for files to include
        exclude_patterns: Glob patterns for files to exclude
        
    Returns:
        Dictionary mapping node names to node classes
    """
    discovered = {}
    
    for path in paths:
        # Try as module import first
        try:
            module = importlib.import_module(path)
            module_path = Path(module.__file__).parent
        except ImportError:
            # Try as filesystem path
            module_path = Path(path)
            if not module_path.exists():
                continue
        
        # Walk the directory tree
        for py_file in module_path.rglob("*.py"):
            # Apply filters
            if exclude_patterns and any(py_file.match(p) for p in exclude_patterns):
                continue
            if include_patterns and not any(py_file.match(p) for p in include_patterns):
                continue
            
            # Import module
            module_name = _path_to_module_name(py_file, module_path)
            try:
                module = importlib.import_module(module_name)
            except Exception as e:
                logging.warning(f"Failed to import {module_name}: {e}")
                continue
            
            # Find MultiprocessNode subclasses
            for name, obj in inspect.getmembers(module, inspect.isclass):
                if (issubclass(obj, MultiprocessNode) and 
                    obj is not MultiprocessNode and
                    obj.__module__ == module.__name__):
                    
                    discovered[name] = obj
    
    return discovered
```

---

### Level 4: Plugin System with Dynamic Loading (Most Flexible)

**Implementation Complexity:** High (12-16 hours)  
**User Experience:** Excellent (especially for 3rd party)  
**Maintenance:** High

```rust
// Load nodes from dynamic libraries (.so/.dll)
let registry = NodeRegistry::new()
    .load_plugin("path/to/my_nodes.so")?
    .load_plugin_dir("/usr/local/lib/remotemedia/plugins")?
    
    // Or from TOML config
    .load_from_config("runtime.toml")?;
```

**Config File (runtime.toml):**

```toml
[nodes.rust]
# Auto-discover from these plugin directories
plugin_dirs = [
    "/usr/local/lib/remotemedia/plugins",
    "~/.remotemedia/plugins",
    "./custom_nodes"
]

# Specific plugins to load
plugins = [
    { path = "libmy_audio_nodes.so", enabled = true },
    { path = "libcuda_accelerator.so", enabled = false, requires = ["cuda"] }
]

[nodes.python]
# Python module paths to scan
module_paths = [
    "remotemedia.nodes",
    "my_project.custom_nodes"
]

# Individual Python nodes
nodes = [
    { class = "OmniASRNode", module = "remotemedia.nodes.omniasr" },
    { class = "KokoroTTSNode", module = "remotemedia.nodes.kokoro" }
]
```

---

## Implementation Status

### Phase 1: Helper Macros ✅ COMPLETE
- `register_rust_node!` and `register_python_node!` macros in `registration_macros.rs`
- `register_rust_node_default!` for Default-implementing types
- **Result:** 97% reduction in boilerplate for compile-time registration

### Phase 2: NodeProvider + Inventory ✅ COMPLETE
- `NodeProvider` trait in `crates/core/src/nodes/provider.rs`
- `inventory` crate for compile-time collection
- `CoreNodesProvider`, `PythonNodesProvider`, `CandleNodesProvider`
- **Result:** Add node crate to Cargo.toml → nodes auto-register

### Phase 3: Python Auto-Discovery ✅ COMPLETE
- `register_python_node()` function in `clients/python/remotemedia/nodes/loader.py`
- `@streaming_node` decorator in `clients/python/remotemedia/nodes/registration.py`
- `register_node_class()` for direct class registration
- `register_python_nodes_from_config()` for YAML/JSON configs
- **Result:** No Rust code needed for custom Python nodes

### Phase 4: Builder API ✅ COMPLETE
- `StreamingNodeRegistryBuilder` fluent API in `crates/core/src/nodes/builder.rs`
- Methods: `.with_defaults()`, `.python()`, `.python_multi_output()`, `.python_batch()`, `.factory()`, `.provider()`
- **Result:** Type-safe, chainable registry construction

### Phase 5: Config-Based Loading ⏳ PLANNED
- TOML-based node configuration
- Plugin system for dynamic loading (.so/.dll)
- **Status:** Not yet implemented

---

## Example: Before & After

### Before (Current)

```rust
// 120 lines of boilerplate for 3 nodes
struct AudioChunkerNodeFactory;
impl NodeFactory for AudioChunkerNodeFactory {
    fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>> {
        // ...
    }
    fn node_type(&self) -> &str { "AudioChunker" }
    fn is_rust_native(&self) -> bool { true }
}

struct FastResampleNodeFactory;
impl NodeFactory for FastResampleNodeFactory {
    fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>> {
        // ...
    }
    fn node_type(&self) -> &str { "FastResample" }
    fn is_rust_native(&self) -> bool { true }
}

struct OmniASRNodeFactory;
impl NodeFactory for OmniASRNodeFactory {
    fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>> {
        Ok(Box::new(PythonNodeExecutor::new("OmniASRNode")))
    }
    fn node_type(&self) -> &str { "OmniASRNode" }
    fn is_rust_native(&self) -> bool { false }
}

let mut registry = NodeRegistry::new();
registry.register_rust(Arc::new(AudioChunkerNodeFactory));
registry.register_rust(Arc::new(FastResampleNodeFactory));
registry.register_python(Arc::new(OmniASRNodeFactory));
```

### After (Level 1: Macros)

```rust
// 6 lines for 3 nodes
let mut registry = NodeRegistry::new();

register_rust_node_default!(registry, AudioChunkerNode);
register_rust_node_default!(registry, FastResampleNode);

register_python_node!(registry, "OmniASRNode");
```

### After (Level 2: Builder)

```rust
// 4 lines for 3 nodes (with type safety)
let registry = NodeRegistry::builder()
    .rust::<AudioChunkerNode>()
    .rust::<FastResampleNode>()
    .python("OmniASRNode")
    .build();
```

### After (Level 3: Auto-Discovery)

```rust
// 1 line for ALL nodes
let registry = collect_nodes!(); // Finds all #[node] annotated types
```

---

## Migration Path

All levels are **backward compatible**. Existing code continues to work:

```rust
// Old style still works
registry.register_rust(Arc::new(CustomFactory));

// New style is opt-in
register_rust_node_default!(registry, NewNode);
```

---

## Additional Features

### Node Metadata & Discovery

```rust
// Query available nodes
let nodes = registry.list_node_types();
for node_type in nodes {
    if let Some(meta) = registry.get_metadata(&node_type) {
        println!("{}: {}", node_type, meta.description);
        println!("  Category: {}", meta.category);
        println!("  Runtime: {}", if meta.is_rust { "Rust" } else { "Python" });
    }
}
```

### Testing Helpers

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_all_nodes_registered() {
        let registry = create_default_streaming_registry();
        
        // Auto-verify all expected nodes are present
        assert_node_registered!(registry, "AudioChunker");
        assert_node_registered!(registry, "FastResample");
        assert_node_registered!(registry, "OmniASRNode");
    }
}
```

### CLI Tools

```bash
# List all available nodes
$ remotemedia-nodes list
Rust Nodes:
  - AudioChunker (audio/processing)
  - FastResample (audio/processing)
  
Python Nodes:
  - OmniASRNode (speech/recognition)
  - KokoroTTSNode (speech/synthesis)

# Inspect node details
$ remotemedia-nodes info OmniASRNode
Name: OmniASRNode
Type: Python (multiprocess)
Category: speech/recognition
Description: Multilingual speech recognition (200+ languages)
Parameters:
  - model_card: string (required)
  - language: string (optional)
  - chunking_mode: enum [none, vad, fixed] (default: none)
```

---

## Design Decisions (Resolved)

1. **Which level should we implement first?**
   - ✅ **Resolved**: Implemented Levels 1 + 3 first (macros + auto-discovery)
   - Level 2 (builder) can be added later if needed

2. **Should we support compile-time registration?**
   - ✅ **Resolved**: Yes, via `inventory` crate
   - Zero runtime cost, providers collected at link time

3. **Python node discovery: decorator vs directory scan?**
   - ✅ **Resolved**: Both supported
   - `@streaming_node` decorator for explicit registration
   - `register_python_node()` for file-based discovery

4. **Config file format: TOML, YAML, or JSON?**
   - ✅ **Resolved**: YAML and JSON (Python-native)
   - `register_python_nodes_from_config()` supports both

5. **Should we create a centralized node registry/marketplace?**
   - ⏳ **Future consideration**
   - Current focus is on local/project-level registration

---

## Conclusion

The node registration system has been modernized to support:

1. **Zero-boilerplate Rust nodes** via `NodeProvider` + `inventory`
2. **File-based Python registration** via `register_python_node()`
3. **Decorator-based Python nodes** via `@streaming_node`
4. **Auto-discovery** when crates are linked

### Quick Reference

| Use Case | Solution |
|----------|----------|
| Add built-in nodes | Add crate to `Cargo.toml` (auto-registers) |
| Custom Python node | `register_python_node("./my_node.py")` |
| Python node class | `register_node_class(MyNode)` |
| Create node crate | Implement `NodeProvider`, use `inventory::submit!` |
| Legacy registration | `register_python_node!` / `register_rust_node!` macros |

### Future Work

- **TOML config** for declarative node loading
- **Plugin system** for dynamic `.so/.dll` loading

