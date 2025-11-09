# Data Model: Native Rust Acceleration

**Feature**: Native Rust Acceleration for AI/ML Pipelines  
**Date**: October 27, 2025  
**Status**: Draft

## Summary

This document defines the core data structures for the Rust pipeline executor. These models enable manifest parsing, graph construction, execution orchestration, error handling, and performance monitoring.

---

## 1. PipelineManifest

**Purpose**: JSON representation of pipeline with nodes, edges, and configuration.

**Schema**:
```json
{
  "version": "1.0",
  "nodes": [
    {
      "id": "resample-1",
      "type": "AudioResampleNode",
      "params": {
        "input_rate": 48000,
        "output_rate": 16000,
        "quality": "high"
      },
      "runtime_hint": "auto"
    }
  ],
  "edges": [
    {
      "from": "source-1",
      "to": "resample-1",
      "output_key": "audio",
      "input_key": "input"
    }
  ],
  "config": {
    "enable_metrics": true,
    "retry_policy": "exponential",
    "max_retries": 3
  }
}
```

**Rust Representation**:
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineManifest {
    pub version: String,
    pub nodes: Vec<NodeManifest>,
    pub edges: Vec<Edge>,
    pub config: PipelineConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeManifest {
    pub id: String,
    pub node_type: String,  // "AudioResampleNode", "VADNode", etc.
    pub params: serde_json::Value,  // Node-specific parameters
    pub runtime_hint: RuntimeHint,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeHint {
    Auto,      // Prefer Rust, fallback to Python
    Rust,      // Rust only (error if unavailable)
    Python,    // Python only
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,  // Source node ID
    pub to: String,    // Target node ID
    pub output_key: String,
    pub input_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub enable_metrics: bool,
    pub retry_policy: RetryPolicyType,
    pub max_retries: u32,
    pub circuit_breaker_threshold: u32,  // Default: 5
}
```

**Validation Rules**:
- `version` MUST be semver format (e.g., "1.0", "1.2.3")
- All `node_id` values MUST be unique within manifest
- All edge `from`/`to` references MUST point to existing node IDs
- No cycles allowed (validated during graph construction)

---

## 2. PipelineGraph

**Purpose**: Internal directed acyclic graph structure with nodes as vertices and dependencies as edges. Supports topological sort, cycle detection, and async execution scheduling.

**Rust Representation**:
```rust
pub struct PipelineGraph {
    pub nodes: HashMap<String, NodeInstance>,
    pub edges: Vec<Edge>,
    pub adjacency_list: HashMap<String, Vec<String>>,  // node_id -> [dependent_ids]
    pub execution_order: Vec<String>,  // Topologically sorted node IDs
}

pub struct NodeInstance {
    pub id: String,
    pub node_type: String,
    pub params: serde_json::Value,
    pub runtime: Runtime,  // Selected runtime after hint resolution
    pub executor: Box<dyn NodeExecutor>,  // Trait object for execution
}

#[derive(Debug, Clone)]
pub enum Runtime {
    RustNative,
    CPython,
}

// Topological sort algorithm
impl PipelineGraph {
    pub fn from_manifest(manifest: &PipelineManifest) -> Result<Self, ExecutorError> {
        // 1. Create nodes map
        // 2. Build adjacency list from edges
        // 3. Perform topological sort (Kahn's algorithm)
        // 4. Detect cycles (error if found)
    }
    
    pub fn topological_sort(&self) -> Result<Vec<String>, ExecutorError> {
        // Kahn's algorithm:
        // 1. Find nodes with in-degree 0 (no dependencies)
        // 2. Process node, remove edges
        // 3. Repeat until all nodes processed or cycle detected
    }
}
```

**Adjacency List Example**:
```text
Pipeline: A → B → C
           ↓     ↓
           D → E

Adjacency List:
A: []        (no dependencies)
B: [A]       (depends on A)
C: [B]       (depends on B)
D: [A]       (depends on A)
E: [D, C]    (depends on both D and C)

Execution Order (topological sort):
[A, B, D, C, E]  or  [A, D, B, C, E]  (both valid)
```

---

## 3. ExecutionMetrics

**Purpose**: Performance data for pipeline run. Contains total execution time, per-node timing data, memory usage, timestamps, and execution order.

**JSON Schema**:
```json
{
  "pipeline_id": "audio-preprocessing-123",
  "start_time": "2025-10-27T10:30:00.123Z",
  "end_time": "2025-10-27T10:30:01.456Z",
  "total_time_us": 1333000,
  "status": "success",
  "nodes": [
    {
      "id": "resample-1",
      "type": "AudioResampleNode",
      "runtime": "rust",
      "start_time_us": 0,
      "execution_time_us": 1200000,
      "memory_peak_mb": 45,
      "status": "success"
    },
    {
      "id": "vad-1",
      "type": "VADNode",
      "runtime": "rust",
      "start_time_us": 1200500,
      "execution_time_us": 50000,
      "memory_peak_mb": 2,
      "status": "success"
    }
  ]
}
```

**Rust Representation**:
```rust
#[derive(Debug, Serialize)]
pub struct ExecutionMetrics {
    pub pipeline_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub total_time_us: u64,
    pub status: ExecutionStatus,
    pub nodes: Vec<NodeMetrics>,
}

#[derive(Debug, Serialize)]
pub struct NodeMetrics {
    pub id: String,
    pub node_type: String,
    pub runtime: Runtime,
    pub start_time_us: u64,  // Relative to pipeline start
    pub execution_time_us: u64,
    pub memory_peak_mb: f64,
    pub status: NodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Success,
    PartialFailure,  // Some nodes failed
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Success,
    Failed,
    Skipped,  // Due to circuit breaker
    Retried,  // Succeeded after retry
}
```

**Collection Strategy**:
```rust
impl MetricsCollector {
    pub fn start(&mut self, node_id: &str) {
        let start = Instant::now();
        self.node_starts.insert(node_id.to_string(), start);
    }
    
    pub fn record(&mut self, node_id: &str, result: &NodeResult) {
        let start = self.node_starts.get(node_id).unwrap();
        let elapsed = start.elapsed();
        let memory = get_process_memory();  // OS-specific API
        
        self.metrics.nodes.push(NodeMetrics {
            id: node_id.to_string(),
            execution_time_us: elapsed.as_micros() as u64,
            memory_peak_mb: memory,
            // ...
        });
    }
}
```

---

## 4. Error Types

**Purpose**: Comprehensive error handling hierarchy for manifest parsing, graph validation, execution failures, and FFI boundary errors.

**Rust Representation**:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Manifest parsing failed: {0}")]
    ManifestError(String),
    
    #[error("Invalid manifest version: expected {expected}, got {actual}")]
    VersionMismatch {
        expected: String,
        actual: String,
    },
    
    #[error("Graph validation failed: {0}")]
    GraphError(String),
    
    #[error("Cycle detected in pipeline: {path}")]
    CycleError {
        path: String,  // "A → B → C → A"
    },
    
    #[error("Node not found: {node_id}")]
    NodeNotFound {
        node_id: String,
    },
    
    #[error("Node execution failed: {node_id}")]
    NodeExecutionError {
        node_id: String,
        #[source]
        source: anyhow::Error,
    },
    
    #[error("Python error: {0}")]
    PythonError(String),
    
    #[error("FFI error: {0}")]
    FfiError(String),
    
    #[error("Retry limit exceeded: {node_id} ({attempts} attempts)")]
    RetryLimitExceeded {
        node_id: String,
        attempts: u32,
    },
    
    #[error("Circuit breaker tripped: {node_id}")]
    CircuitBreakerTripped {
        node_id: String,
    },
}

impl ExecutorError {
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            ExecutorError::NodeExecutionError { .. } |
            ExecutorError::PythonError(_)
        )
    }
    
    pub fn to_python_error(&self) -> PyErr {
        // Convert to PyO3 PyErr for FFI boundary
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(self.to_string())
    }
}
```

**Error Context Pattern**:
```rust
use anyhow::Context;

fn execute_node(node_id: &str) -> Result<Value, ExecutorError> {
    let result = node.execute()
        .with_context(|| format!("Failed to execute node: {}", node_id))
        .map_err(|e| ExecutorError::NodeExecutionError {
            node_id: node_id.to_string(),
            source: e,
        })?;
    Ok(result)
}
```

---

## 5. RetryPolicy

**Purpose**: Configuration for error handling. Includes maximum retry attempts, backoff strategy, and rules for classifying errors as transient or permanent.

**Rust Representation**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,  // Default: 3
    pub backoff: BackoffStrategy,
    pub circuit_breaker_threshold: u32,  // Default: 5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackoffStrategy {
    Exponential {
        initial_delay_ms: u64,  // Default: 100
        multiplier: f64,        // Default: 2.0
    },
    Fixed {
        delay_ms: u64,
    },
    None,
}

impl RetryPolicy {
    pub fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential {
                initial_delay_ms: 100,
                multiplier: 2.0,
            },
            circuit_breaker_threshold: 5,
        }
    }
    
    pub fn get_delay(&self, attempt: u32) -> Duration {
        match &self.backoff {
            BackoffStrategy::Exponential { initial_delay_ms, multiplier } => {
                let delay = *initial_delay_ms as f64 * multiplier.powi(attempt as i32);
                Duration::from_millis(delay as u64)
            }
            BackoffStrategy::Fixed { delay_ms } => {
                Duration::from_millis(*delay_ms)
            }
            BackoffStrategy::None => Duration::from_millis(0),
        }
    }
    
    pub async fn execute<F, T, E>(&self, mut op: F) -> Result<T, E>
    where
        F: FnMut() -> Result<T, E>,
        E: std::fmt::Display,
    {
        for attempt in 0..self.max_attempts {
            match op() {
                Ok(result) => return Ok(result),
                Err(e) if attempt + 1 < self.max_attempts => {
                    let delay = self.get_delay(attempt);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }
}
```

**Example Usage**:
```rust
let policy = RetryPolicy::default();
let result = policy.execute(|| {
    fetch_remote_file(url)
}).await?;

// Delays: 100ms, 200ms, 400ms (total 3 attempts)
```

---

## 6. AudioBuffer

**Purpose**: Container for audio data flowing between nodes. Attributes include sample rate, format (floating-point or integer samples), number of channels, length in samples. Supports efficient sharing between operations to minimize copying.

**Rust Representation**:
```rust
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub data: Arc<Vec<f32>>,  // Shared ownership (avoid copies)
    pub sample_rate: u32,     // Hz (e.g., 16000, 48000)
    pub channels: u16,        // 1 = mono, 2 = stereo
    pub format: AudioFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    F32,   // 32-bit float [-1.0, 1.0]
    I16,   // 16-bit signed integer [-32768, 32767]
    I32,   // 32-bit signed integer
}

impl AudioBuffer {
    pub fn new(data: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            data: Arc::new(data),
            sample_rate,
            channels,
            format: AudioFormat::F32,
        }
    }
    
    pub fn len_samples(&self) -> usize {
        self.data.len()
    }
    
    pub fn len_frames(&self) -> usize {
        self.data.len() / self.channels as usize
    }
    
    pub fn duration_secs(&self) -> f64 {
        self.len_frames() as f64 / self.sample_rate as f64
    }
    
    // Zero-copy view (borrow from Arc)
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }
    
    // Clone only if needed (Arc is cheap)
    pub fn make_mut(&mut self) -> &mut Vec<f32> {
        Arc::make_mut(&mut self.data)
    }
}
```

**Memory Efficiency**:
- `Arc<Vec<f32>>`: Shared ownership, cheap to clone (only increments refcount)
- `as_slice()`: Zero-copy access for read-only operations
- `make_mut()`: Copy-on-write if multiple refs exist

---

## 7. NodeExecutor Trait

**Purpose**: Unified interface for executing nodes regardless of runtime (Rust native or CPython).

**Rust Representation**:
```rust
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    async fn execute(&self, inputs: NodeInputs) -> Result<NodeOutputs, ExecutorError>;
    
    fn node_type(&self) -> &str;
    
    fn runtime(&self) -> Runtime;
}

