# Whisper WASM/WASI Implementation Status

## Goal

Run Whisper transcription in `wasm32-wasip1` target (same as our current WASM runtime), not Emscripten.

## Current Status: **Blocked on Windows CMake Limitations**

### What We Tried

1. ✅ **Installed wasi-sdk 24.0** - WASI toolchain with clang for cross-compilation
2. ✅ **Created CMake toolchain file** - `wasi-toolchain.cmake` for WASI builds
3. ✅ **Configured environment** - Set CMAKE_TOOLCHAIN_FILE, BINDGEN_EXTRA_CLANG_ARGS
4. ❌ **Build whisper-rs-sys** - **BLOCKED**

### Build Errors

```
CMake Error: Could NOT find Threads (missing: Threads_FOUND)
Call Stack: ggml/CMakeLists.txt:237 (find_package)
```

**Root cause**: whisper.cpp's `ggml/CMakeLists.txt` calls `find_package(Threads REQUIRED)` which fails because:
- WASI doesn't have pthread support
- CMake's `FindThreads.cmake` can't find threads in WASI sysroot
- Setting `Threads_FOUND=TRUE` in toolchain file gets overridden by `find_package()`

### Technical Challenges

| Challenge | Status | Solution Complexity |
|-----------|--------|---------------------|
| **whisper.cpp requires pthreads** | ❌ Blocked | High - need to patch whisper.cpp CMake |
| **bindgen can't find stdio.h** | ⚠️ Warning | Medium - fixed with BINDGEN_EXTRA_CLANG_ARGS |
| **CMake uses wrong compiler** | ✅ Fixed | Low - toolchain file works |
| **WASI has no thread support** | ❌ Fundamental | Very High - architecture issue |

## Why This Is Hard

### 1. whisper.cpp Threading Architecture

whisper.cpp uses threads for parallel inference:
- `ggml` library uses pthreads for multi-threaded operations
- WASI Preview 1 doesn't support threads
- WASI Preview 2 has experimental thread support (wasi-threads)

### 2. Build System Complexity

```
Rust (cargo)
  → whisper-rs-sys (build.rs)
    → CMake
      → whisper.cpp/CMakeLists.txt
        → ggml/CMakeLists.txt (requires threads)
```

Each layer needs WASI-aware configuration.

### 3. Alternative Approaches

#### Option A: Patch whisper.cpp for Single-Threaded WASM
**Effort**: High
**Steps**:
1. Fork whisper.cpp or patch locally
2. Make threads optional in `ggml/CMakeLists.txt`
3. Add `#ifdef WASM` guards around pthread code
4. Single-threaded inference (slower but works)

**Code changes needed**:
```cmake
# In ggml/CMakeLists.txt
if(NOT CMAKE_SYSTEM_NAME STREQUAL "WASI")
    find_package(Threads REQUIRED)
endif()
```

#### Option B: Use whisper.cpp Emscripten Build
**Effort**: Medium
**Trade-off**: Different WASM toolchain (Emscripten vs wasm32-wasip1)

Our current architecture:
```
Runtime: wasm32-wasip1 (WASI)
Pyodide: Emscripten
```

Adding Whisper:
```
Whisper: Emscripten (whisper.wasm)
```

**Issues**:
- Three different WASM runtimes
- Can't integrate into single `wasm32-wasip1` binary
- Complex data marshaling

#### Option C: Wait for WASI Preview 2 + wasi-threads
**Effort**: Low (wait)
**Timeline**: Uncertain (months to years)

WASI Preview 2 will have threads, but:
- Still experimental
- No stable Rust support yet
- whisper.cpp would still need updates

#### Option D: Pure Rust Whisper Implementation
**Effort**: Very High
**Example**: https://github.com/FL33TW00D/whisper-burn

Rewrite whisper in pure Rust:
- No C dependencies
- Native WASM support
- Much slower than whisper.cpp (no optimizations)

## Recommended Path Forward

### Short-term: **Document and Defer**

**Recommendation**: Keep Phase 3 deferred until tooling improves.

**Rationale**:
1. **Native Whisper works** - `RustWhisperNode` via `rwhisper` for server-side
2. **Complex engineering** - Patching whisper.cpp CMake is non-trivial
3. **Uncertain benefit** - Browser transcription is niche use case
4. **Better alternatives** - Server-side GPU transcription is faster anyway

### Medium-term: **Experiment with Patches**

If there's user demand, try Option A:

1. Fork whisper.cpp or create local patches
2. Make threads optional with `-DGGML_NO_THREADS=ON` flag
3. Test single-threaded WASM build
4. Benchmark performance vs native

**Estimated effort**: 2-4 days of experimentation

### Long-term: **Hybrid Approach**

Best production solution:

```
Browser Demo:
├─ Rust nodes (MultiplyNode, Add) - wasm32-wasip1
├─ Python nodes (TextProcessor) - Pyodide
└─ Whisper - Server-side API (native + GPU)
```

**Benefits**:
- Simple browser runtime
- Fast GPU transcription
- No WASM threading issues
- Privacy via self-hosted API

## Current Deliverables

### What We Have ✅

1. **wasi-sdk 24.0** installed and configured
2. **CMake toolchain file** for WASI cross-compilation
3. **whisper-rs** dependency added to Cargo.toml
4. **Feature flag** `whisper-wasm` for conditional compilation
5. **Documentation** of technical challenges

### What's Missing ❌

1. **Working whisper.cpp WASI build** - blocked on threads
2. **WhisperNode WASM implementation** - depends on #1
3. **Browser demo integration** - depends on #2
4. **Model loading for WASM** - architecture not defined

## Next Steps

### If Continuing Phase 3:

1. **Patch whisper.cpp CMake**:
   ```bash
   cd runtime/target/wasm32-wasip1/debug/build/whisper-rs-sys-*/out/whisper.cpp
   # Edit ggml/CMakeLists.txt to make threads optional
   ```

2. **Set CMake define**:
   ```bash
   CMAKE_DEFINES="-DGGML_NO_THREADS=ON" cargo build ...
   ```

3. **Test single-threaded build**

4. **Create WhisperNode wrapper** for wasm32-wasip1

### If Deferring Phase 3:

1. ✅ **Update tasks.md** - Mark as "Investigation Complete, Blocked on Tooling"
2. ✅ **Document findings** - This file
3. ✅ **Focus on deployment** - Phase 2.7
4. **Revisit later** - When WASI threads are stable

## Files Created

- `wasi-toolchain.cmake` - CMake toolchain for WASI
- `docs/WHISPER_WASM_WASI_STATUS.md` - This file
- `runtime/Cargo.toml` - Added `whisper-rs` dependency + `whisper-wasm` feature

## References

- [whisper.cpp](https://github.com/ggerganov/whisper.cpp)
- [whisper-rs](https://docs.rs/whisper-rs)
- [WASI Threads Proposal](https://github.com/WebAssembly/wasi-threads)
- [wasi-sdk](https://github.com/WebAssembly/wasi-sdk)
- [whisper-burn (Pure Rust)](https://github.com/FL33TW00D/whisper-burn)
