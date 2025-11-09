# NodeExecutor API Contract

**Feature**: Native Rust Acceleration  
**Date**: October 27, 2025  
**Version**: 1.0

## Overview

This document defines the `NodeExecutor` trait that all pipeline nodes must implement. This trait provides a unified interface for executing nodes regardless of runtime (Rust native or CPython fallback).

---

## 1. Core Trait Definition

### 1.1 `NodeExecutor` Trait

**Purpose**: Unified execution interface for all node types.

**Signature**:
```rust
use async_trait::async_trait;
use std::collections::HashMap;
use serde_json::Value;

#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Execute the node with given inputs, return outputs
    async fn execute(&self, inputs: NodeInputs) -> Result<NodeOutputs, ExecutorError>;
    
    /// Get the node type name (e.g., "AudioResampleNode")
    fn node_type(&self) -> &str;
    
    /// Get the runtime this node uses (RustNative or CPython)
    fn runtime(&self) -> Runtime;
    
    /// Optional: Validate input schema before execution
    fn validate_inputs(&self, inputs: &NodeInputs) -> Result<(), ExecutorError> {
        Ok(())  // Default: no validation
    }
    
    /// Optional: Get expected input keys
    fn input_keys(&self) -> Vec<String> {
        vec![]  // Default: no requirements
    }
    
    /// Optional: Get output keys this node produces
    fn output_keys(&self) -> Vec<String> {
        vec![]  // Default: unknown
    }
}
```

**Thread Safety Requirements**:
- `Send`: Can be transferred across thread boundaries
- `Sync`: Can be shared between threads (immutable access)
- Enables async concurrent execution via tokio

---

## 2. Data Types

### 2.1 `NodeInputs`

**Purpose**: Container for input data passed to node.

**Definition**:
```rust
#[derive(Debug, Clone)]
pub struct NodeInputs {
    pub data: HashMap<String, Value>,
}

impl NodeInputs {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
    
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        self.data.insert(key.into(), value);
    }
    
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }
    
    pub fn get_audio_buffer(&self, key: &str) -> Result<AudioBuffer, ExecutorError> {
        let value = self.get(key)
            .ok_or_else(|| ExecutorError::NodeExecutionError {
                node_id: "unknown".to_string(),
                source: anyhow::anyhow!("Missing input key: {}", key),
            })?;
        
        serde_json::from_value(value.clone())
            .map_err(|e| ExecutorError::NodeExecutionError {
                node_id: "unknown".to_string(),
                source: anyhow::anyhow!("Failed to deserialize AudioBuffer: {}", e),
            })
    }
}
```

**Example**:
```rust
let mut inputs = NodeInputs::new();
inputs.insert("input", serde_json::to_value(audio_buffer)?);
inputs.insert("threshold", serde_json::json!(-30.0));
```

---

### 2.2 `NodeOutputs`

**Purpose**: Container for output data produced by node.

**Definition**:
```rust
#[derive(Debug, Clone)]
pub struct NodeOutputs {
    pub data: HashMap<String, Value>,
}

impl NodeOutputs {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
    
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        self.data.insert(key.into(), value);
    }
    
    pub fn from_audio_buffer(key: impl Into<String>, buffer: AudioBuffer) -> Result<Self, ExecutorError> {
        let mut outputs = Self::new();
        outputs.insert(key, serde_json::to_value(buffer)?);
        Ok(outputs)
    }
}
```

**Example**:
```rust
let mut outputs = NodeOutputs::new();
outputs.insert("output", serde_json::to_value(processed_audio)?);
outputs.insert("segments", serde_json::json!(vad_segments));
```

---

## 3. Example Implementations

### 3.1 Rust Native Node (AudioResampleNode)

