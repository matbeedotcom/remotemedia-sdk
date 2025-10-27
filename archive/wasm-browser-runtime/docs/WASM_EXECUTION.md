# WASM Execution Guide

## Overview

The RemoteMedia SDK supports executing pipelines in WebAssembly (WASM) environments using embedded CPython via PyO3. This document explains the differences between native and WASM execution modes.

## Execution Modes

### Native Execution (Async)

Native execution uses Tokio's async runtime for full async/await support:

```rust
// Native async execution
let executor = Executor::new();
let result = executor.execute(&manifest).await?;
```

**Features:**
- Full tokio async runtime
- Concurrent task execution
- Async I/O operations
- Python async/await support via `pyo3-async-runtimes`
- Zero-copy numpy arrays via `rust-numpy`

### WASM Execution (Sync)

WASM execution uses synchronous blocking execution via `futures::executor::block_on`:

```rust
// WASM synchronous execution
let executor = Executor::new();
let result = executor.execute_sync(&manifest)?;
```

**Features:**
- Uses `futures::executor::block_on` internally
- Limited tokio features (sync, macros, io-util, rt, time)
- No async Python support (no `pyo3-async-runtimes`)
- JSON/base64 numpy serialization (no zero-copy)
- Embedded CPython 3.12 via `libpython3.12.a`

## Sync vs Async Differences

### Method Signatures

| Native | WASM |
|--------|------|
| `async fn execute(&self, manifest: &Manifest) -> Result<ExecutionResult>` | `fn execute_sync(&self, manifest: &Manifest) -> Result<ExecutionResult>` |
| `async fn execute_with_input(&self, manifest: &Manifest, input_data: Vec<Value>) -> Result<ExecutionResult>` | `fn execute_with_input_sync(&self, manifest: &Manifest, input_data: Vec<Value>) -> Result<ExecutionResult>` |

### Implementation

The WASM `execute_sync()` methods wrap the async methods using `futures::executor::block_on`:

```rust
#[cfg(target_family = "wasm")]
pub fn execute_sync(&self, manifest: &Manifest) -> Result<ExecutionResult> {
    use futures::executor::block_on;
    block_on(self.execute(manifest))
}
```

### Why Synchronous Execution?

1. **Tokio Limitations**: WASM targets don't support all tokio features (no threading, limited I/O)
2. **Browser Integration**: Browsers use event loops, not OS threads
3. **Simplicity**: Synchronous execution is easier to reason about in single-threaded WASM
4. **Compatibility**: Works with both WASI Command and browser WASM runtimes

### Performance Implications

| Aspect | Native | WASM |
|--------|--------|------|
| Pipeline Execution | 1.0x (baseline) | 1.2-1.5x slower |
| Startup Time | <100ms | ~500ms (one-time) |
| Memory Usage | baseline | +10-20% overhead |
| Concurrency | Full async/await | Limited (simulated via blocking) |

## Conditional Compilation

The codebase uses feature flags to enable WASM-specific code:

```rust
// Only compile for WASM targets
#[cfg(target_family = "wasm")]
pub fn execute_sync(&self, manifest: &Manifest) -> Result<ExecutionResult> {
    // ...
}

// Only include for native builds
#[cfg(not(target_family = "wasm"))]
use pyo3_async_runtimes::tokio::future_into_py;
```

## Data Marshaling Differences

### Native (Zero-Copy)
```rust
// Native: Use rust-numpy for zero-copy
#[cfg(feature = "native-numpy")]
use numpy::PyArray1;
```

### WASM (JSON/Base64)
```rust
// WASM: Serialize to JSON + base64
#[cfg(target_family = "wasm")]
fn serialize_numpy_wasm(array: &[f32]) -> Value {
    let base64_data = base64::encode(bytemuck::cast_slice(array));
    json!({
        "dtype": "float32",
        "shape": [array.len()],
        "data": base64_data
    })
}
```

## Feature Flags

The WASM build uses specific feature flags:

```toml
[features]
wasm = []
python-async = ["pyo3-async-runtimes"]
native-numpy = ["numpy"]
grpc-transport = ["tonic", "prost"]
```

Build WASM binary:
```bash
cargo build --target wasm32-wasip1 \
    --bin pipeline_executor_wasm \
    --no-default-features \
    --features wasm
```

