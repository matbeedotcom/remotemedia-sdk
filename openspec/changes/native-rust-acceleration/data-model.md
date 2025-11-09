# Data Model: Native Rust Acceleration

**Date**: 2025-10-27  
**Phase**: 1 (Design)

## Overview

This document defines all data structures, schemas, and type definitions for the Rust runtime executor and audio processing nodes.

---

## 1. Pipeline Manifest Schema

### JSON Schema (v1)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "RemoteMedia Pipeline Manifest",
  "type": "object",
  "required": ["version", "nodes", "connections"],
  "properties": {
    "version": {
      "type": "string",
      "enum": ["v1"],
      "description": "Manifest schema version"
    },
    "metadata": {
      "type": "object",
      "properties": {
        "name": { "type": "string" },
        "description": { "type": "string" },
        "created_at": { "type": "string", "format": "date-time" }
      }
    },
    "nodes": {
      "type": "array",
      "items": { "$ref": "#/definitions/node" }
    },
    "connections": {
      "type": "array",
      "items": { "$ref": "#/definitions/connection" }
    }
  },
  "definitions": {
    "node": {
      "type": "object",
      "required": ["id", "node_type"],
      "properties": {
        "id": { "type": "string" },
        "node_type": { "type": "string" },
        "params": { "type": "object" },
        "runtime_hint": {
          "type": "string",
          "enum": ["rust_native", "cpython", "auto"]
        }
      }
    },
    "connection": {
      "type": "object",
      "required": ["from", "to"],
      "properties": {
        "from": { "type": "string" },
        "to": { "type": "string" }
      }
    }
  }
}
```

### Rust Types

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub metadata: ManifestMetadata,
    pub nodes: Vec<NodeDefinition>,
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: String,
    pub node_type: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub runtime_hint: RuntimeHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHint {
    RustNative,
    Cpython,
    Auto,
}

impl Default for RuntimeHint {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub from: String,
    pub to: String,
}
```

---

## 2. Pipeline Graph

### Graph Data Structure

```rust
use std::collections::{HashMap, HashSet};

pub struct PipelineGraph {
    nodes: HashMap<String, NodeInstance>,
    adjacency: HashMap<String, Vec<String>>,
    execution_order: Vec<String>,
}

pub struct NodeInstance {
    pub id: String,
    pub node_type: NodeType,
    pub params: serde_json::Value,
    pub executor: Box<dyn NodeExecutor>,
}

pub enum NodeType {
    RustNative(RustNodeKind),
    CPython(String),
}

pub enum RustNodeKind {
    VAD,
    Resample,
    FormatConverter,
    Multiply,
    Add,
}

impl PipelineGraph {
    pub fn from_manifest(manifest: &Manifest) -> Result<Self> {
        let mut graph = Self {
            nodes: HashMap::new(),
            adjacency: HashMap::new(),
            execution_order: Vec::new(),
        };
        
        // Build nodes
        for node_def in &manifest.nodes {
            let executor = Self::create_executor(node_def)?;
            graph.nodes.insert(node_def.id.clone(), NodeInstance {
                id: node_def.id.clone(),
                node_type: Self::infer_node_type(node_def),
                params: node_def.params.clone(),
                executor,
            });
        }
        
        // Build adjacency list
        for conn in &manifest.connections {
            graph.adjacency
                .entry(conn.from.clone())
                .or_default()
                .push(conn.to.clone());
        }
        
        // Topological sort
        graph.execution_order = graph.topological_sort()?;
        
        Ok(graph)
    }
    
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue = Vec::new();
        let mut order = Vec::new();
        
        // Calculate in-degrees
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }
        for neighbors in self.adjacency.values() {
            for neighbor in neighbors {
                *in_degree.get_mut(neighbor).unwrap() += 1;
            }
        }
        
        // Enqueue nodes with no dependencies
        for (node_id, &degree) in &in_degree {
            if degree == 0 {
                queue.push(node_id.clone());
            }
        }
        
        // Kahn's algorithm
        while let Some(node_id) = queue.pop() {
            order.push(node_id.clone());
            
            if let Some(neighbors) = self.adjacency.get(&node_id) {
                for neighbor in neighbors {
                    let degree = in_degree.get_mut(neighbor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push(neighbor.clone());
                    }
                }
            }
        }
        
        // Detect cycles
        if order.len() != self.nodes.len() {
            let remaining: Vec<String> = self.nodes.keys()
                .filter(|id| !order.contains(id))
                .cloned()
                .collect();
            return Err(ExecutorError::CycleError { nodes: remaining });
        }
        
        Ok(order)
    }
}
```

---

## 3. Node Executor Trait

### Trait Definition