```rust
pub struct RustResampleNode {
    input_rate: u32,
    output_rate: u32,
    quality: ResampleQuality,
    resampler: Resampler,  // From rubato crate
}

impl RustResampleNode {
    pub fn new(input_rate: u32, output_rate: u32, quality: ResampleQuality) -> Result<Self, ExecutorError> {
        let resampler = Resampler::new(
            input_rate as f64,
            output_rate as f64,
            quality.into(),
        )?;
        
        Ok(Self {
            input_rate,
            output_rate,
            quality,
            resampler,
        })
    }
    
    fn resample(&self, audio: &AudioBuffer) -> Result<AudioBuffer, ExecutorError> {
        // Use rubato to resample
        let input_slice = audio.as_slice();
        let output_len = (input_slice.len() as f64 * self.output_rate as f64 / self.input_rate as f64) as usize;
        let mut output = vec![0.0f32; output_len];
        
        self.resampler.process(input_slice, &mut output)?;
        
        Ok(AudioBuffer::new(output, self.output_rate, audio.channels))
    }
}

#[async_trait]
impl NodeExecutor for RustResampleNode {
    async fn execute(&self, inputs: NodeInputs) -> Result<NodeOutputs, ExecutorError> {
        // Extract input audio buffer
        let audio = inputs.get_audio_buffer("input")?;
        
        // Validate sample rate
        if audio.sample_rate != self.input_rate {
            return Err(ExecutorError::NodeExecutionError {
                node_id: "resample".to_string(),
                source: anyhow::anyhow!(
                    "Input sample rate {} doesn't match expected {}",
                    audio.sample_rate,
                    self.input_rate
                ),
            });
        }
        
        // Perform resampling (CPU-bound, release GIL if called from Python)
        let resampled = self.resample(&audio)?;
        
        // Return output
        NodeOutputs::from_audio_buffer("output", resampled)
    }
    
    fn node_type(&self) -> &str {
        "AudioResampleNode"
    }
    
    fn runtime(&self) -> Runtime {
        Runtime::RustNative
    }
    
    fn input_keys(&self) -> Vec<String> {
        vec!["input".to_string()]
    }
    
    fn output_keys(&self) -> Vec<String> {
        vec!["output".to_string()]
    }
}
```

---

### 3.2 CPython Fallback Node

```rust
pub struct CPythonNode {
    node_type: String,
    python_class: String,
    params: Value,
}

#[async_trait]
impl NodeExecutor for CPythonNode {
    async fn execute(&self, inputs: NodeInputs) -> Result<NodeOutputs, ExecutorError> {
        Python::with_gil(|py| {
            // Import Python module
            let module = py.import("remotemedia.nodes.audio")?;
            
            // Instantiate Python node class
            let node_class = module.getattr(&self.python_class)?;
            let node_instance = node_class.call1((&self.params,))?;
            
            // Convert inputs to Python dict
            let py_inputs = inputs_to_pydict(py, &inputs)?;
            
            // Call Python execute method
            let py_outputs = node_instance.call_method1("execute", (py_inputs,))?;
            
            // Convert Python outputs back to Rust
            pydict_to_outputs(py, py_outputs)
        })
        .map_err(|e| ExecutorError::PythonError(e.to_string()))
    }
    
    fn node_type(&self) -> &str {
        &self.node_type
    }
    
    fn runtime(&self) -> Runtime {
        Runtime::CPython
    }
}
```

---

## 4. Node Registration

### 4.1 Node Registry

**Purpose**: Factory for creating node instances based on manifest.

**Definition**:
```rust
pub struct NodeRegistry {
    factories: HashMap<String, Box<dyn NodeFactory>>,
}

pub trait NodeFactory: Send + Sync {
    fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>, ExecutorError>;
    fn runtime(&self) -> Runtime;
}

impl NodeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };
        
        // Register Rust nodes
        registry.register("AudioResampleNode", Box::new(ResampleNodeFactory));
        registry.register("VADNode", Box::new(VADNodeFactory));
        registry.register("FormatConverterNode", Box::new(FormatConverterNodeFactory));
        
        registry
    }
    
    pub fn register(&mut self, node_type: &str, factory: Box<dyn NodeFactory>) {
        self.factories.insert(node_type.to_string(), factory);
    }
    
    pub fn create_node(
        &self,
        node_type: &str,
        params: Value,
        runtime_hint: RuntimeHint,
    ) -> Result<Box<dyn NodeExecutor>, ExecutorError> {
        // Try Rust native first if auto or rust hint
        if matches!(runtime_hint, RuntimeHint::Auto | RuntimeHint::Rust) {
            if let Some(factory) = self.factories.get(node_type) {
                if factory.runtime() == Runtime::RustNative {
                    return factory.create(params);
                }
            }
        }
        
        // Fallback to CPython
        if matches!(runtime_hint, RuntimeHint::Auto | RuntimeHint::Python) {
            return Ok(Box::new(CPythonNode::new(node_type, params)));
        }
        
        Err(ExecutorError::NodeNotFound {
            node_id: node_type.to_string(),
        })
    }
}
```

