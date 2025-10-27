# From Task 1.3.5 to Task 1.3.6

**Completed:** Task 1.3.5 - Node Lifecycle Management
**Next:** Task 1.3.6 - Basic Capability-Aware Execution Placement
**Date:** 2025-10-22

---

## What We Just Completed (Task 1.3.5)

### Node Lifecycle Management ✅

**Implemented:**
- `NodeExecutor` trait with `initialize()`, `process()`, `cleanup()`
- `NodeRegistry` with factory pattern
- Built-in nodes: `PassThroughNode`, `EchoNode`
- Full lifecycle integration in `Executor`
- 14 tests passing
- Benchmarks: **2-2.5x speedup over Python**

**Key Files:**
- `runtime/src/nodes/mod.rs` - Node lifecycle system
- `runtime/src/executor/mod.rs` - Executor integration
- `runtime/benches/pipeline_execution.rs` - Performance benchmarks
- `TASK_1.3.5_SUMMARY.md` - Complete documentation

**Benchmark Results:**
| Scenario | Python | Rust | Speedup |
|----------|--------|------|---------|
| Simple (3 nodes, 100 items) | 79.25 µs | 38.04 µs | **2.08x** |
| Complex (10 nodes, 100 items) | 773.66 µs | 313.48 µs | **2.47x** |

---

## What's Next (Task 1.3.6)

### Basic Capability-Aware Execution Placement

**Goal:** Match nodes to executors based on GPU/CPU/memory requirements

**Why This Matters:**
- Some nodes need GPU (Whisper, Transformers)
- Some nodes work fine on CPU (PassThrough, Calculator)
- Executors have different capabilities (GPU vs CPU machines)
- Need to route nodes to compatible executors

---

## Implementation Plan for Task 1.3.6

### Step 1: Define Executor Capabilities

**Create:** `runtime/src/executor/placement.rs`

```rust
/// Describes the capabilities of an executor
#[derive(Debug, Clone)]
pub struct ExecutorCapabilities {
    /// Does this executor have a GPU?
    pub has_gpu: bool,

    /// GPU type if available
    pub gpu_type: Option<GpuType>,

    /// GPU memory in GB
    pub gpu_memory_gb: f64,

    /// Number of CPU cores
    pub cpu_cores: usize,

    /// System memory in GB
    pub memory_gb: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GpuType {
    Cuda,   // NVIDIA
    Rocm,   // AMD
    Metal,  // Apple
}

impl ExecutorCapabilities {
    /// Detect local executor capabilities
    pub fn detect() -> Self {
        // Check for CUDA, ROCm, Metal
        // Query system memory
        // Count CPU cores
    }
}
```

### Step 2: Capability Matching Logic

```rust
/// Check if a node can execute on this executor
pub fn can_execute_node(
    node: &GraphNode,
    capabilities: &ExecutorCapabilities
) -> Result<bool> {
    // No requirements = can run anywhere
    let Some(requirements) = &node.capabilities else {
        return Ok(true);
    };

    // Check GPU requirements
    if let Some(gpu_req) = &requirements.gpu {
        if gpu_req.required && !capabilities.has_gpu {
            return Ok(false);
        }

        if let Some(min_memory) = gpu_req.min_memory_gb {
            if capabilities.gpu_memory_gb < min_memory {
                return Ok(false);
            }
        }
    }

    // Check memory requirements
    if let Some(min_memory) = requirements.memory_gb {
        if capabilities.memory_gb < min_memory {
            return Ok(false);
        }
    }

    Ok(true)
}
```

### Step 3: Integrate with Executor

**Modify:** `runtime/src/executor/mod.rs`

```rust
pub struct Executor {
    config: ExecutorConfig,
    registry: NodeRegistry,
    capabilities: ExecutorCapabilities,  // NEW
}

impl Executor {
    pub fn new() -> Self {
        Self {
            config: ExecutorConfig::default(),
            registry: NodeRegistry::default(),
            capabilities: ExecutorCapabilities::detect(),  // NEW
        }
    }

    pub async fn execute_with_input(
        &self,
        manifest: &Manifest,
        input_data: Vec<Value>
    ) -> Result<ExecutionResult> {
        let graph = PipelineGraph::from_manifest(manifest)?;

        // NEW: Check all nodes are compatible
        for node_id in &graph.execution_order {
            let node = graph.nodes.get(node_id).unwrap();
            if !can_execute_node(node, &self.capabilities)? {
                return Err(Error::Capability(
                    format!(
                        "Node '{}' requires capabilities not available: {:?}",
                        node.id, node.capabilities
                    )
                ));
            }
        }

        // ... rest of execution
    }
}
```