```rust
use async_trait::async_trait;

#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Initialize the node with parameters
    async fn initialize(&mut self, params: &serde_json::Value) -> Result<()>;
    
    /// Execute the node on input data
    async fn execute(&self, input: Value) -> Result<Value>;
    
    /// Cleanup resources
    async fn cleanup(&mut self) -> Result<()>;
    
    /// Get node metadata
    fn metadata(&self) -> NodeMetadata;
}

pub struct NodeMetadata {
    pub node_type: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
}

pub enum Capability {
    Audio,
    Video,
    GPU,
    CPU,
}
```

### Rust Native Nodes

```rust
pub struct VADNodeExecutor {
    threshold: f32,
    frame_length_ms: u32,
    sample_rate: u32,
}

#[async_trait]
impl NodeExecutor for VADNodeExecutor {
    async fn initialize(&mut self, params: &Value) -> Result<()> {
        self.threshold = params.get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(-30.0) as f32;
        self.frame_length_ms = params.get("frame_length_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;
        Ok(())
    }
    
    async fn execute(&self, input: Value) -> Result<Value> {
        // Extract audio array from input
        let audio = extract_audio_array(&input)?;
        
        // Process
        let segments = self.detect_voice_activity(&audio)?;
        
        // Return as JSON
        Ok(serde_json::to_value(segments)?)
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        Ok(()) // No resources to cleanup
    }
    
    fn metadata(&self) -> NodeMetadata {
        NodeMetadata {
            node_type: "VADNode".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec![Capability::Audio],
        }
    }
}
```

---

## 4. Error Types

### Error Hierarchy

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Manifest parsing failed: {0}")]
    ManifestError(String),
    
    #[error("Invalid pipeline graph: {0}")]
    GraphError(String),
    
    #[error("Cycle detected in pipeline: {nodes:?}")]
    CycleError {
        nodes: Vec<String>,
    },
    
    #[error("Node '{node_id}' not found")]
    NodeNotFound {
        node_id: String,
    },
    
    #[error("Node execution failed: {node_id}")]
    NodeExecutionError {
        node_id: String,
        #[source]
        source: anyhow::Error,
    },
    
    #[error("Python exception in '{node_id}': {message}")]
    PythonError {
        node_id: String,
        message: String,
        traceback: Option<String>,
    },
    
    #[error("Retryable error (attempt {attempt}/{max_attempts}): {message}")]
    RetryableError {
        message: String,
        attempt: u32,
        max_attempts: u32,
        #[source]
        source: Option<anyhow::Error>,
    },
    
    #[error("Data marshaling failed: {0}")]
    MarshalingError(String),
    
    #[error("Invalid audio format: {0}")]
    AudioFormatError(String),
}

