# Code Archival Guide

**Version**: v0.2.1  
**Date**: 2025-10-27  
**Purpose**: Document archived components and restoration procedures

---

## Overview

As part of the v0.2.1 release, we archived unused code to reduce maintenance burden and focus on core functionality. This guide explains what was archived, why, and how to restore components if needed.

## What Was Archived

### 1. WASM/Pyodide Browser Runtime

**Location**: `archive/wasm-browser-runtime/`  
**Size**: ~15,000 LoC  
**Archived**: 2025-10-27

#### What It Was

Browser-based Python execution using Pyodide and WebAssembly. Allowed running RemoteMedia pipelines directly in web browsers without a backend server.

**Key Components**:
- `browser-demo/` - Complete TypeScript demo application
- `wasi-sdk-*/` - WebAssembly System Interface toolchains
- Documentation - WASM_*.md, PYODIDE_*.md, BROWSER_*.md

#### Why Archived

1. **No Production Use**: Zero users deployed browser runtime in production
2. **Performance**: PyO3 native runtime provides 72x better performance
3. **Complexity**: WASM toolchain adds significant build complexity
4. **Maintenance**: Cross-platform WASM builds require ongoing effort
5. **Alternative Exists**: WebRTC server (kept) provides real-time processing

**Performance Comparison**:
- Pyodide: ~10x slower than native Python (interpreter overhead)
- Native Rust: ~72x faster than Python
- **Gap**: Pyodide is ~720x slower than Rust native!

#### When to Restore

Consider restoring if you need:
- ✅ Offline web applications (no server required)
- ✅ Browser-only execution (sandboxed environment)
- ✅ Client-side ML inference (privacy-sensitive data)
- ❌ Real-time performance (use WebRTC server instead)
- ❌ Production ML workloads (use native runtime instead)

#### How to Restore

See `archive/wasm-browser-runtime/README.md` for detailed restoration instructions.

---

### 2. Old NodeExecutor Trait & Adapter

**Location**: `archive/old-node-executor/`  
**Size**: ~2,000 LoC  
**Archived**: 2025-10-27

#### What It Was

Dual NodeExecutor trait architecture with adapter layer:
- `nodes::NodeExecutor` - Original trait used by audio nodes
- `executor::node_executor::NodeExecutor` - Python executor trait
- `CPythonNodeAdapter` - Bridged between the two traits

#### Why Archived

1. **Complexity**: Two traits doing the same thing caused confusion
2. **Maintenance**: Changes required updating both traits
3. **Error Handling**: Error enum changes affected 62 files (now 15)
4. **No Value**: Adapter was pure boilerplate with no logic

**Impact**:
- **Before**: Error enum changes → 62 files affected
- **After**: Error enum changes → 15 files affected (-76%)

#### Current Architecture

**Two Traits Remain** (for now):
1. `nodes::NodeExecutor` - Audio nodes (resample, VAD, format)
2. `executor::node_executor::NodeExecutor` - Python executor

**Adapter Removed**:
- `CPythonNodeAdapter` no longer needed
- Direct trait implementation works fine

**Future**: Full consolidation to single trait planned for v0.3.0

#### When to Restore

**Do NOT restore**. This is historical reference only. The adapter was pure overhead with no benefit.

If you're working on trait consolidation (v0.3.0), see migration guide in `archive/old-node-executor/README.md`.

---

### 3. Old Specification Documents

**Location**: `archive/old-specifications/`  
**Size**: ~10,000 LoC (markdown)  
**Archived**: 2025-10-27

#### What It Was

Historical specification and planning documents from v0.1.x through v0.2.0:
- RustPython exploration docs (`RUSTPYTHON_*.md`)
- Old task tracking (`TASK_*.md`, `FROM_*.md`)
- Phase completion reports (`PHASE_*.md`)
- Benchmark results from early development
- Old specification format (`updated_spec/`)

#### Why Archived

1. **Superseded**: v0.2.0 OpenSpec format is canonical
2. **Historical**: Useful for understanding decisions, not active work
3. **Clutter**: 50+ old markdown files in root directory
4. **Misleading**: Old specs don't reflect current architecture

#### When to Restore

**Do NOT restore to active codebase**. Access via archive for:
- Understanding historical decisions
- Seeing evolution of architecture
- Reference for "why we didn't do X"

