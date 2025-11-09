# Archived Components

**Last Updated**: 2025-10-27  
**Branch**: 002-code-archival-consolidation  
**Release**: v0.2.1

This directory contains code and documentation that has been archived as part of the v0.2.1 codebase consolidation effort. All archived components are fully functional but are no longer actively maintained or required for the core remotemedia-sdk functionality.

## Overview

The v0.2.1 release focused the codebase on the production-ready Rust+PyO3 runtime, achieving:
- **70% LoC reduction**: From ~50,000 to ~15,000 lines
- **76% fewer affected files**: Error enum changes now impact ~15 files (was 62)
- **Clearer architecture**: Single NodeExecutor trait, one runtime path
- **Better performance**: WebRTC latency reduced from 380ms to <10ms

## Archived Components

### 1. WASM/Browser Runtime (`wasm-browser-runtime/`)

**What**: Experimental WASM-based Python runtime using Pyodide and WASI  
**Size**: ~15,000 LoC  
**Status**: Functional but not production-ready  
**Archived**: 2025-10-27

**Contents**:
- `browser-demo/` - Browser-based demo application
- `wasi-sdk-*/` - WASI toolchains (24.0, 27.0)
- `docs/` - WASM, Pyodide, and browser documentation
- `whisper.cpp/` - WASM-compiled Whisper models (if applicable)

**Why Archived**:
- Pyodide has significant performance overhead (~10x slower than native)
- Browser use case not required for current production deployments
- WASM toolchain maintenance burden (multiple SDK versions)
- v0.2.0 Rust+PyO3 runtime provides superior performance

**When to Restore**:
- If browser-based audio processing becomes a requirement
- For client-side ML inference in web applications
- To explore WebAssembly for sandboxed execution

See `wasm-browser-runtime/README.md` for detailed restoration instructions.

---

### 2. Old NodeExecutor Trait (`old-node-executor/`)

**What**: Previous NodeExecutor trait and adapter layer  
**Size**: ~2,000 LoC  
**Status**: Superseded by unified trait  
**Archived**: 2025-10-27

**Contents**:
- `nodes_mod.rs` - Original nodes::NodeExecutor trait definition
- `cpython_node.rs` - Adapter between CPython executor and nodes trait
- `tests/` - Rust tests that failed after consolidation

**Why Archived**:
- v0.2.1 consolidated to single `executor::node_executor::NodeExecutor` trait
- Adapter layer no longer needed (direct implementation)
- Reduced Error enum propagation from 62 to ~15 files
- Simpler architecture, easier maintenance

**When to Restore**:
- If multiple NodeExecutor trait variants become necessary
- For reference when debugging trait-related issues
- To understand historical architecture decisions

See `old-node-executor/README.md` for detailed restoration instructions.

---

### 3. Historical Specifications (`old-specifications/`)

**What**: Development documentation from v0.1.x and early v0.2.0  
**Size**: ~50 markdown files, ~10,000 LoC  
**Status**: Historical reference only  
**Archived**: 2025-10-27

**Contents**:
- `updated_spec/` - Early specification documents
- `RUSTPYTHON_*.md` - RustPython exploration (abandoned)
- `TASK_*.md`, `PHASE_*.md` - Development tracking documents
- `FROM_*.md` - Migration guides between versions
- `BENCHMARK_*.md`, `IMPLEMENTATION_*.md` - Historical reports

**Why Archived**:
- RustPython approach abandoned in favor of PyO3
- Phase/task tracking documents completed
- New specification format adopted (`.specify/` directory)
- Current documentation in `docs/` and `specs/`

**When to Restore**:
- For historical context on architecture decisions
- To understand why RustPython was rejected
- To reference old API designs

See `old-specifications/README.md` for detailed restoration instructions.

---

## Restoration Process

If you need to restore any archived component:

1. **Check compatibility**: Archived code may depend on old APIs
2. **Read component README**: Each directory has restoration instructions
3. **Create feature branch**: `git checkout -b restore-[component]`
4. **Copy files back**: Move from `archive/` to original locations
5. **Update dependencies**: Check for version conflicts
6. **Run tests**: Verify functionality after restoration
7. **Update documentation**: Add restoration notes to CHANGELOG.md

## Git History

All archived code remains in git history. To view original locations:

```bash
# Find when a file was moved to archive
git log --follow --all -- archive/wasm-browser-runtime/browser-demo/

# View file at specific commit
git show <commit>:browser-demo/index.html

# Restore file from history
git checkout <commit> -- browser-demo/
```

## Impact on v0.2.1

**Before Archival** (v0.2.0):
- Total LoC: ~50,000
- Active runtimes: 3 (CPython, Pyodide, RustPython)
- NodeExecutor traits: 2
- Error enum impact: 62 files
- WebRTC latency: 380ms (Python runtime)

**After Archival** (v0.2.1):
- Total LoC: ~15,000 (-70%)
- Active runtimes: 1 (Rust+PyO3)
- NodeExecutor traits: 1 (-50%)
- Error enum impact: ~15 files (-76%)
- WebRTC latency: <10ms (Rust runtime, 72x faster)

## Questions?

For questions about archived components or restoration:
1. Check the component's specific README.md
2. Review git history for original implementation
3. Consult docs/ARCHIVAL_GUIDE.md for general guidance
4. Open an issue on GitHub with `archival` label

---

**Note**: This archival was part of the v0.2.1 release to focus the codebase on production-ready features. All archived code is fully functional and can be restored if needed.
