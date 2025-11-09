# Node Executor API Contract

**Interface**: Rust `NodeExecutor` trait  
**Protocol**: async_trait  
**Version**: 1.0.0

## Overview

Defines the contract that all node implementations (Rust native and CPython wrapper) must satisfy.

---

## Trait Definition

```rust
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Initialize the node with parameters from manifest
    async fn initialize(&mut self, params: &Value) -> Result<()>;
    
    /// Execute the node on input data
    async fn execute(&self, input: Value) -> Result<Value>;
    
    /// Cleanup resources (optional, default no-op)
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// Get node metadata (name, version, capabilities)
    fn metadata(&self) -> NodeMetadata;
}
```

---

## NodeMetadata

```rust
pub struct NodeMetadata {
    /// Node type name (e.g., "VADNode")
    pub name: String,
    
    /// Semantic version (e.g., "1.0.0")
    pub version: String,
    
    /// Required capabilities (e.g., Audio, GPU)
    pub capabilities: Vec<Capability>,
    
    /// Parameter schema (for validation and documentation)
    pub parameters: ParameterSchema,
}

pub enum Capability {
    Audio,
    Video,
    GPU,
    CPU,
}

pub struct ParameterSchema {
    pub required: Vec<String>,
    pub optional: Vec<(String, Value)>,  // (name, default_value)
}
```

---

## Lifecycle Contract

### Initialization

**Called**: Once per pipeline execution, before any `execute()` calls

**Purpose**: Validate parameters, allocate resources

**Contract**:
- MUST validate all required parameters
- MAY load models, allocate buffers
- MUST return `Err` if parameters are invalid
- MUST be idempotent (safe to call multiple times)

**Example**:
```rust
async fn initialize(&mut self, params: &Value) -> Result<()> {
    // Extract and validate parameters
    self.threshold = params.get("threshold")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow!("Missing required parameter: threshold"))?
        as f32;
    
    // Validate range
    if self.threshold < -60.0 || self.threshold > 0.0 {
        return Err(anyhow!("threshold must be in range [-60.0, 0.0]"));
    }
    
    Ok(())
}
```

---

### Execution

**Called**: One or more times per pipeline execution

**Purpose**: Process input data and produce output