All information is preserved in git history and archive directory.

---

## Impact Summary

### LoC Reduction

| Component | Before | After | Reduction |
|-----------|--------|-------|-----------|
| **WASM/Browser** | ~15,000 | 0 | -100% |
| **NodeExecutor** | ~2,000 | 0 | -100% |
| **Old Specs** | ~10,000 | 0 | -100% |
| **Total Archived** | ~27,000 | 0 | **-27,000 LoC** |
| **Remaining Active** | ~50,000 | ~23,000 | **-54% total** |

### Maintenance Reduction

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| **Active Runtimes** | 3 (Native, WASM, Pyodide) | 1 (Native) | -66% |
| **NodeExecutor Traits** | 2 + adapter | 2 (documented) | -1 file |
| **Error Enum Impact** | 62 files | 15 files | -76% |
| **Build Targets** | 3 (native, wasm32, wasi) | 1 (native) | -66% |
| **Test Suites** | Multiple (WASM, native) | 1 (native) | -50% effort |

### What Was Kept

✅ **Rust Runtime (PyO3)** - Core v0.2.0 feature (72x speedup)  
✅ **WebRTC Server** - Production use case (real-time audio)  
✅ **Audio Nodes** - Resample, VAD, Format (Rust native)  
✅ **Python SDK** - User-facing API with runtime_hint  
✅ **All Passing Tests** - 15/15 compatibility tests  
✅ **Examples** - Updated to v0.2.0 API

---

## Restoration Process

### General Steps

1. **Check Archive README**: Each archived component has detailed README.md
2. **Review Git History**: `git log --all --follow <archived-file-path>`
3. **Copy Back**: `git mv archive/<component>/ <original-location>/`
4. **Update Dependencies**: May need to reinstall toolchains/packages
5. **Update to v0.2.0 API**: Archived code uses old API
6. **Test Thoroughly**: Ensure compatibility with current codebase

### WASM/Browser Runtime Restoration

See `archive/wasm-browser-runtime/README.md` for:
- WASI SDK installation (600MB download)
- Node.js dependencies (npm install)
- Browser compatibility requirements
- API migration guide (v0.1.x → v0.2.0)
- Build instructions (TypeScript + Vite)

**Estimated Effort**: 2-3 days to restore and update

### NodeExecutor Consolidation (v0.3.0)

See `archive/old-node-executor/README.md` for:
- Trait comparison table
- Files affected by consolidation
- Migration strategy
- Testing checklist
- Rollback plan

**Estimated Effort**: 1-2 weeks for full consolidation

---

## FAQs

### Q: Will archived code work if restored?

**A**: Code is functional but uses v0.1.x API. You'll need to:
1. Update imports (AudioTransform → AudioResampleNode)
2. Add runtime_hint parameters
3. Update error handling
4. Re-test everything

### Q: Can I access archived code history?

**A**: Yes! Two ways:
1. Browse `archive/` directory (most recent archived state)
2. Git history: `git log --all -- <original-path>`

### Q: Why not just delete?

**A**: Archival provides:
- Quick restoration if needed
- Clear documentation of decisions
- Reference for future features
- Lower risk than deletion

### Q: What if I need browser execution?

**A**: Options:
1. Restore WASM runtime (see guide above)
2. Use WebRTC server (recommended - 72x faster!)
3. Server-side rendering (no client-side execution)

### Q: Will archival break my code?

**A**: No! Changes are internal only:
- Python SDK API unchanged
- All tests pass (15/15)
- Examples work without modification
- Zero breaking changes for users

---

## Version History

| Version | Date | Change | Impact |
|---------|------|--------|--------|
| **v0.2.1** | 2025-10-27 | Initial archival | -54% LoC |
| Future | TBD | NodeExecutor consolidation (v0.3.0) | Single trait |

---

## Support

**Questions about archived components?**
- Check component-specific README in archive/
- Review git history for context
- Ask in GitHub issues if restoration needed

**Found a bug in archived code?**
- We don't maintain archived code
- Consider using active alternatives instead
- Restoration PRs welcome if there's clear use case

---

**Last Updated**: 2025-10-27  
**Maintained By**: RemoteMedia SDK Team