Build native binary:
```bash
cargo build \
    --features python-async,native-numpy,grpc-transport
```

## WASM Binary Entry Point

The WASM binary (`pipeline_executor_wasm.wasm`) provides a WASI Command interface:

```rust
fn main() {
    // Initialize PyO3
    pyo3::prepare_freethreaded_python();

    // Read manifest from stdin
    let manifest_json = read_stdin()?;

    // Execute synchronously
    let executor = Executor::new();
    let result = executor.execute_sync(&manifest)?;

    // Write results to stdout
    println!("{}", serde_json::to_string_pretty(&result)?);
}
```

## Usage Examples

### Native (Async)
```rust
use remotemedia_runtime::executor::Executor;

#[tokio::main]
async fn main() -> Result<()> {
    let manifest = load_manifest("pipeline.json")?;
    let executor = Executor::new();
    let result = executor.execute(&manifest).await?;
    println!("Result: {:?}", result);
    Ok(())
}
```

### WASM (Sync)
```rust
use remotemedia_runtime::executor::Executor;

fn main() -> Result<()> {
    let manifest = load_manifest("pipeline.json")?;
    let executor = Executor::new();
    let result = executor.execute_sync(&manifest)?;
    println!("Result: {:?}", result);
    Ok(())
}
```

## Browser Integration

When running in the browser, use Wasmer SDK:

```typescript
import { init, Wasmer } from "@wasmer/sdk";

await init();

const pkg = await Wasmer.fromFile("pipeline_executor.wasm");
const instance = await pkg.entrypoint.run({
    args: [],
    stdin: JSON.stringify(manifest)
});

const result = await instance.wait();
console.log(result.stdout);
```

## Limitations

### WASM Execution Limitations

1. **No Native Threading**: Single-threaded execution only
2. **No Async Python**: Cannot use `async def` Python nodes
3. **No Zero-Copy Numpy**: Must serialize/deserialize numpy arrays
4. **Limited Concurrency**: Simulated via `block_on`, not true parallelism
5. **No gRPC Transport**: Cannot use gRPC for remote nodes (WASM limitation)

### Workarounds

- Use synchronous Python nodes only
- Accept serialization overhead for numpy arrays
- Use linear pipelines for best performance
- Limit pipeline complexity to avoid stack overflow

## Testing

### Native Tests
```bash
cargo test
```

### WASM Tests (with Wasmtime)
```bash
# Install wasmtime
curl https://wasmtime.dev/install.sh -sSf | bash

# Build WASM binary
cargo build --target wasm32-wasip1 --bin pipeline_executor_wasm --features wasm

# Run with test manifest
echo '{"version":"v1","metadata":{"name":"test"},"nodes":[],"connections":[]}' | \
    wasmtime target/wasm32-wasip1/debug/pipeline_executor_wasm.wasm
```

## Performance Tuning

### WASM Build Optimization

1. **Release Build**:
```bash
cargo build --target wasm32-wasip1 --release --bin pipeline_executor_wasm --features wasm
```

2. **wasm-opt** (from binaryen):
```bash
wasm-opt -O3 -o optimized.wasm pipeline_executor_wasm.wasm
```

3. **Strip Debug Info**:
```bash
wasm-strip pipeline_executor_wasm.wasm
```

Expected sizes:
- Debug: ~131 MB
- Release: ~15-20 MB
- Optimized: ~10-15 MB

## Troubleshooting

### Common Issues

**Issue**: `execute_sync` not found
**Solution**: Ensure you're building for `wasm32-wasip1` target

**Issue**: Binary too large
**Solution**: Use release build + wasm-opt

**Issue**: Slow execution
**Solution**: Use linear pipelines, avoid complex DAGs

**Issue**: Memory errors
**Solution**: Reduce pipeline complexity, use smaller batch sizes

## References

- [PyO3 WASM Guide](https://pyo3.rs/v0.26.0/building-and-distribution.html#wasm)
- [Tokio WASM Support](https://tokio.rs/tokio/topics/wasm)
- [futures::executor::block_on](https://docs.rs/futures/latest/futures/executor/fn.block_on.html)
- [WASI Preview 1](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md)
