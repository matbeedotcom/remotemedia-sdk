# ⚠️ DEPRECATION NOTICE: `runtime/` crate

**Status:** LEGACY - Consider this crate deprecated as of v0.4.0

**Date:** 2025-01-07

## Summary

The `runtime/` crate is **legacy code** that has been superseded by the properly decoupled architecture:
- **`runtime-core/`** - Core runtime with zero transport dependencies
- **`transports/`** - Independent transport implementations

## Why is this deprecated?

The `runtime/` crate has **embedded protobuf types** throughout its codebase, creating tight coupling with gRPC transport:

```rust
// ❌ Legacy runtime/ code
use crate::grpc_service::generated::AudioBuffer;  // Protobuf type!
use crate::grpc_service::generated::VideoFrame;   // Protobuf type!
```

This means the runtime cannot be built without gRPC/protobuf dependencies, violating the transport decoupling goal.

The **properly decoupled** architecture uses transport-agnostic types:

```rust
// ✅ runtime-core code
use crate::data::RuntimeData;      // Transport-agnostic!
use crate::transport::TransportData;  // Transport-agnostic!
```

## Migration Path

### If you're using gRPC transport:

**OLD (v0.3.x):**
```rust
use remotemedia_runtime::grpc_service::server::RemoteMediaServer;
```

**NEW (v0.4.0+):**
```rust
use remotemedia_grpc::server::RemoteMediaServer;
```

### If you're building custom transports:

**OLD (v0.3.x):**
```rust
// No clean way to do this - had to depend on full runtime with gRPC
```

**NEW (v0.4.0+):**
```rust
use remotemedia_runtime_core::transport::PipelineTransport;
use remotemedia_runtime_core::transport::PipelineRunner;

struct MyCustomTransport { /* ... */ }

#[async_trait]
impl PipelineTransport for MyCustomTransport {
    // Implement trait methods
}
```

### If you're using Python SDK:

**OLD (v0.3.x):**
```python
from remotemedia_runtime import execute_pipeline
```

**NEW (v0.4.0+):**
```python
from remotemedia_ffi import execute_pipeline
```

## What should you use instead?

| Use Case | Use This | Not This |
|----------|----------|----------|
| Core runtime logic | `runtime-core/` | ~~`runtime/`~~ |
| gRPC server | `transports/remotemedia-grpc/` | ~~`runtime/` with grpc feature~~ |
| Python FFI | `transports/remotemedia-ffi/` | ~~`runtime/` with python feature~~ |
| Custom transport | Implement `PipelineTransport` from `runtime-core` | ~~Fork `runtime/`~~ |
| Browser/WASM | See note below | `runtime/src/bin/pipeline_executor_wasm.rs` (temporary) |

## What about WASM support?

The WASM binary (`runtime/src/bin/pipeline_executor_wasm.rs`) currently lives in this crate. This is the **one exception** where `runtime/` is still needed temporarily.

**Planned migration:** The WASM binary should be moved to a separate `wasm-executor/` crate or integrated with `runtime-core`.

## What functionality exists only in runtime/?

The following modules exist in `runtime/` but not (yet) in `runtime-core/`:

- **`cache/`** - Caching layer
- **`capabilities/`** - Runtime capability detection
- **`registry/`** - Node registry implementation
- **`wasm/`** - WASM execution support
- **`bin/`** - Utility binaries (analyze_pipeline, detect_capabilities, pipeline_executor_wasm)

If you need these, they may need to be migrated to `runtime-core` or separate crates.

## Timeline

- **v0.4.0 (current):** `runtime/` marked as legacy, prefer `runtime-core` + transports
- **v0.5.0 (future):** Move WASM binary to separate crate
- **v0.6.0 (future):** Consider removing `runtime/` entirely or making it a thin compatibility shim

## Build Status

The `runtime/` crate **will not build** without gRPC dependencies due to embedded protobuf types:

```bash
# ❌ This will fail
cd runtime
cargo build --no-default-features --features multiprocess

# Error: cannot find `grpc_service` in the crate root
# Error: use of undeclared crate or module `prost`
```

This is **expected behavior** - the crate is tightly coupled to gRPC and cannot be easily decoupled without significant refactoring.

## If you must use runtime/:

1. **For WASM:** Use `cargo build --target wasm32-wasip1 --bin pipeline_executor_wasm --features wasm`
2. **For existing projects:** Migrate to `runtime-core` + transport crates when possible
3. **For new projects:** Start with `runtime-core` + appropriate transport crates

## Questions?

See:
- [LEGACY_ARCHIVE_SUMMARY.md](../LEGACY_ARCHIVE_SUMMARY.md) - Overview of legacy code
- [archive/ARCHIVE.md](../archive/ARCHIVE.md) - Archived transport code
- [docs/MIGRATION_GUIDE_v0.3_to_v0.4.md](../docs/MIGRATION_GUIDE_v0.3_to_v0.4.md) - Migration guide

## For Contributors

**DO:**
- ✅ Use `runtime-core/` for new core runtime features
- ✅ Use `transports/remotemedia-grpc/` for gRPC work
- ✅ Use `transports/remotemedia-ffi/` for Python FFI work
- ✅ Migrate functionality from `runtime/` to `runtime-core/` when practical

**DON'T:**
- ❌ Add new features to `runtime/`
- ❌ Try to "fix" the protobuf coupling in `runtime/`
- ❌ Use `runtime/` as an example for new code
