# Whisper WASM Integration Analysis

## Executive Summary

**Recommendation**: **Defer Whisper WASM integration** for now. Focus on completing Phase 2.7 deployment first.

**Rationale**:
- Whisper WASM adds significant complexity (~75MB model + build toolchain)
- Browser demo is already feature-complete with Rust + Python nodes
- Native Rust Whisper integration (`rwhisper`) already works well
- Can be added later as Phase 3 without blocking current deployment

## Current State

### What We Have ✅

1. **Native Rust Whisper Node** (`RustWhisperNode`)
   - Uses `rwhisper` crate (Rust bindings to whisper.cpp)
   - Fully functional in native runtime
   - Example: `examples/rust_runtime/11_whisper_benchmark.py`
   - Feature flag: `whisper = ["rwhisper", "rodio"]`

2. **Python Whisper Nodes**
   - `WhisperXTranscriber` (CTranslate2 optimized)
   - Working benchmarks comparing Python vs Rust

3. **Hybrid Browser Runtime**
   - Rust WASM for native nodes
   - Pyodide for Python nodes
   - Successfully deployed and tested

### What whisper.cpp Offers

- **Official WASM build**: `examples/whisper.wasm/`
- **Emscripten-based**: Compiles to WebAssembly
- **Browser-compatible**: Runs entirely client-side
- **Privacy-focused**: Audio processed locally

## Integration Challenges

### 1. Build Complexity

**whisper.cpp WASM Requirements**:
```bash
# Requires Emscripten toolchain
git clone https://github.com/ggerganov/whisper.cpp
cd whisper.cpp/examples/whisper.wasm
emcmake cmake ..
make -j
```

**Output**: `libmain.js` + embedded WASM (or separate .wasm file)

**Challenge**: Different toolchain than our `wasm32-wasip1` builds
- Our WASM: Rust → wasm32-wasip1 → WASI
- Whisper WASM: C++ → Emscripten → JavaScript glue

### 2. Model Size

| Model | Size | RTF (CPU) | Use Case |
|-------|------|-----------|----------|
| tiny | 75 MB | ~0.3x | Demo, fast preview |
| base | 142 MB | ~0.5x | Moderate quality |
| small | 466 MB | ~1.0x | Good quality |
| medium | 1.5 GB | ~2.5x | High quality |

**Issue**: Even `tiny` model is 75 MB (4x larger than our entire WASM runtime)

**Impact**:
- Slow initial load (several seconds for model download)
- High memory usage in browser
- Storage requirements

### 3. Integration Approaches

#### Option A: Separate Whisper WASM Binary

```
Browser
├─ pipeline_executor.wasm (20 MB) - Rust nodes
├─ whisper.wasm (embedded in libmain.js) - Transcription
└─ Pyodide (40 MB) - Python nodes
```

**Pros**:
- Clean separation
- Can use official whisper.cpp builds
- Independent updates

**Cons**:
- **Three separate WASM runtimes** to coordinate
- Complex data marshaling (audio → whisper.wasm → results)
- Different build systems (Rust vs Emscripten)
- Total size: ~135 MB + models

#### Option B: Embed whisper.cpp in Rust WASM

Use `whisper-rs` or `rwhisper` crate, compile to WASM:

```rust
// In runtime/src/nodes/whisper.rs
#[cfg(target_family = "wasm")]
impl RustWhisperNode {
    // Try to compile rwhisper for WASM
}
```

**Pros**:
- Single Rust WASM binary
- Consistent toolchain
- Unified manifest format

**Cons**:
- `rwhisper` likely **doesn't support WASM** (uses `rodio` for audio, system-dependent)
- Would need to fork/modify or use whisper.cpp directly via FFI
- WASM build size balloons (20 MB → 100+ MB with model)

#### Option C: Pyodide + Python Whisper

Use existing `WhisperXTranscriber` via Pyodide:

