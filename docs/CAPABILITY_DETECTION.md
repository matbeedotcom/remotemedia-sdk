# Capability Detection: Concrete Indicators Only

**Status**: ✅ Implemented (Phase 1.3.6-1.3.8 Complete)

## Philosophy

**We do NOT guess capabilities from node names.** Instead, we use concrete, explicit indicators:

1. **Manifest declarations** (user explicitly states requirements)
2. **Parameter analysis** (device="cuda", threads=4, model="large")
3. **Compilation metadata** (future: build.rs detects native dependencies)

## Detection Priority

```
1. execution.placement = "remote"     ← Highest priority (explicit user declaration)
2. capabilities { gpu, memory, cpu }  ← Explicit requirements
3. params { device, threads, model }  ← Concrete parameter values
4. Node registry (future)             ← Build-time dependency analysis
5. Defaults (browser-compatible)      ← Safe defaults
```

## Example: Whisper Node

### ❌ Old Approach (Name-based guessing)
```rust
if node_type.contains("whisper") {
    requires_threads = true;        // Guessing!
    requires_native_libs = true;    // Assuming!
}
```

### ✅ New Approach (Explicit declarations)

**Option 1: Explicit execution metadata in manifest**
```json
{
  "id": "whisper-1",
  "node_type": "WhisperNode",
  "execution": {
    "placement": "remote",
    "reason": "requires_native_libs_and_threads"
  }
}
```

**Option 2: Explicit capability requirements**
```json
{
  "id": "whisper-1",
  "node_type": "WhisperNode",
  "capabilities": {
    "memory_gb": 0.5,
    "cpu": { "cores": 4 }
  }
}
```

**Option 3: Parameters indicate requirements**
```json
{
  "id": "whisper-1",
  "node_type": "WhisperNode",
  "params": {
    "threads": 4,          ← Concrete: needs threading
    "device": "cpu"        ← Concrete: CPU execution
  }
}
```

**Option 4: Build-time detection (future)**
```rust
// build.rs analyzes dependencies
#[cfg(feature = "whisper-wasm")]
static WHISPER_NODE_CAPS: NodeCapabilities = NodeCapabilities {
    requires_native_libs: true,   // Detected: links whisper.cpp
    requires_threads: true,        // Detected: uses pthread
    estimated_memory_mb: 512,      // Measured at compile time
};
```

## Concrete Indicators

### GPU Requirements
```rust
// From params
if params["device"] == "cuda" → requires_gpu = true, gpu_type = Cuda
if params["device"] == "metal" → requires_gpu = true, gpu_type = Metal

// From manifest
capabilities.gpu.type = "cuda" → requires_gpu = true
```

### Threading Requirements
```rust
// From params
if params["threads"] > 1 → requires_threads = true
if params["num_workers"] > 1 → requires_threads = true

// From manifest
capabilities.cpu.cores > 1 → requires_threads = true
```

### Memory Requirements
```rust
// From model path
if params["model"].contains("large") → estimated_memory_mb = 2048
if params["model"].contains("tiny") → estimated_memory_mb = 256

// From manifest
capabilities.memory_gb = 8.0 → estimated_memory_mb = 8192
```

### File I/O
```rust
// From params
if params.contains("input_file") || params.contains("output_file")
→ May not work in browser, prefer local execution
```

## Future: Build-Time Detection

### Goal
Automatically detect capabilities when a node is **compiled**, not when its name is read.

### Implementation Plan

**Step 1: Dependency Analysis in build.rs**
```rust
// runtime/build.rs
fn analyze_node_dependencies() {
    // Check Cargo.toml dependencies
    if has_dependency("whisper-rs") {
        register_capability("WhisperNode", NodeCapabilities {
            requires_native_libs: true,
            requires_threads: true,
            estimated_memory_mb: 512,
        });
    }

    if has_dependency("cuda") || has_feature("gpu-accel") {
        // GPU nodes detected at compile time
    }
}
```

**Step 2: Python Dependency Introspection**
```python
# Analyze Python node imports
import ast

def analyze_python_node(node_file):
    tree = ast.parse(open(node_file).read())
    imports = [node.names[0].name for node in ast.walk(tree) if isinstance(node, ast.Import)]

    caps = {}
    if "whisper" in imports:
        caps["requires_native_libs"] = True
    if "torch" in imports and "cuda" in source:
        caps["requires_gpu"] = True

    return caps
```

**Step 3: Store in Registry**
```rust
// Generated at compile time
lazy_static! {
    pub static ref NODE_CAPABILITIES: HashMap<String, NodeCapabilities> = {
        let mut m = HashMap::new();
        m.insert("WhisperNode", /* detected caps */);
        m.insert("CudaInferenceNode", /* detected caps */);
        m
    };
}
```

## Developer Workflow

### Creating a New Node

**Option A: Declare capabilities explicitly**
```python
class MyHeavyNode(Node):
    # Explicit metadata
    CAPABILITIES = {
        "requires_threads": True,
        "requires_gpu": True,
        "estimated_memory_mb": 2048,
    }
```

**Option B: Use parameters**
```python
class MyNode(Node):
    def __init__(self, device="cpu", threads=1):
        # Capabilities inferred from params
        self.device = device   # "cuda" → requires_gpu
        self.threads = threads  # > 1 → requires_threads
```

**Option C: Let build detect (future)**
```python
# Just import what you need
import whisper  # Build detects: requires native libs
import torch    # Build detects: may need GPU
```

## Testing

All 9 tests pass:
- ✅ Explicit execution metadata
- ✅ Explicit capability requirements
- ✅ Parameter-based detection (device, threads, model)
- ✅ Pipeline aggregation
- ✅ Safe defaults (browser-compatible)

## Summary

**Key Principle**: Nodes declare what they need, we don't guess from names.

**Current**: Explicit manifest declarations + parameter analysis
**Future**: Build-time dependency detection + import analysis
**Never**: String matching on node type names