impl ExecutorError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, 
            ExecutorError::RetryableError { .. } |
            ExecutorError::PythonError { message, .. } if message.contains("timeout") ||
                                                         message.contains("rate limit") ||
                                                         message.contains("OOM")
        )
    }
    
    pub fn to_python_error(&self) -> pyo3::PyErr {
        use pyo3::exceptions::*;
        
        match self {
            ExecutorError::ManifestError(msg) => PyValueError::new_err(msg.clone()),
            ExecutorError::GraphError(msg) => PyValueError::new_err(msg.clone()),
            ExecutorError::CycleError { nodes } => {
                PyValueError::new_err(format!("Cycle detected: {:?}", nodes))
            }
            ExecutorError::NodeNotFound { node_id } => {
                PyKeyError::new_err(format!("Node not found: {}", node_id))
            }
            ExecutorError::NodeExecutionError { node_id, source } => {
                PyRuntimeError::new_err(format!("{}: {}", node_id, source))
            }
            ExecutorError::PythonError { message, traceback, .. } => {
                let msg = match traceback {
                    Some(tb) => format!("{}\n{}", message, tb),
                    None => message.clone(),
                };
                PyRuntimeError::new_err(msg)
            }
            _ => PyRuntimeError::new_err(self.to_string()),
        }
    }
}
```

---

## 5. Performance Metrics

### Metrics Schema

```rust
use serde::{Serialize, Deserialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineMetrics {
    pub total_time_ms: u64,
    pub total_memory_mb: f64,
    pub node_count: usize,
    pub nodes: Vec<NodeMetrics>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeMetrics {
    pub node_id: String,
    pub node_type: String,
    pub runtime: RuntimeType,
    pub execution_time_ms: u64,
    pub memory_mb: f64,
    pub status: ExecutionStatus,
    pub retry_count: u32,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeType {
    RustNative,
    Cpython,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Failed,
    Skipped,
}

impl PipelineMetrics {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
    
    pub fn summary(&self) -> String {
        format!(
            "Pipeline completed in {}ms ({} nodes, {:.2}MB peak memory)",
            self.total_time_ms,
            self.node_count,
            self.total_memory_mb
        )
    }
}
```

### Example JSON Output

```json
{
  "total_time_ms": 1234,
  "total_memory_mb": 256.5,
  "node_count": 3,
  "nodes": [
    {
      "node_id": "vad",
      "node_type": "VADNode",
      "runtime": "rust_native",
      "execution_time_ms": 5,
      "memory_mb": 10.2,
      "status": "success",
      "retry_count": 0,
      "error": null
    },
    {
      "node_id": "resample",
      "node_type": "ResampleNode",
      "runtime": "rust_native",
      "execution_time_ms": 8,
      "memory_mb": 15.3,
      "status": "success",
      "retry_count": 0,
      "error": null
    },
    {
      "node_id": "whisper",
      "node_type": "RustWhisperTranscriber",
      "runtime": "cpython",
      "execution_time_ms": 1221,
      "memory_mb": 231.0,
      "status": "success",
      "retry_count": 1,
      "error": null
    }
  ]
}
```

---

## 6. Audio Data Types

### Audio Buffer

```rust
pub struct AudioBuffer {
    pub data: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub format: AudioFormat,
}

#[derive(Debug, Clone, Copy)]
pub enum AudioFormat {
    F32,  // 32-bit float [-1.0, 1.0]
    I16,  // 16-bit signed integer [-32768, 32767]
    I32,  // 32-bit signed integer
    F64,  // 64-bit float
}

impl AudioBuffer {
    pub fn from_numpy(py: Python, array: &PyArrayDyn<f32>, sample_rate: u32) -> Result<Self> {
        let shape = array.shape();
        let channels = if shape.len() == 1 { 1 } else { shape[1] as u32 };
        
        // Zero-copy view
        let data = unsafe { array.as_slice()? }.to_vec();
        
        Ok(Self {
            data,
            sample_rate,
            channels,
            format: AudioFormat::F32,
        })
    }
    
    pub fn to_numpy(&self, py: Python) -> PyResult<Py<PyArrayDyn<f32>>> {
        let shape = if self.channels == 1 {
            vec![self.data.len()]
        } else {
            vec![self.data.len() / self.channels as usize, self.channels as usize]
        };
        
        PyArrayDyn::from_vec(py, &shape, self.data.clone())
    }
    
    pub fn duration_seconds(&self) -> f64 {
        self.data.len() as f64 / (self.sample_rate * self.channels) as f64
    }
}
```

### VAD Segment

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSegment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub energy_db: f32,
    pub confidence: f32,
}

impl VoiceSegment {
    pub fn duration_ms(&self) -> u32 {
        self.end_ms - self.start_ms
    }
}
```

---

## 7. State Transitions

### Node Lifecycle

```
┌─────────────┐
│   Created   │
└─────┬───────┘
      │ initialize()
      ▼
┌─────────────┐
│ Initialized │◄──┐
└─────┬───────┘   │
      │ execute() │ (repeatable)
      ▼           │
┌─────────────┐   │
│ Executing   ├───┘
└─────┬───────┘
      │ cleanup()
      ▼
┌─────────────┐
│  Cleaned    │
└─────────────┘
```

### Pipeline Execution States

```
┌─────────────┐
│    Idle     │
└─────┬───────┘
      │ execute()
      ▼
┌─────────────┐
│   Parsing   │ (parse manifest)
└─────┬───────┘
      │
      ▼
┌─────────────┐
│   Building  │ (build graph, topological sort)
└─────┬───────┘
      │
      ▼
┌─────────────┐
│  Executing  │ (node-by-node execution)
└─────┬───────┘
      │
      ▼
┌─────────────┐
│ Collecting  │ (gather metrics)
└─────┬───────┘
      │
      ▼
┌─────────────┐
│  Complete   │ (return results + metrics)
└─────────────┘
```

---

## 8. Data Validation Rules

### Manifest Validation

```rust
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        // Version check
        if self.version != "v1" {
            return Err(ExecutorError::ManifestError(
                format!("Unsupported manifest version: {}", self.version)
            ));
        }
        
        // Node ID uniqueness
        let mut seen_ids = HashSet::new();
        for node in &self.nodes {
            if !seen_ids.insert(&node.id) {
                return Err(ExecutorError::ManifestError(
                    format!("Duplicate node ID: {}", node.id)
                ));
            }
        }
        
        // Connection validity
        for conn in &self.connections {
            if !seen_ids.contains(&conn.from) {
                return Err(ExecutorError::ManifestError(
                    format!("Connection references unknown node: {}", conn.from)
                ));
            }
            if !seen_ids.contains(&conn.to) {
                return Err(ExecutorError::ManifestError(
                    format!("Connection references unknown node: {}", conn.to)
                ));
            }
        }
        
        Ok(())
    }
}
```

---

## Summary

**Key Data Structures**:
- `Manifest`: JSON schema for pipeline definition
- `PipelineGraph`: Execution graph with topological ordering
- `NodeExecutor`: Trait for all node types
- `ExecutorError`: Comprehensive error hierarchy
- `PipelineMetrics`: Performance monitoring data
- `AudioBuffer`: Audio data representation

**Validation Rules**:
- Manifest version must be "v1"
- Node IDs must be unique
- Connections must reference existing nodes
- Graph must be acyclic (DAG)
- Audio format conversions must be valid

**Next**: Create API contracts in `contracts/`
