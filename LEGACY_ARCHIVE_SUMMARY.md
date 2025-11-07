# Legacy Code Archive Summary

**Date:** 2025-01-07
**Action:** Archived legacy transport implementation code from v0.3.x

## What Was Archived

### Transport-Specific Code Moved to `archive/`

1. **`archive/legacy-grpc-service/`** - gRPC service implementation from `runtime/src/grpc_service/`
2. **`archive/legacy-python-ffi/`** - Python FFI code from `runtime/src/python/{ffi.rs, marshal.rs, numpy_marshal.rs}`
3. **`archive/legacy-protos/`** - Protobuf definitions from `runtime/protos/`
4. **`archive/legacy-bins/`** - gRPC server/client binaries from `runtime/bin/`
5. **`archive/legacy-grpc-service/build.rs`** - Protobuf build script

### Documentation Added

- **`archive/ARCHIVE.md`** - Comprehensive documentation of archived code
- **`README.md`** - Updated with archive directory structure
- **`runtime/Cargo.toml`** - Added comments about extracted transports
- **`runtime/src/lib.rs`** - Updated docs to reflect v0.4.0 architecture

## Current State

### ✅ Properly Decoupled (Use These)

**`runtime-core/`** - Core runtime with zero transport dependencies
- Uses `RuntimeData` and `TransportData` (transport-agnostic types)
- Provides `PipelineTransport` trait for custom transports
- Contains `PipelineRunner` for transport abstraction
- **ZERO dependencies on gRPC/protobuf/tonic**

**`transports/remotemedia-grpc/`** - Independent gRPC transport
- Implements `PipelineTransport` trait
- Uses protobuf for serialization (contained within this crate)
- Converts between `TransportData` and protobuf types
- Build time: 18.5s, 26/26 tests passing

**`transports/remotemedia-ffi/`** - Independent Python FFI transport
- Implements PyO3 FFI interface
- Compiles successfully
- No gRPC dependencies

**`transports/remotemedia-webrtc/`** - WebRTC placeholder

### ⚠️ Legacy Code (DEPRECATED - Do Not Use for New Development)

**`runtime/`** - Legacy monolithic runtime crate
- **DEPRECATED as of v0.4.0** - See [runtime/DEPRECATION_NOTICE.md](runtime/DEPRECATION_NOTICE.md)
- **Still contains embedded protobuf types** (not migrated)
- Uses `grpc_service::generated::AudioBuffer` etc. in 12+ files
- Has hard dependency on `prost` in data conversions
- **Cannot build without gRPC dependencies** (by design - tightly coupled)
- **Only kept for WASM binary temporarily** (`pipeline_executor_wasm.rs`)
- Will be removed or made into thin compatibility shim in future versions

## Path Forward

### Recommended Approach

1. **For new development:** Use `runtime-core` + specific transport crates
2. **For gRPC services:** Use `transports/remotemedia-grpc`
3. **For Python SDK:** Use `transports/remotemedia-ffi`
4. **For custom transports:** Implement `PipelineTransport` trait from `runtime-core`

### Legacy Runtime Migration (If Needed)

The legacy `runtime/` crate still has 12+ files using protobuf types directly:
- `runtime/src/data/{runtime_data.rs, conversions.rs, validation.rs}`
- `runtime/src/nodes/{audio_*.rs, video_processor.rs, silero_vad.rs, etc.}`
- `runtime/src/python/runtime_data_py.rs`
- `runtime/src/python/multiprocess/multiprocess_executor.rs`

**These should be refactored to use `RuntimeData` instead of protobuf types.**

However, since the properly decoupled architecture already exists in `runtime-core` and `transports/`, the preferred path is to:

1. **Deprecate the legacy `runtime/` crate entirely**
2. **Migrate any unique functionality to `runtime-core`**
3. **Use transport crates for all transport-specific code**

## Build Status

### ✅ Working
- `runtime-core`: Builds with zero transport dependencies
- `transports/remotemedia-grpc`: 18.5s build, 26/26 tests passing
- `transports/remotemedia-ffi`: Compiles successfully

### ❌ Not Working (Expected)
- `runtime` (legacy): Fails to build without protobuf due to direct protobuf type usage
- This is expected and correct - the legacy runtime should not be used

## For Contributors

**When working on this codebase:**

1. ✅ Use `runtime-core/` for core runtime logic
2. ✅ Use `transports/remotemedia-grpc/` for gRPC-specific code
3. ✅ Use `transports/remotemedia-ffi/` for Python FFI code
4. ❌ Avoid adding new code to `runtime/` (deprecated legacy crate)
5. ❌ Do not restore archived code from `archive/`
6. ❌ Do not try to "fix" protobuf coupling in `runtime/` - use `runtime-core` instead

**When you see `use crate::grpc_service`:**
- This indicates legacy code that hasn't been migrated
- It should be using `runtime-core` types instead
- Consider refactoring or migrating to `runtime-core`

## Related Documentation

- [archive/ARCHIVE.md](archive/ARCHIVE.md) - Detailed archive documentation
- [docs/MIGRATION_GUIDE_v0.3_to_v0.4.md](docs/MIGRATION_GUIDE_v0.3_to_v0.4.md) - Migration guide
- [specs/003-transport-decoupling/](specs/003-transport-decoupling/) - Design specs
- [IMPLEMENTATION_COMPLETE.md](IMPLEMENTATION_COMPLETE.md) - Implementation summary

## Questions?

- **Q: Why is `runtime/` still here?**
  A: It contains multiprocess Python execution logic that hasn't been fully migrated to `runtime-core`. It's retained for backward compatibility but should be considered legacy.

- **Q: Should I fix the build errors in `runtime/`?**
  A: No. The path forward is using `runtime-core`, not fixing the legacy runtime.

- **Q: Where do protobuf types belong?**
  A: Only in `transports/remotemedia-grpc/`. Core runtime should use `RuntimeData`/`TransportData`.

- **Q: Can I restore archived code?**
  A: No. Use the transport crates instead. Archived code is for reference only.
