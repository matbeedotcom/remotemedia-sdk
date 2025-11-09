# RustPython Benchmark Plan

**Status:** ✅ Ready to Run - Phase 1.6 Complete
**Updated:** 2025-10-23
**Dependencies:** ✅ RustPython VM integration (Phase 1.6 - COMPLETE)
**Current:** Phase 1.6 complete, benchmarks ready to execute

## Overview

This document outlines the benchmarks needed to compare three runtime environments:
1. **CPython** - Native Python execution (current baseline)
2. **RustPython** - Python code running in RustPython VM (Task 1.5)
3. **Rust Native** - Pure Rust node execution (Task 1.3.5 - complete)

---

## Why Benchmark RustPython?

### Key Questions

1. **Is RustPython slower than CPython?**
   - Expected: Yes, 0.5-2x slower for pure Python
   - RustPython is still maturing, CPython is highly optimized

2. **Does the Rust orchestration offset Python slowdown?**
   - Hypothesis: Yes, Rust executor speedup compensates for RustPython overhead
   - Total system performance should still beat pure CPython

3. **Which nodes benefit from Rust vs RustPython?**
   - **Rust Native:** Simple logic nodes (PassThrough, Calculator, routing)
   - **RustPython:** ML nodes with heavy Python dependencies (transformers, whisper)

4. **What's the optimal execution strategy?**
   - Mixed mode: Rust for simple nodes, RustPython for complex nodes
   - Capability-based selection

---

## Benchmark Scenarios

### 1. Pure Python Node Execution
**Goal:** Isolate RustPython VM performance

```python
# Benchmark CPython vs RustPython for same node
calculator_node = CalculatorNode(operation="add", operand=5)

# CPython
for i in range(100):
    result = calculator_node.process(i)

# RustPython (via Rust runtime)
for i in range(100):
    result = rustpython_calculator.process(i)
```

**Expected:**
- CPython: 79 µs (from current benchmarks)
- RustPython: 120-160 µs (estimated 1.5-2x slower)
- Rust Native: 38 µs (current)

### 2. Mixed Pipeline Execution
**Goal:** Measure total system performance

```
Pipeline: PassThrough(Rust) → Calculator(RustPython) → PassThrough(Rust)
```

**Expected:**
- All CPython: 79 µs
- All RustPython: 120-160 µs
- Mixed (Rust + RustPython): 70-90 µs (better than all-RustPython)
- All Rust: 38 µs (best case)

### 3. Real-World ML Pipeline
**Goal:** Test with actual ML workload

```
Pipeline: AudioSource → WhisperTranscription → TextTransform → Sink
```

**Expected:**
- CPython: Baseline (e.g., 500 ms/item)
- RustPython: 600-1000 ms/item (Python VM slower)
- **BUT:** Rust orchestration reduces startup, I/O overhead
- Net gain: 10-20% faster despite slower VM

### 4. Pipeline Initialization
**Goal:** Measure startup overhead

| Runtime | Init Time |
|---------|-----------|
| CPython | ~50-100 ms |
| RustPython | ~10-20 ms |
| Rust Native | ~5 ms |

**Advantage:** Rust/RustPython have faster startup

### 5. Concurrent Pipeline Execution
**Goal:** Test GIL impact

```
Execute 10 pipelines concurrently
```

**Expected:**
- CPython: Limited by GIL, minimal parallelism
- RustPython: True parallelism, near-linear scaling
- **Speedup:** 5-10x for concurrent workloads

---

## Implementation Requirements

### Task 1.5: RustPython VM Integration

#### 1.5.1: Embed RustPython
```rust
use rustpython::vm::VirtualMachine;

pub struct PythonNode {
    vm: VirtualMachine,
    code: String,
    instance: PyObjectRef,
}
```

#### 1.5.2: Node Lifecycle in RustPython
```rust
impl NodeExecutor for PythonNode {
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        // Load Python code into VM
        // Call node.__init__(**params)
    }

    async fn process(&mut self, input: Value) -> Result<Option<Value>> {
        // Call node.process(input) in VM
        // Marshal result back to Rust
    }
}
```

