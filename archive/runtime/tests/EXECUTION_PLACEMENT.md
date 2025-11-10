# Execution Placement Strategy

## Overview

The capability detection system supports granular execution placement based on **what each node requires** rather than where it's restricted to run.

## Key Concept

Naming is based on **requirements**, not restrictions:

- `Anywhere` - Pure WASM, **can run everywhere** (browser, server WASM, native)
- `RequiresWasi` - Needs WASI APIs, **can run in server WASM or native** (not browser)
- `RequiresNative` - Needs native execution, **can only run on native host**
- `RequiresRemote` - Must use external executor

## Execution Placement Enum

```rust
pub enum ExecutionPlacement {
    // What the node requires
    Anywhere,         // Pure WASM (wasm32-unknown-unknown) - most portable
    RequiresWasi,     // WASI APIs (wasm32-wasip1) - server WASM or native
    RequiresNative,   // Native libs/C-extensions - native only
    RequiresRemote,   // External executor - GPU clusters, etc.

    // Fallback preferences
    PreferAnywhere,   // Try: anywhere → WASI → native → remote
    PreferWasi,       // Try: WASI → native → remote
    PreferNative,     // Try: native → remote
    PreferRemote,     // Try: remote → native

    Auto,             // System decides
}
```

## Placement in JSON Manifests

### Requirements-Based

```json
{
  "execution": {
    "placement": "anywhere"
  }
}
```

**Valid values:**
- `"anywhere"` → `Anywhere` (pure WASM, runs everywhere)
- `"wasi"` or `"requires_wasi"` → `RequiresWasi`
- `"native"` or `"requires_native"` → `RequiresNative`
- `"remote"` or `"requires_remote"` → `RequiresRemote`

### Preference-Based

```json
{
  "execution": {
    "placement": "prefer_anywhere"
  }
}
```

**Valid values:**
- `"prefer_anywhere"` → `PreferAnywhere`
- `"prefer_wasi"` → `PreferWasi`
- `"prefer_native"` → `PreferNative`
- `"prefer_remote"` → `PreferRemote`

### Legacy Support

Old names automatically mapped:
- `"browser"` or `"local"` → `Anywhere`
- `"prefer_browser"` or `"prefer_local"` → `PreferAnywhere`
- `"host"` → `RequiresNative`
- `"prefer_host"` → `PreferNative`

## What Can Run Where?

| Placement | Browser | Server WASM | Native Host | Remote |
|-----------|---------|-------------|-------------|--------|
| **Anywhere** | ✅ | ✅ | ✅ | ✅ |
| **RequiresWasi** | ❌ | ✅ | ✅ | ✅ |
| **RequiresNative** | ❌ | ❌ | ✅ | ✅ |
| **RequiresRemote** | ❌ | ❌ | ❌ | ✅ |

## Examples

### 1. Anywhere (Pure WASM)

```json
{
  "id": "calculator",
  "node_type": "SimpleCalculator",
  "execution": {"placement": "anywhere"}
}
```

**Compiles to:** `wasm32-unknown-unknown`
**Can run:** Browser, server WASM, native, remote - everywhere!

---

### 2. RequiresWasi (File I/O)

```json
{
  "id": "file-processor",
  "node_type": "FileProcessor",
  "params": {"input_file": "/data/input.txt"},
  "execution": {"placement": "wasi"}
}
```

**Compiles to:** `wasm32-wasip1`
**Can run:** Server WASM (wasmtime/Node.js), native
**Cannot run:** Browser (no WASI APIs)

---

### 3. RequiresNative (C-Extensions)

```json
{
  "id": "numpy-processor",
  "node_type": "NumpyProcessor",
  "execution": {"placement": "native"}
}
```

**Compiles to:** Native binary only
**Can run:** Native host
**Cannot run:** Browser, server WASM (won't compile to WASM)

---

### 4. RequiresRemote (GPU Cluster)

```json
{
  "id": "llm-inference",
  "node_type": "LLMInference",
  "params": {"device": "cuda"},
  "execution": {"placement": "remote"},
  "capabilities": {
    "gpu": {"type": "cuda", "required": true}
  }
}
```

**Must run:** External GPU cluster/server
**Why:** Requires specialized hardware not available locally

---

## Fallback Chains

### PreferAnywhere
Try in order:
1. Browser (pure WASM)
2. Server WASM
3. Native host
4. Remote executor

### PreferWasi
Try in order:
1. Server WASM (WASI)
2. Native host
3. Remote executor

### PreferNative
Try in order:
1. Native host
2. Remote executor

### PreferRemote
Try in order:
1. Remote executor
2. Native host (fallback)

## Detection Examples

### Pure Python (stdlib only)
```
✓ Python node uses only stdlib (Pyodide-compatible)
  Can run: browser (Pyodide), server, native
Placement: Anywhere
```

### Python with Numpy
```
✓ Python node requires native execution (C-extensions)
  Can run: native only
Placement: RequiresNative
```

### Rust Pure WASM
```
✓ Node compiles to pure WASM (wasm32-unknown-unknown)
  Can run: browser, server WASM, native
Placement: Anywhere
```

### Rust with WASI
```
✓ Node compiles to WASI WASM (wasm32-wasip1)
  Can run: server WASM, native (not browser)
Placement: RequiresWasi
```

## Summary

The naming now clearly communicates **what the node needs**, not where it's limited:

- ✅ **`Anywhere`** - No special requirements, maximum portability
- ✅ **`RequiresWasi`** - Needs WASI APIs (file I/O, etc.)
- ✅ **`RequiresNative`** - Needs native libraries or won't compile to WASM
- ✅ **`RequiresRemote`** - Needs specialized remote hardware

**All 11 tests passing** ✅
