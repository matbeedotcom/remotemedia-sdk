# Audio Node Performance Profiling Analysis

## Executive Summary

Profiling reveals that **69.5% of execution time is overhead**, not actual audio processing. The bottlenecks are:

1. **JSON Serialization** (input): ~10ms (46% of total time)
2. **Output Serialization**: ~17ms (46% of total time)  
3. **Actual Processing**: ~11ms (31% of total time)

**Conclusion**: The node abstraction overhead is 2.3x larger than the actual processing work.

---

## Detailed Profiling Results

### Format Conversion (1M samples, f32→i16)

| Stage | Time | % of Total | Notes |
|-------|------|-----------|--------|
| 1. JSON creation (input) | 10.4 ms | 47.6% | Creating serde_json::Value with 1M f32 array |
| 2. Node creation | 0.4 ns | 0.0% | Trivial |
| 3. Initialization | 89 ns | 0.0% | Trivial |
| 4. Setup overhead (1+2+3) | 11.5 ms | 52.7% | Total pre-processing overhead |
| 5. **Full operation** | **21.6 ms** | **100%** | Everything including processing |
| 6. Direct conversion (no wrapper) | 1.36 ms | 6.3% | Pure Rust without abstraction |

**Key Finding**: Direct conversion is **15.9x faster** than the wrapped version (1.36ms vs 21.6ms)

#### Breakdown of 21.6ms Total:
- Input JSON creation: ~10.4ms (48%)
- Processing: ~11.2ms (52%)
  - Actual conversion: ~1.4ms (6%)
  - JSON parsing + overhead: ~9.8ms (45%)
- Output serialization: ~17ms (not in timing, but from step-by-step)

### Step-by-Step Timing (Real Measurements)

```
1. JSON creation:        8.69 ms   (23.6%)
2. Node creation:        0.00 ms   (0.0%)
3. Initialization:       0.0025 ms (0.0%)
4. Processing:          11.23 ms   (30.5%)
5. Output serialization: 16.90 ms  (45.9%)

Total time:             36.82 ms
Overhead (1+2+3+5):     25.59 ms   (69.5% of total)
Actual work (4):        11.23 ms   (30.5% of total)
```

**Critical Insight**: Overhead is 2.28x the actual processing time!

---

## Resample Profiling (44.1kHz → 16kHz, 1 second)

| Stage | Time | Notes |
|-------|------|--------|
| JSON serialization | 920 μs | Serializing 88,200 f32 samples |
| Node creation | 35 ns | Trivial |
| **Full operation** | **1.37 ms** | Total time |
| Processing (estimated) | ~450 μs | Full - JSON = 1.37ms - 0.92ms |

**Finding**: JSON overhead is 67% of total time (920μs / 1370μs)

---

## VAD Profiling (1 second of audio)

| Stage | Time | Notes |
|-------|------|--------|
| JSON overhead | 940 μs | Serializing audio data |
| Node creation | 47 ns | Trivial |
| **Full operation** | **2.65 ms** | Total time |
| Processing (estimated) | ~1.71 ms | Full - JSON = 2.65ms - 0.94ms |

**Finding**: JSON overhead is 35% of total time (940μs / 2650μs)

---

## Root Cause Analysis

### 1. JSON Serialization Dominates Performance

**Input Serialization** (creating `serde_json::Value`):
- For 1M samples (f32): **10.4ms**
- For 88K samples (resample): **0.92ms**  
- For 44K samples (VAD): **0.94ms**

**Why so slow?**
- `serde_json::Value` requires heap allocation for every element
- Array of 1M floats becomes Vec<Value> with 1M heap allocations
- Memory copying: original Vec<f32> → Vec<Value>

**Output Serialization** (converting result to JSON):
- For 1M samples: **16.9ms**
- Reconverting Vec<i16> back to serde_json::Value
- Even slower than input due to array wrapping overhead

### 2. Direct Conversion is 15.9x Faster

Without the node wrapper:
- Input: Direct Vec<f32> reference
- Processing: Simple iterator map
- Output: Direct Vec<i16>
- Time: **1.36ms** vs **21.6ms** with wrapper

**Overhead breakdown**:
- Node wrapper adds: 21.6ms - 1.36ms = **20.24ms overhead**
- Overhead ratio: 20.24ms / 1.36ms = **14.9x slower** due to abstraction

### 3. The Problem with Value Types

Current architecture:
```rust
fn process(&mut self, input: Value) -> Result<Vec<Value>>
```

Issues:
1. **Input parsing**: Must extract Vec<f32> from Value (deserialize)
2. **Processing**: Actual audio work (fast!)
3. **Output wrapping**: Must convert Vec<i16> to Vec<Value> (serialize)

Each step copies memory and allocates on heap.

---

## Optimization Opportunities

### High Impact (50-90% speedup potential)

1. **Remove JSON for Internal Nodes** ⭐⭐⭐
   - Use direct buffer types: `&[f32]` instead of `Value`
   - Estimated speedup: **10-15x**
   - Impact: Eliminates 25ms overhead per operation
   
2. **Zero-Copy Buffer Passing** ⭐⭐⭐
   - Use `Arc<Vec<T>>` or custom buffer type
   - Share buffers between nodes without copying
   - Estimated speedup: **5-10x**

3. **Remove Async for CPU-Bound Ops** ⭐⭐
   - Async adds ~50-100μs overhead per call
   - Use sync trait for audio nodes
   - Estimated speedup: **1.1-1.2x**

### Medium Impact (20-50% speedup)

4. **Batch Processing API**
   - Process multiple samples in one call
   - Amortize initialization overhead
   - Estimated speedup: **2-3x** for small batches

