# WASM/Browser Runtime - Archived

**Archived**: 2025-10-27  
**Original Location**: `browser-demo/`, `wasi-sdk-*/`, `docs/WASM_*.md`, `docs/PYODIDE_*.md`, `docs/BROWSER_*.md`  
**Size**: ~15,000 LoC  
**Status**: Functional but not production-ready  
**Reason**: Pyodide performance overhead, v0.2.0 Rust+PyO3 runtime provides superior performance

## What's Archived

This directory contains the experimental WASM-based Python runtime implementation using Pyodide and WASI toolchains:

### 1. Browser Demo Application
- **Location**: `browser-demo/`
- **Description**: Browser-based demo showcasing RemoteMedia SDK running in the browser via Pyodide
- **Features**: Client-side audio processing, WASM-compiled nodes, browser UI
- **Status**: Working demo but 10x slower than native runtime

### 2. WASI Toolchains
- **Locations**: 
  - `wasi-sdk-24.0-x86_64-windows/`
  - `wasi-sdk-27.0-x86_64-linux/`
  - `wasi-sdk-27.0-x86_64-windows/`
- **Description**: WebAssembly System Interface toolchains for compiling to WASM
- **Size**: ~500MB total (already in .gitignore)
- **Purpose**: Compile Python/Rust code to WASM for browser execution

### 3. WASM Documentation
- **Location**: `docs/`
- **Files**:
  - `WASM_*.md` - WebAssembly runtime documentation
  - `PYODIDE_*.md` - Pyodide integration guides
  - `BROWSER_*.md` - Browser deployment instructions
  - `docs/BROWSER_PYTHON_SOLUTION.md` - Browser Python runtime architecture

## Why Archived

### Performance Issues
- **10x slower**: Pyodide adds significant overhead compared to native Python
- **100x slower**: Than Rust+PyO3 runtime (1000x vs native!)
- **Warm-up time**: Pyodide takes ~2-5 seconds to initialize
- **Memory usage**: Much higher than native runtime

### Maintenance Burden
- **Multiple WASI SDK versions**: Need to maintain 24.0 and 27.0
- **Platform-specific builds**: Separate toolchains for Windows/Linux
- **Pyodide updates**: Frequent breaking changes in Pyodide releases
- **WASM ecosystem churn**: Fast-moving target, constant updates needed

### Production Viability
- **Not production-ready**: Performance too poor for real-time audio
- **Limited use case**: Browser-only deployment (not required currently)
- **Better alternatives**: v0.2.0 Rust+PyO3 runtime provides superior performance
- **WebRTC works natively**: No need for browser-based audio processing

### Strategic Decision
The v0.2.0 release focused on production-ready Rust+PyO3 runtime:
- **72x speedup**: Real measured performance improvement
- **<10ms latency**: Fast enough for real-time WebRTC audio
- **Zero overhead**: No interpreter or WASM layer
- **Production proven**: Used in actual deployments

## When to Restore

Consider restoring this component if:

1. **Browser deployment required**: Need client-side audio processing in browsers
2. **Sandboxed execution**: WASM provides security isolation
3. **Offline web apps**: Process audio locally without server
4. **Edge computing**: Run processing in browser/edge devices
5. **Client-side ML**: Browser-based model inference needed

## Performance Comparison

| Runtime | Init Time | Processing Time | Memory | Use Case |
|---------|-----------|-----------------|--------|----------|
| Rust+PyO3 | <1ms | 5ms | 4MB | ✅ Production |
| Native Python | ~1.8s (first) | 5ms (warm) | 140MB | Development |
| Pyodide/WASM | ~3s | 50ms+ | 200MB+ | ❌ Not viable |

## Restoration Instructions

### Prerequisites
1. Check dependencies have not changed significantly
2. Verify Pyodide version compatibility (was using latest stable)
3. Ensure WASI SDK toolchains still compatible with current Rust

### Step 1: Restore Files
```bash
# Create a restoration branch
git checkout -b restore-wasm-runtime

# Copy back from archive
cp -r archive/wasm-browser-runtime/browser-demo/ ./
cp -r archive/wasm-browser-runtime/docs/* docs/

# WASI SDKs are in .gitignore, download fresh versions
# (Don't copy from archive - too large)
```

### Step 2: Download Fresh WASI SDKs
```bash
# Linux
wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-27/wasi-sdk-27.0-x86_64-linux.tar.gz
tar xvf wasi-sdk-27.0-x86_64-linux.tar.gz

# Windows
# Download from: https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-27/wasi-sdk-27.0-x86_64-windows.tar.gz
# Extract to project root
```

### Step 3: Update Dependencies
```bash
# Update Pyodide version in browser-demo/package.json
cd browser-demo
npm install
npm update pyodide

# Update WASM build configuration
# Check wasi-toolchain.cmake for path updates
```

### Step 4: Build and Test
```bash
# Build browser demo
cd browser-demo
npm run build

# Test in browser
npm run dev
# Open http://localhost:5173

# Verify audio processing works
# Check browser console for errors
```

### Step 5: Update API Compatibility
The v0.2.0 API has changed since archival:
- Update `AudioTransform` → `AudioResampleNode`
- Add `runtime_hint` parameter support
- Update node initialization patterns

See `docs/MIGRATION_GUIDE.md` for v0.2.0 API changes.

### Step 6: Documentation
```bash
# Update README.md to mention WASM runtime option
# Update docs/DEPLOYMENT.md with browser deployment steps
# Add performance caveats to documentation
```

## Technical Details

### Architecture
```
Browser
  └─── Pyodide (Python in WASM)
        └─── RemoteMedia Python SDK
              └─── WASM-compiled nodes (optional)
                    └─── Audio processing
```

### Key Components
- **Pyodide**: Python interpreter compiled to WASM
- **WASI SDK**: Toolchain for compiling to WASM with system interface
- **Browser Demo**: Vite-based web application
- **WASM Nodes**: Optional WASM-compiled audio nodes (experimental)

### Limitations
- No access to native libraries (librosa, soundfile, etc.)
- Limited filesystem access (virtual FS only)
- No threading (WASM is single-threaded)
- High memory usage (entire Python runtime in browser)
- Slow initialization (load Python + packages)

## Related Documentation

- `docs/BROWSER_PYTHON_SOLUTION.md` - Original architecture document
- `browser-demo/README.md` - Demo application setup
- `specs/001-native-rust-acceleration/spec.md` - Why Rust+PyO3 was chosen

## Questions?

For questions about this archived component:
1. Review git history: `git log --follow --all -- archive/wasm-browser-runtime/`
2. Check original documentation in this directory
3. Consult `docs/ARCHIVAL_GUIDE.md` for general restoration guidance
4. Open GitHub issue with `archival` label

---

**Note**: This component was archived in v0.2.1 to focus on production-ready Rust+PyO3 runtime. It remains fully functional and can be restored if browser deployment becomes a requirement.