#### 1.5.3: Data Marshaling
- Rust Value → Python object
- Python object → Rust Value
- Handle numpy arrays, lists, dicts

### Benchmark Infrastructure

#### File: `runtime/benches/rustpython_execution.rs`
```rust
fn bench_python_node_cpython(c: &mut Criterion) {
    // Baseline: Call Python via PyO3
}

fn bench_python_node_rustpython(c: &mut Criterion) {
    // RustPython: Execute in embedded VM
}

fn bench_mixed_pipeline(c: &mut Criterion) {
    // Some nodes Rust native, some RustPython
}
```

#### File: `python-client/benchmarks/compare_runtimes.py`
```python
async def benchmark_all_runtimes():
    # Test same pipeline with:
    # 1. Pure CPython
    # 2. Rust runtime + RustPython
    # 3. Pure Rust (where possible)

    # Generate comparison report
```

---

## Metrics to Collect

### Performance Metrics
1. **Execution time** per node
2. **Throughput** (items/sec)
3. **Latency** (p50, p95, p99)
4. **Memory usage** (RSS, heap)
5. **CPU utilization**

### Comparative Metrics
1. **Speedup** vs CPython baseline
2. **Scaling** with concurrent pipelines
3. **Startup overhead** reduction
4. **Memory efficiency** improvement

### Quality Metrics
1. **Correctness** (output matches CPython)
2. **Compatibility** (which stdlib modules work)
3. **Error handling** (exceptions propagate correctly)

---

## Expected Results

### Scenario 1: Simple Pipelines (PassThrough, Calculator)
```
CPython:      79 µs    (baseline)
RustPython:   120 µs   (1.5x slower - VM overhead)
Rust Native:  38 µs    (2.1x faster - best choice)
```

**Recommendation:** Use Rust native nodes for simple logic

### Scenario 2: ML Pipelines (Whisper, Transformers)
```
CPython:      500 ms   (baseline)
RustPython:   600 ms   (1.2x slower - Python VM)
+ Rust orchestration: -50ms startup/IO
Net result:   550 ms   (1.1x slower)
```

**Recommendation:** Use RustPython for compatibility, optimize hot paths to Rust

### Scenario 3: Mixed Pipelines
```
All CPython:           79 µs
All RustPython:        120 µs
Optimal mix:           55 µs
  - PassThrough: Rust  (10 µs)
  - Calculator: Rust   (15 µs)  ← Can convert to Rust
  - ML node: RustPython (30 µs)  ← Must use Python
```

**Recommendation:** Capability-based routing (Task 1.3.6)

---

## Benchmark Acceptance Criteria

### Phase 1.4 (Complete ✅)
- [x] CPython baseline established ✅
- [x] Rust FFI integration complete ✅
- [x] Pipeline.run() integration complete ✅
- [x] Three-way comparison benchmarks implemented ✅
- [x] Automatic runtime selection working ✅

### Phase 1.6 (Complete ✅)
- [x] RustPython VM integrated ✅
- [x] Basic node execution working ✅
- [x] Stateful nodes with state preservation ✅
- [x] Streaming/generator nodes ✅
- [x] Python logging bridged to Rust tracing ✅
- [x] 11 test nodes passing all tests ✅
- [ ] Benchmark execution (ready to run)

### Phase 2 (Task 1.9)
- [ ] Compatibility matrix complete
- [ ] 80%+ of SDK nodes work in RustPython
- [ ] Known limitations documented

### Phase 3 (Task 1.13.7)
- [ ] Full runtime comparison
- [ ] Mixed-mode execution benchmarked
- [ ] Optimal execution strategy defined
- [ ] Total system speedup ≥1.5x vs pure CPython

---

## Benchmark Commands

### ✅ Phase 1.6 Complete - Ready to Benchmark