**Cons**:
- WhisperX requires CTranslate2 (C++ dependencies, doesn't work in browser)
- OpenAI Whisper Python requires PyTorch (gigabytes, impractical)
- No practical pure-Python Whisper implementation

### 4. User Experience Trade-offs

| Aspect | Without Whisper WASM | With Whisper WASM |
|--------|---------------------|-------------------|
| Initial load | ~8s (Pyodide) | ~20s+ (models) |
| Demo size | 60 MB total | 135+ MB total |
| Complexity | 2 runtimes | 3 runtimes |
| Use cases | Math, text, data | + Audio transcription |
| Showcase value | Good | Excellent |

## Recommendation: Phase 3 Roadmap

### Immediate (Phase 2.7): Deploy Current Demo ✅

**Focus**:
- Complete deployment to GitHub Pages
- Showcase hybrid Rust + Python execution
- .rmpkg package format
- Calculator and text processing examples

**Benefits**:
- Get demo live quickly
- Gather user feedback
- Prove architecture works

### Near-term (Phase 3.1): Whisper Investigation

**Research tasks**:
1. Test official whisper.wasm standalone
2. Benchmark model sizes and load times
3. Design integration architecture
4. Create PoC with tiny model

**Success criteria**:
- <5s model load time
- <1.0x RTF for tiny model
- Clean integration with existing pipeline runner

### Future (Phase 3.2): Full Integration

**Only proceed if**:
- User demand is clear (feedback from deployed demo)
- Technical challenges solved (PoC successful)
- Resources available (time, expertise)

**Implementation**:
- Option A (Separate WASM) if whisper.wasm API is stable
- Build tooling for Emscripten + Rust dual-WASM packages
- Update .rmpkg format to support multiple WASM binaries
- Create audio transcription examples

## Alternative: Server-Side Whisper

### Hybrid Approach

Instead of browser Whisper, support **remote Whisper nodes**:

```json
{
  "nodes": [
    {
      "id": "whisper",
      "node_type": "RemoteWhisperNode",
      "params": {
        "endpoint": "https://api.example.com/whisper",
        "model": "base"
      }
    }
  ]
}
```

**Pros**:
- No browser WASM complexity
- Use powerful server GPUs
- Faster transcription (GPU acceleration)
- Smaller browser bundle

**Cons**:
- Requires server infrastructure
- Privacy concerns (audio leaves browser)
- Network latency

**Best for**: Production applications with audio transcription needs

## Conclusion

### For Phase 2.7 Deployment: ❌ Skip Whisper WASM

**Reasons**:
1. **Complexity risk**: Don't jeopardize current deployment
2. **Size concerns**: 75 MB model is too large for demo
3. **Limited ROI**: Text/math examples already showcase hybrid runtime
4. **Time constraint**: Focus on getting current demo live

### For Future Phases: ✅ Revisit After Deployment

**Next steps**:
1. Deploy current demo (Phase 2.7)
2. Gather user feedback
3. Assess demand for audio features
4. Build PoC with whisper.wasm (Phase 3.1)
5. Decide based on PoC results

### Documentation

**Update tasks.md**:
```markdown
## Phase 3: Whisper WASM (Deferred)

**Status**: Investigation complete, implementation deferred

**Decision**: Focus on Phase 2.7 deployment first. Whisper WASM adds:
- 75 MB minimum (tiny model)
- Third WASM runtime (Emscripten)
- Complex build toolchain

**Recommendation**: Deploy current demo, then revisit based on user feedback.

**Alternative**: Native Rust Whisper already works well for server-side use cases.
```

## References

- [whisper.cpp GitHub](https://github.com/ggerganov/whisper.cpp)
- [whisper.wasm example](https://github.com/ggerganov/whisper.cpp/tree/master/examples/whisper.wasm)
- [rwhisper crate](https://github.com/tazz4843/whisper-rs) (note: not on crates.io, likely unmaintained)
- [Emscripten documentation](https://emscripten.org/)