---

### 4.2 Node Factory Example

```rust
struct ResampleNodeFactory;

impl NodeFactory for ResampleNodeFactory {
    fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>, ExecutorError> {
        let input_rate: u32 = params["input_rate"].as_u64().unwrap() as u32;
        let output_rate: u32 = params["output_rate"].as_u64().unwrap() as u32;
        let quality = params.get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("high");
        
        let node = RustResampleNode::new(
            input_rate,
            output_rate,
            ResampleQuality::from_str(quality)?,
        )?;
        
        Ok(Box::new(node))
    }
    
    fn runtime(&self) -> Runtime {
        Runtime::RustNative
    }
}
```

---

## 5. Execution Flow

### 5.1 Pipeline Executor

```rust
pub struct Executor {
    graph: PipelineGraph,
    metrics: MetricsCollector,
    retry_policy: RetryPolicy,
}

impl Executor {
    pub async fn execute(&mut self) -> Result<NodeOutputs, ExecutorError> {
        let mut node_outputs: HashMap<String, NodeOutputs> = HashMap::new();
        
        // Execute nodes in topological order
        for node_id in &self.graph.execution_order {
            let node = self.graph.nodes.get(node_id).unwrap();
            
            // Gather inputs from upstream nodes
            let inputs = self.gather_inputs(node_id, &node_outputs)?;
            
            // Execute with retry policy
            self.metrics.start(node_id);
            let result = self.execute_node_with_retry(node, inputs).await?;
            self.metrics.record(node_id, &result);
            
            // Store outputs for downstream nodes
            node_outputs.insert(node_id.clone(), result);
        }
        
        // Return final outputs (from sink nodes)
        self.get_final_outputs(&node_outputs)
    }
    
    async fn execute_node_with_retry(
        &self,
        node: &NodeInstance,
        inputs: NodeInputs,
    ) -> Result<NodeOutputs, ExecutorError> {
        let mut attempts = 0;
        
        loop {
            match node.executor.execute(inputs.clone()).await {
                Ok(outputs) => return Ok(outputs),
                Err(e) if e.is_retryable() && attempts < self.retry_policy.max_attempts => {
                    attempts += 1;
                    let delay = self.retry_policy.get_delay(attempts);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    fn gather_inputs(
        &self,
        node_id: &str,
        node_outputs: &HashMap<String, NodeOutputs>,
    ) -> Result<NodeInputs, ExecutorError> {
        let mut inputs = NodeInputs::new();
        
        // Find edges pointing to this node
        for edge in &self.graph.edges {
            if edge.to == node_id {
                let upstream_outputs = node_outputs.get(&edge.from)
                    .ok_or_else(|| ExecutorError::NodeNotFound {
                        node_id: edge.from.clone(),
                    })?;
                
                let value = upstream_outputs.data.get(&edge.output_key)
                    .ok_or_else(|| ExecutorError::NodeExecutionError {
                        node_id: edge.from.clone(),
                        source: anyhow::anyhow!("Missing output key: {}", edge.output_key),
                    })?;
                
                inputs.insert(edge.input_key.clone(), value.clone());
            }
        }
        
        Ok(inputs)
    }
}
```

---

## 6. Error Handling

### 6.1 Error Propagation

**Pattern**: Errors bubble up with context
```rust
async fn execute_pipeline() -> Result<NodeOutputs, ExecutorError> {
    let node = create_node()?;  // May fail with NodeNotFound
    
    let outputs = node.execute(inputs).await
        .map_err(|e| ExecutorError::NodeExecutionError {
            node_id: "resample-1".to_string(),
            source: anyhow::anyhow!("Execution failed: {}", e),
        })?;
    
    Ok(outputs)
}
```

### 6.2 Retry Logic