#### Rust-Native RustPython Benchmarks (NEW)
```bash
cd runtime

# Run all RustPython node execution benchmarks
cargo bench --bench rustpython_nodes

# Expected output:
# - Simple node execution: ~100-200 µs
# - Stateful node: ~110-220 µs
# - Marshaling overhead: by data type
# - Node initialization: ~150-300 µs
# - Complex transform: ~200-400 µs
# - Streaming nodes: ~500-1000 µs
# - Node chain (3 nodes): ~300-600 µs
# - VM reuse vs recreation: significant difference
```

#### Python-Side RustPython Benchmarks (NEW)
```bash
cd python-client

# Run comprehensive RustPython benchmark suite
python benchmarks/benchmark_rustpython.py

# With options:
python benchmarks/benchmark_rustpython.py -n 1000  # 1000 iterations
python benchmarks/benchmark_rustpython.py -d       # Detailed stats
python benchmarks/benchmark_rustpython.py -o results.json  # Custom output

# Expected output:
# - Single Node Execution
# - Stateful Node (Counter)
# - Simple 3-Node Pipeline
# - Complex 5-Node Pipeline
# - Data-Intensive (List Processing)
# - Mixed Data Types
```

#### Legacy Benchmarks (Still Available)
```bash
cd python-client

# Three-way comparison (CPython, Rust FFI, RustPython)
python benchmarks/compare_all_runtimes.py

# Detailed simple benchmark
python benchmarks/benchmark_simple.py

# Legacy pipeline benchmarks
python benchmarks/benchmark_runtimes.py
```

**Current Status:**
- ✅ CPython baseline established
- ✅ Rust FFI benchmarks working
- ✅ RustPython VM integrated and tested
- ⏳ Performance benchmarking ready to execute

---

## Optimization Opportunities

### 1. Node Conversion to Rust
**High-value conversions:**
- PassThroughNode → Already in Rust ✅
- CalculatorNode → Easy to implement in Rust
- BufferNode → Easy to implement in Rust
- FilterNode → Easy to implement in Rust

**Keep in RustPython:**
- ML nodes (WhisperNode, TransformersNode)
- Custom user nodes
- Complex Python dependencies

### 2. Hot Path Optimization
- Identify nodes called most frequently
- Convert to Rust if possible
- Use native Rust for control flow

### 3. Data Marshaling Optimization
- Zero-copy for numpy arrays (shared memory)
- Batch marshaling for multiple items
- Lazy conversion (only when needed)

---

## Success Metrics

### MVP (Phase 1)
✅ **Achieved:** 2x speedup with pure Rust nodes
⏳ **Target:** 1.5x speedup with RustPython nodes
⏳ **Target:** <200ms startup time

### Production (Phase 3)
⏳ **Target:** 2-3x speedup for typical workloads
⏳ **Target:** 5-10x speedup for concurrent workloads
⏳ **Target:** <50ms cold start

---

## Risks and Mitigations

### Risk 1: RustPython Too Slow
**Impact:** Performance worse than CPython
**Mitigation:**
- Use CPython fallback (Task 1.10)
- Convert more nodes to Rust
- Profile and optimize RustPython integration

### Risk 2: Compatibility Issues
**Impact:** Many nodes don't work in RustPython
**Mitigation:**
- Compatibility matrix (Task 1.9)
- Auto-fallback to CPython
- Document known limitations

### Risk 3: Marshaling Overhead
**Impact:** Data conversion negates speedup
**Mitigation:**
- Zero-copy techniques
- Batch processing
- Keep data in Rust when possible

---

## Conclusion

RustPython benchmarks will answer the critical question:

> **Can we achieve backward compatibility (run Python nodes) while still gaining performance benefits?**

**Hypothesis:** Yes, through:
1. Faster Rust orchestration layer
2. Better concurrency (no GIL)
3. Faster startup times
4. Mixed execution (Rust + RustPython)

**Next Steps:**
1. Complete Task 1.5 (RustPython integration)
2. Implement benchmarks from this plan
3. Measure, optimize, repeat
4. Achieve ≥1.5x total speedup target
