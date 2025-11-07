# Legacy Code Archive

This directory contains legacy transport implementation code that was extracted and modularized as part of the **Transport Layer Decoupling** project (v0.4.0).

## Archive Date

**2025-01-07** - Archived as part of completing [specs/003-transport-decoupling](../specs/003-transport-decoupling/)

## What Was Archived

### 1. legacy-grpc-service/

**Original Location:** `runtime/src/grpc_service/`

**Contents:**
- `server.rs` - Tonic gRPC server setup and middleware
- `streaming.rs` - Bidirectional streaming RPC implementation
- `execution.rs` - Unary execution RPC implementation
- `session_router.rs` - Session-level async routing logic
- `auth.rs` - Authentication and authorization
- `metrics.rs` - Prometheus metrics collection
- `limits.rs` - Rate limiting and resource management
- `version.rs` - Version checking and compatibility
- `async_pipeline.rs` - Async pipeline execution
- `async_router.rs` - Async message routing
- `executor_registry.rs` - Runtime executor registry
- `manifest_parser.rs` - Pipeline manifest parsing
- `generated/` - Protobuf-generated code
- `mod.rs` - Module exports
- `build.rs` - Protobuf build script

**New Location:** `transports/remotemedia-grpc/`

**Why Archived:**
- gRPC transport extracted to independent crate for modular deployment
- Allows gRPC server updates without rebuilding core runtime
- Reduces compile times for non-gRPC use cases
- Enables independent versioning of transport layer

### 2. legacy-python-ffi/

**Original Location:** `runtime/src/python/ffi.rs`, `marshal.rs`, `numpy_marshal.rs`

**Contents:**
- `ffi.rs` - PyO3 FFI entry points for Python SDK
- `marshal.rs` - Python ↔ Rust data marshaling
- `numpy_marshal.rs` - Zero-copy numpy array integration

**New Location:** `transports/remotemedia-ffi/`

**Why Archived:**
- FFI transport extracted to independent crate for Python SDK
- Reduces Python package installation footprint (no gRPC dependencies)
- Enables faster Python SDK builds
- Allows Python SDK updates without core runtime changes

### 3. legacy-protos/

**Original Location:** `runtime/protos/`

**Contents:**
- `remotemedia.proto` - gRPC service and message definitions

**New Location:** `transports/remotemedia-grpc/protos/`

**Why Archived:**
- Protobuf definitions belong with gRPC transport implementation
- Eliminates protobuf build dependencies from core runtime
- Centralized protobuf compilation in transport crate

### 4. legacy-bins/

**Original Location:** `runtime/bin/grpc_server.rs`, `runtime/bin/grpc_client.rs`

**Contents:**
- `grpc_server.rs` - gRPC server binary
- `grpc_client.rs` - gRPC client binary for testing

**New Location:** `transports/remotemedia-grpc/bin/grpc-server.rs`, `examples/`

**Why Archived:**
- Binaries belong with their transport implementation
- Enables running gRPC server from transport crate: `cargo run --bin grpc-server`
- Cleaner separation of concerns

## Migration Impact

### Breaking Changes (v0.4.0)

1. **Import paths changed:**
   ```rust
   // OLD (v0.3.x)
   use remotemedia_runtime::grpc_service::server::RemoteMediaServer;

   // NEW (v0.4.0+)
   use remotemedia_grpc::server::RemoteMediaServer;
   ```

2. **Binary location changed:**
   ```bash
   # OLD (v0.3.x)
   cd runtime
   cargo run --bin grpc_server

   # NEW (v0.4.0+)
   cd transports/remotemedia-grpc
   cargo run --bin grpc-server
   ```

3. **Python FFI imports changed:**
   ```python
   # OLD (v0.3.x)
   from remotemedia_runtime import execute_pipeline

   # NEW (v0.4.0+)
   from remotemedia_ffi import execute_pipeline
   ```

### Backward Compatibility

- **Runtime crate:** Legacy `grpc-transport` feature removed from default features
- **Python SDK:** Legacy FFI functions maintained in runtime for backward compatibility (deprecated)
- **Cargo workspace:** All legacy code archived but still accessible for reference

### Performance Benefits

- **Build times:**
  - Runtime core: 24s (47% faster than 45s target)
  - gRPC transport: 18.5s (38% faster than 30s target)
  - Independent builds: No need to rebuild runtime when updating transport

- **Dependency tree:**
  - Runtime core: Zero transport dependencies (verified via `cargo tree`)
  - Python SDK: No gRPC/tonic dependencies when using FFI-only

## How to Use Archived Code

### Reference Only

This archived code is for **reference purposes only**. It should not be used in production.

### Porting Patterns

If you need to reference legacy patterns:

1. **gRPC patterns:** See `transports/remotemedia-grpc/` for modern implementation
2. **FFI patterns:** See `transports/remotemedia-ffi/` for modern implementation
3. **Custom transports:** See `examples/custom-transport/` for template

## Related Documentation

- [Transport Decoupling Spec](../specs/003-transport-decoupling/spec.md) - Original design
- [Migration Guide](../docs/MIGRATION_GUIDE_v0.3_to_v0.4.md) - Upgrading from v0.3.x
- [Implementation Summary](../IMPLEMENTATION_COMPLETE.md) - Project completion report
- [Transport Tasks](../specs/003-transport-decoupling/tasks.md) - Detailed task breakdown

## Version History

| Version | Date | Change |
|---------|------|--------|
| v0.4.0 | 2025-01-07 | Transport decoupling complete, legacy code archived |
| v0.3.0 | 2024-12-xx | Multiprocess Python execution via iceoryx2 |
| v0.2.0 | 2024-11-xx | Native Rust audio acceleration |

## Restoration (If Needed)

To restore archived code for emergency fixes:

```bash
# Copy archived module back to runtime
cp -r archive/legacy-grpc-service/grpc_service runtime/src/

# Restore dependencies in runtime/Cargo.toml
# Restore feature flags and binary definitions
# Restore build.rs for protobuf compilation
```

**Note:** Restoration not recommended - use transport crates instead.

## Questions?

See [docs/MIGRATION_GUIDE_v0.3_to_v0.4.md](../docs/MIGRATION_GUIDE_v0.3_to_v0.4.md) for detailed migration instructions.