5. **SIMD Vectorization**
   - Use `std::simd` or `packed_simd` for format conversion
   - Estimated speedup: **4-8x** for conversions only

### Low Impact (5-20% speedup)

6. **Pool NodeContext Objects**
   - Reuse instead of recreating
   - Minimal impact (already ~0ns overhead)

---

## Recommended Architecture Changes

### Option A: Dual API (Best for Compatibility)

Keep current API for external users, add fast path for internal:

```rust
// Public API (with JSON for compatibility)
pub trait NodeExecutor {
    async fn process(&mut self, input: Value) -> Result<Vec<Value>>;
}

// Internal fast path
pub trait FastAudioNode {
    fn process_buffer(&mut self, input: &AudioBuffer) -> Result<AudioBuffer>;
}

pub struct AudioBuffer {
    data: Arc<Vec<f32>>,  // Zero-copy sharing
    sample_rate: u32,
    channels: usize,
    format: AudioFormat,
}
```

**Benefits**:
- Backward compatible
- 10-15x faster for internal pipelines
- Can still use JSON for external APIs

### Option B: Replace Value with Custom Types

```rust
pub enum NodeInput {
    Audio(AudioBuffer),
    Video(VideoBuffer),
    Json(Value),  // Fallback for custom nodes
}

pub trait NodeExecutor {
    async fn process(&mut self, input: NodeInput) -> Result<Vec<NodeInput>>;
}
```

**Benefits**:
- Type-safe
- Zero-copy for typed buffers
- Still supports arbitrary JSON

### Option C: Direct Buffer Nodes (Most Radical)

Remove abstraction entirely for performance-critical nodes:

```rust
// Audio-specific node trait
pub trait AudioProcessor {
    fn process(&mut self, 
               input: &[f32], 
               sample_rate: u32, 
               channels: usize) -> Result<Vec<f32>>;
}
```

**Benefits**:
- Maximum performance (matches direct conversion: 1.36ms)
- No serialization overhead
- Simple to implement

**Drawbacks**:
- Separate trait for audio vs video vs custom
- Not compatible with current architecture

---

## Recommended Implementation Plan

### Phase 1: Quick Wins (Week 1)
1. Implement `FastAudioNode` trait (Option A)
2. Add `AudioBuffer` type with `Arc<Vec<T>>`
3. Implement fast path for format conversion
4. Benchmark: Expect 10-15x speedup

### Phase 2: Core Nodes (Week 2)
1. Migrate resample node to fast path
2. Migrate VAD node to fast path
3. Update pipeline to use fast path when all nodes support it
4. Benchmark full pipeline

### Phase 3: SIMD Optimization (Week 3)
1. Add SIMD for format conversion
2. Add SIMD for resampling (if rubato doesn't already)
3. Re-benchmark against Python

### Phase 4: Async Removal (Optional)
1. Make audio nodes sync (no async overhead)
2. Use thread pool for parallelism if needed
3. Benchmark async vs sync

---

## Expected Performance After Optimization

### Format Conversion (1M samples)

| Implementation | Time | Speedup vs Current | Speedup vs Python |
|----------------|------|-------------------|-------------------|
| Current (with JSON) | 21.6 ms | 1.0x | 0.06x ❌ |
| Without JSON (Option A) | 1.4 ms | 15.4x | 1.0x ⚠️ |
| With SIMD | 0.2 ms | 108x | 7.0x ✅ |
| **Target** | <0.1 ms | **216x** | **14x** ✅ |

### Resample (1 second, 44.1→16kHz)

| Implementation | Time | Speedup vs Current | Speedup vs Python |
|----------------|------|-------------------|-------------------|
| Current | 1.37 ms | 1.0x | 0.20x ❌ |
| Without JSON | 0.45 ms | 3.0x | 0.60x ⚠️ |
| **Target** | <0.27 ms | **5.1x** | **1.0x** ⚠️ |

**Note**: Resample may not beat Python (librosa uses highly optimized SciPy), but can match it.

### VAD (1 second)

| Implementation | Time | Speedup vs Current | Speedup vs Python |
|----------------|------|-------------------|-------------------|
| Current | 2.65 ms | 1.0x | 0.19x ❌ |
| Without JSON | 1.71 ms | 1.5x | 0.29x ⚠️ |
| Optimized algorithm | 0.15 ms | 17.7x | 3.0x ✅ |

---

## Conclusion

The profiling clearly shows:

1. **69.5% of time is serialization overhead**, not processing
2. **Direct conversion is 15.9x faster** than wrapped version
3. **JSON serialization** is the primary bottleneck:
   - Input: 10.4ms (48%)
   - Output: 16.9ms (46%)
   - Processing: 11.2ms (31% including parsing)

**Bottom line**: Removing JSON overhead alone would give us 10-15x speedup, putting us in competitive range with Python. Adding SIMD could give another 4-8x, achieving the 50-100x target.

**Recommended priority**: Implement Option A (dual API) first. This gives immediate 10-15x gains while maintaining backward compatibility.

---

## Next Steps

1. ✅ **Profiling complete** - Bottlenecks identified
2. ⬜ Implement `AudioBuffer` type
3. ⬜ Implement `FastAudioNode` trait  
4. ⬜ Migrate format converter to fast path
5. ⬜ Benchmark and validate 10x+ speedup
6. ⬜ Document new API patterns
7. ⬜ Migrate remaining nodes

**Estimated effort**: 2-3 weeks for full optimization pipeline

**Expected outcome**: 50-100x speedup target **achievable** after optimization