**Pattern**: Exponential backoff for transient errors
```rust
impl ExecutorError {
    pub fn is_retryable(&self) -> bool {
        match self {
            ExecutorError::NodeExecutionError { .. } => true,  // May be transient
            ExecutorError::PythonError(_) => true,             // Python errors may be transient
            ExecutorError::ManifestError(_) => false,          // Parse errors are permanent
            ExecutorError::CycleError { .. } => false,         // Graph errors are permanent
            _ => false,
        }
    }
}
```

---

## 7. Performance Considerations

### 7.1 Async Execution

**Pattern**: Concurrent execution of independent nodes
```rust
// If nodes A and B have no dependency, execute in parallel
let (result_a, result_b) = tokio::join!(
    executor_a.execute(inputs_a),
    executor_b.execute(inputs_b),
);
```

### 7.2 Zero-Copy Data Flow

**Pattern**: Pass references, not copies
```rust
// GOOD: Pass reference to AudioBuffer (Arc<Vec<f32>> is cheap to clone)
let buffer = AudioBuffer::new(data, 16000, 1);
node_a.execute(inputs_with_buffer(buffer.clone()))?;  // Clone Arc, not data
node_b.execute(inputs_with_buffer(buffer))?;          // Reuse Arc

// BAD: Clone entire Vec
let buffer_copy = buffer.data.to_vec();  // ❌ Expensive copy
```

---

## 8. Testing Contract

### 8.1 Unit Tests

```rust
#[tokio::test]
async fn test_resample_node() {
    let node = RustResampleNode::new(48000, 16000, ResampleQuality::High).unwrap();
    
    let audio = AudioBuffer::new(vec![0.0; 48000], 48000, 1);
    let mut inputs = NodeInputs::new();
    inputs.insert("input", serde_json::to_value(audio).unwrap());
    
    let outputs = node.execute(inputs).await.unwrap();
    
    let output_audio: AudioBuffer = serde_json::from_value(
        outputs.data["output"].clone()
    ).unwrap();
    
    assert_eq!(output_audio.sample_rate, 16000);
    assert_eq!(output_audio.len_samples(), 16000);  // 1/3 of input
}
```

### 8.2 Integration Tests

```rust
#[tokio::test]
async fn test_pipeline_execution() {
    let mut executor = Executor::new(manifest)?;
    
    let start = Instant::now();
    let outputs = executor.execute().await.unwrap();
    let elapsed = start.elapsed();
    
    assert!(elapsed < Duration::from_millis(100));  // Performance target
    assert!(outputs.data.contains_key("output"));
}
```

---

## 9. Extension Points

### 9.1 Custom Node Types

**Add new Rust node**:
```rust
// 1. Implement NodeExecutor trait
struct CustomNode { /* ... */ }

#[async_trait]
impl NodeExecutor for CustomNode {
    async fn execute(&self, inputs: NodeInputs) -> Result<NodeOutputs, ExecutorError> {
        // Custom logic
    }
    
    fn node_type(&self) -> &str { "CustomNode" }
    fn runtime(&self) -> Runtime { Runtime::RustNative }
}

// 2. Create factory
struct CustomNodeFactory;
impl NodeFactory for CustomNodeFactory {
    fn create(&self, params: Value) -> Result<Box<dyn NodeExecutor>, ExecutorError> {
        Ok(Box::new(CustomNode::new(params)?))
    }
    fn runtime(&self) -> Runtime { Runtime::RustNative }
}

// 3. Register in registry
registry.register("CustomNode", Box::new(CustomNodeFactory));
```

---

## 10. Versioning

**API Version**: 1.0  
**Stability**: Stable (no breaking changes planned)

**Future Additions** (non-breaking):
- `async fn validate_inputs()` → Pre-execution validation
- `async fn prepare()` → Resource initialization hook
- `async fn cleanup()` → Resource cleanup hook

---

## Next Steps

1. ✅ NodeExecutor contract defined
2. ⏳ Implement core nodes (VAD, Resample, Format)
3. ⏳ Implement CPython fallback node
4. ⏳ Add node registry with factory pattern
5. ⏳ Write unit tests for each node
6. ⏳ Write integration tests for pipeline execution