pub struct NodeInputs {
    pub data: HashMap<String, serde_json::Value>,
}

pub struct NodeOutputs {
    pub data: HashMap<String, serde_json::Value>,
}

// Example Rust node
pub struct RustResampleNode {
    input_rate: u32,
    output_rate: u32,
    quality: ResampleQuality,
}

#[async_trait]
impl NodeExecutor for RustResampleNode {
    async fn execute(&self, inputs: NodeInputs) -> Result<NodeOutputs, ExecutorError> {
        let audio: AudioBuffer = serde_json::from_value(inputs.data["input"].clone())?;
        
        // Resample using rubato
        let resampled = self.resample(&audio)?;
        
        let mut outputs = HashMap::new();
        outputs.insert("output".to_string(), serde_json::to_value(resampled)?);
        Ok(NodeOutputs { data: outputs })
    }
    
    fn node_type(&self) -> &str {
        "AudioResampleNode"
    }
    
    fn runtime(&self) -> Runtime {
        Runtime::RustNative
    }
}
```

---

## Data Flow Diagram

```text
Python User Code
       ↓
  pipeline.run(audio_data)
       ↓
  Serialize to PipelineManifest (JSON)
       ↓
  FFI: execute_pipeline_ffi(manifest_json)
       ↓
  Parse PipelineManifest
       ↓
  Build PipelineGraph
       ↓
  Topological Sort (execution_order)
       ↓
  For each node in execution_order:
    ├─ Create NodeInstance
    ├─ Resolve runtime (RuntimeHint → Runtime)
    ├─ Execute via NodeExecutor trait
    ├─ Collect metrics (start_time, elapsed, memory)
    └─ Pass outputs to dependent nodes
       ↓
  Return ExecutionMetrics (JSON)
       ↓
  FFI boundary (Rust → Python)
       ↓
  Python receives metrics