### Step 4: Tests

**Create:** `runtime/tests/test_placement.rs`

```rust
#[test]
fn test_cpu_only_executor_accepts_cpu_node() {
    let caps = ExecutorCapabilities {
        has_gpu: false,
        gpu_type: None,
        gpu_memory_gb: 0.0,
        cpu_cores: 8,
        memory_gb: 16.0,
    };

    let node = GraphNode {
        capabilities: Some(CapabilityRequirements {
            gpu: None,
            memory_gb: Some(4.0),
        }),
        // ...
    };

    assert!(can_execute_node(&node, &caps).unwrap());
}

#[test]
fn test_cpu_only_executor_rejects_gpu_node() {
    let caps = ExecutorCapabilities {
        has_gpu: false,
        // ...
    };

    let node = GraphNode {
        capabilities: Some(CapabilityRequirements {
            gpu: Some(GpuRequirements {
                required: true,
                min_memory_gb: Some(4.0),
                gpu_type: Some("cuda".to_string()),
            }),
        }),
        // ...
    };

    assert!(!can_execute_node(&node, &caps).unwrap());
}

#[test]
fn test_insufficient_memory_rejected() {
    let caps = ExecutorCapabilities {
        memory_gb: 8.0,
        // ...
    };

    let node = GraphNode {
        capabilities: Some(CapabilityRequirements {
            memory_gb: Some(16.0),  // Needs more than available
        }),
        // ...
    };

    assert!(!can_execute_node(&node, &caps).unwrap());
}
```

---

## Changes Required

### New Files
1. `runtime/src/executor/placement.rs` - Capability matching logic
2. `runtime/tests/test_placement.rs` - Placement tests

### Modified Files
1. `runtime/src/executor/mod.rs` - Add capabilities to Executor
2. `runtime/src/lib.rs` - Export placement module
3. `runtime/Cargo.toml` - Add sysinfo crate for capability detection

### Dependencies to Add

```toml
# For detecting system capabilities
sysinfo = "0.30"

# Optional: For GPU detection
[target.'cfg(target_os = "linux")'.dependencies]
nvml-wrapper = { version = "0.9", optional = true }
```

---

## Acceptance Criteria

**Task 1.3.6 Complete When:**
- [ ] `ExecutorCapabilities` struct defined
- [ ] `can_execute_node()` matching logic working
- [ ] Local capability detection implemented
- [ ] Executor checks capabilities before execution
- [ ] Error returned when capabilities don't match
- [ ] Tests passing (at least 5 tests)
- [ ] Documentation updated

---

## Testing Strategy

### Unit Tests
- Capability matching logic (various scenarios)
- GPU requirements (required vs optional)
- Memory requirements (sufficient vs insufficient)
- No requirements (always compatible)

### Integration Tests
- CPU-only executor with CPU nodes → success
- CPU-only executor with GPU nodes → error
- GPU executor with GPU nodes → success
- Memory-constrained executor → appropriate failures

### Manual Testing
```bash
# Run on CPU machine
cargo test --test test_placement

# Run executor with different manifests
cargo run --example capability_test
```

---

## After Task 1.3.6

**Next Tasks:**
- **1.3.7:** Local-first execution (default to local when no host specified)
- **1.3.8:** Fallback logic (try local → fall back to remote if capabilities not met)

**Together these complete:** Phase 1.3 Rust Runtime Core ✅

---

## Questions?

**Before starting, confirm:**
1. Approach makes sense?
2. Should we detect GPU automatically or require explicit config?
3. What level of capability detection is needed for MVP?
   - Basic: Just has_gpu flag
   - Advanced: GPU type, memory, CUDA version, etc.

**My recommendation:** Start with basic (has_gpu + memory), expand later.