**Contract**:
- MUST accept `Value` (JSON-serializable or numpy array)
- MUST produce `Value` output
- MUST be thread-safe (can be called concurrently)
- MUST NOT mutate `self` (use interior mutability if needed)
- SHOULD release GIL for CPU-intensive work (Rust native nodes automatic)
- MUST handle errors gracefully (return `Err`, don't panic)

**Example**:
```rust
async fn execute(&self, input: Value) -> Result<Value> {
    // Extract audio data
    let audio = extract_audio(&input)?;
    
    // Process (computation-heavy, GIL released)
    let segments = self.detect_voice_activity(&audio)?;
    
    // Return as JSON
    Ok(serde_json::to_value(segments)?)
}
```

---

### Cleanup

**Called**: Once per pipeline execution, after all `execute()` calls

**Purpose**: Release resources, close files, deallocate buffers

**Contract**:
- MUST release all resources (files, memory, GPU)
- MUST be idempotent (safe to call multiple times)
- SHOULD NOT fail (log errors instead of returning `Err`)

**Example**:
```rust
async fn cleanup(&mut self) -> Result<()> {
    // Close file handles
    if let Some(file) = self.output_file.take() {
        file.close().await.ok(); // Ignore errors
    }
    
    // Deallocate large buffers
    self.buffer = Vec::new();
    
    Ok(())
}
```

---

## Data Marshaling

### Input Format

**Supported Types**:
- `null` → Rust `()` or skip
- `bool` → Rust `bool`
- `number` → Rust `f64`, `i64`, `u64`
- `string` → Rust `String`
- `array` → Rust `Vec<T>`
- `object` → Rust `HashMap<String, Value>` or struct
- **numpy array** → Rust `&[T]` (zero-copy via rust-numpy)

**Example**:
```json
{
  "audio": {
    "__numpy__": true,
    "data": [0.1, 0.2, 0.3, ...],
    "dtype": "float32",
    "shape": [16000]
  },
  "sample_rate": 16000
}
```

### Output Format

**Same as input** + structured results

**Example**:
```json
{
  "segments": [
    {"start_ms": 0, "end_ms": 500, "energy_db": -25.3},
    {"start_ms": 1000, "end_ms": 2000, "energy_db": -22.1}
  ]
}
```

---

## Error Handling

### Errors to Return

| Scenario | Error Type | Example |
|----------|------------|---------|
| **Invalid parameter** | `anyhow!("...")` | `anyhow!("threshold must be in [-60, 0]")` |
| **Missing data** | `anyhow!("...")` | `anyhow!("Missing audio data in input")` |
| **Computation failure** | `anyhow!("...")` | `anyhow!("FFT computation failed")` |
| **Resource error** | `anyhow!("...")` | `anyhow!("Out of memory")` |

### Errors to NOT Return

- Don't panic (use `Result` instead)
- Don't return generic errors (add context)
- Don't swallow errors (propagate with context)

**Good**:
```rust
let audio = extract_audio(&input)
    .context("Failed to extract audio from input")?;
```

**Bad**:
```rust
let audio = extract_audio(&input)?;  // No context!
```

---

## Performance Requirements

| Operation | Target | Notes |
|-----------|--------|-------|
| **initialize()** | <10ms | One-time cost |
| **execute()** | Depends on algorithm | Should be O(n) or better |
| **cleanup()** | <5ms | Minimal overhead |
| **Memory overhead** | <10MB | Per node instance |

---

## Example Implementations

### Rust Native Node

```rust
pub struct VADNode {
    threshold: f32,
    frame_length_ms: u32,
    sample_rate: u32,
}

#[async_trait]
impl NodeExecutor for VADNode {
    async fn initialize(&mut self, params: &Value) -> Result<()> {
        self.threshold = params.get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(-30.0) as f32;
        self.frame_length_ms = params.get("frame_length_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;
        self.sample_rate = params.get("sample_rate")
            .and_then(|v| v.as_u64())
            .unwrap_or(16000) as u32;
        Ok(())
    }
    
    async fn execute(&self, input: Value) -> Result<Value> {
        let audio = extract_audio_f32(&input)?;
        let segments = self.process(&audio)?;
        Ok(serde_json::to_value(segments)?)
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        Ok(()) // No resources to clean
    }
    
    fn metadata(&self) -> NodeMetadata {
        NodeMetadata {
            name: "VADNode".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec![Capability::Audio],
            parameters: ParameterSchema {
                required: vec!["sample_rate".to_string()],
                optional: vec![
                    ("threshold".to_string(), json!(-30.0)),
                    ("frame_length_ms".to_string(), json!(30)),
                ],
            },
        }
    }
}
```

### CPython Wrapper Node

```rust
pub struct CPythonNodeExecutor {
    node_instance: Py<PyAny>,
}

#[async_trait]
impl NodeExecutor for CPythonNodeExecutor {
    async fn initialize(&mut self, params: &Value) -> Result<()> {
        Python::with_gil(|py| {
            // Call Python node.__init__(**params)
            let kwargs = pythonize(py, params)?;
            self.node_instance.call_method(py, "__init__", (), Some(kwargs))?;
            Ok(())
        })
    }
    
    async fn execute(&self, input: Value) -> Result<Value> {
        Python::with_gil(|py| {
            // Convert input to Python
            let py_input = json_to_python(py, &input)?;
            
            // Call node.process(input)
            let result = self.node_instance.call_method1(py, "process", (py_input,))?;
            
            // Convert result back to Rust
            python_to_json(py, result)
        })
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        Python::with_gil(|py| {
            self.node_instance.call_method0(py, "cleanup").ok();
            Ok(())
        })
    }
    
    fn metadata(&self) -> NodeMetadata {
        Python::with_gil(|py| {
            let name = self.node_instance
                .getattr(py, "__class__")?
                .getattr(py, "__name__")?
                .extract::<String>(py)?;
            
            NodeMetadata {
                name,
                version: "1.0.0".to_string(),
                capabilities: vec![Capability::CPU],
                parameters: ParameterSchema::default(),
            }
        })
    }
}
```

---

## Testing Contract

### Unit Tests Required

Each node implementation MUST have:

1. **Initialization test**
   - Valid parameters → success
   - Invalid parameters → error
   - Missing parameters → error (if required)

2. **Execution test**
   - Valid input → expected output
   - Invalid input → error
   - Edge cases (empty audio, single sample, etc.)

3. **Idempotency test**
   - Multiple `execute()` calls produce same result
   - `initialize()` can be called multiple times

4. **Cleanup test**
   - Resources released after `cleanup()`
   - No memory leaks (valgrind)

### Example Test

```rust
#[tokio::test]
async fn test_vad_node_execution() {
    let mut node = VADNode::default();
    
    // Initialize
    let params = json!({
        "threshold": -30.0,
        "frame_length_ms": 30,
        "sample_rate": 16000
    });
    node.initialize(&params).await.unwrap();
    
    // Execute
    let audio = vec![0.1; 16000]; // 1 second of audio
    let input = json!({"audio": audio, "sample_rate": 16000});
    let output = node.execute(input).await.unwrap();
    
    // Verify
    let segments = output["segments"].as_array().unwrap();
    assert!(!segments.is_empty());
    
    // Cleanup
    node.cleanup().await.unwrap();
}
```

---

## Versioning

**Node versions follow semantic versioning**:
- **MAJOR**: Breaking changes (parameter schema changes, output format changes)
- **MINOR**: New features (new parameters, new output fields)
- **PATCH**: Bug fixes, performance improvements

**Version compatibility**:
- Executor MUST support all MINOR versions within same MAJOR
- Executor MAY warn on deprecated parameters
- Executor MUST error on unsupported MAJOR version

---

## Registration

Nodes are registered in `runtime/src/nodes/registry.rs`:

```rust
pub fn create_executor(node_type: &str) -> Result<Box<dyn NodeExecutor>> {
    match node_type {
        "VADNode" => Ok(Box::new(VADNode::default())),
        "ResampleNode" => Ok(Box::new(ResampleNode::default())),
        "FormatConverterNode" => Ok(Box::new(FormatConverterNode::default())),
        _ => {
            // Try CPython fallback
            create_cpython_executor(node_type)
        }
    }
}
```

---

## Best Practices

1. ✅ **Use `anyhow` for rich error context**
2. ✅ **Release GIL for CPU-intensive work** (automatic in Rust native)
3. ✅ **Validate parameters in `initialize()`**
4. ✅ **Use zero-copy for audio buffers** (rust-numpy)
5. ✅ **Document parameter schema** in `metadata()`
6. ✅ **Write comprehensive tests** (happy path + errors)
7. ✅ **Profile performance** (criterion benchmarks)

---

**Next**: See `ffi-api.md` for Python integration