```

---

## Relationships

```text
PipelineManifest
  ├─ contains → NodeManifest[]
  ├─ contains → Edge[]
  └─ contains → PipelineConfig

PipelineGraph
  ├─ built from → PipelineManifest
  ├─ contains → NodeInstance[]
  └─ contains → execution_order (topologically sorted)

NodeInstance
  ├─ implements → NodeExecutor trait
  ├─ has → Runtime (resolved from RuntimeHint)
  └─ produces → NodeOutputs

ExecutionMetrics
  ├─ tracks → NodeMetrics[]
  └─ exported as → JSON

RetryPolicy
  ├─ configured in → PipelineConfig
  └─ applied during → NodeExecutor::execute()

AudioBuffer
  ├─ passed between → NodeExecutor instances
  └─ shared via → Arc<Vec<f32>> (zero-copy)
```

---

## Validation Rules Summary

1. **Manifest Validation**:
   - Unique node IDs
   - Valid edge references
   - Supported node types
   - Valid parameter types

2. **Graph Validation**:
   - No cycles (DAG required)
   - All dependencies resolvable
   - At least one source node (in-degree 0)
   - At least one sink node (out-degree 0)

3. **Runtime Validation**:
   - Node type has implementation for selected runtime
   - Input data types match node expectations
   - Output data types match edge connections

4. **Performance Constraints**:
   - Metrics collection overhead: <100μs
   - FFI call overhead: <1μs
   - Memory tracking accuracy: ±5%

---

## Next Steps

1. ✅ Data model defined
2. ⏳ Create API contracts (FFI boundary, NodeExecutor trait)
3. ⏳ Implement core structures in `runtime/src/`
4. ⏳ Add validation logic
5. ⏳ Write unit tests for each structure
