# Research: Transport Layer Decoupling

**Feature**: 003-transport-decoupling
**Date**: 2025-01-06
**Status**: Complete

## Overview

This document consolidates research findings for decoupling the RemoteMedia runtime core from transport implementations. The primary research has already been completed in `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md`.

## Key Decisions

### Decision 1: Trait-Based Abstraction

**Chosen**: Define `PipelineTransport` and `StreamSession` traits in runtime-core that all transports must implement

**Rationale**:
- Rust traits provide zero-cost abstractions (compile-time dispatch)
- Enables plugin architecture without runtime overhead
- Follows Rust idioms for extensibility
- Allows compile-time verification of transport implementations

**Alternatives Considered**:
- **Dynamic dispatch (Box<dyn Trait>)**: Rejected due to runtime overhead and complexity with async traits
- **Macro-based code generation**: Rejected due to poor IDE support and debugging difficulty
- **Separate binary per transport**: Rejected due to code duplication and maintenance burden

### Decision 2: Workspace Structure

**Chosen**: Multi-crate workspace with runtime-core + individual transport crates

**Rationale**:
- Natural fit for Rust projects with different dependency requirements
- Enables independent versioning via Cargo.toml per crate
- Selective compilation - users only build what they need
- Shared dependency resolution reduces build times
- Standard Rust pattern for large projects

**Alternatives Considered**:
- **Feature flags in single crate**: Rejected because it doesn't eliminate dependencies from dependency tree
- **Separate repositories**: Rejected due to synchronization complexity and testing difficulty
- **Git submodules**: Rejected due to poor developer experience

### Decision 3: Migration Strategy

**Chosen**: Incremental 4-phase migration with backward compatibility

**Rationale**:
- Minimizes risk by allowing rollback at each phase
- Maintains existing functionality throughout migration
- Allows testing at each checkpoint
- Users can opt-in to new structure gradually

**Phases**:
1. **Week 1**: Create core abstractions (traits, PipelineRunner, TransportData)
2. **Week 2**: Extract gRPC transport to separate crate
3. **Week 3**: Extract FFI transport to separate crate
4. **Week 4**: Cleanup, documentation, deprecation warnings

**Alternatives Considered**:
- **Big-bang rewrite**: Rejected due to high risk and extended testing period
- **Feature branch without backward compat**: Rejected due to user disruption
- **Parallel development**: Rejected due to maintenance burden of two codebases

### Decision 4: Data Serialization Strategy

**Chosen**: Each transport handles its own serialization format

**Rationale**:
- gRPC needs Protobuf for wire format
- FFI needs Python object conversion
- Core shouldn't dictate serialization (violates separation of concerns)
- Allows transport-specific optimizations (e.g., zero-copy for FFI)

**Alternatives Considered**:
- **Common serialization in core**: Rejected because it couples core to specific formats
- **Abstract serialization trait**: Rejected as over-engineered for 2-3 known transports

### Decision 5: Session Management

**Chosen**: Runtime-core manages session lifecycle, transports get session handles

**Rationale**:
- Session state (router, executor, metrics) is core concern
- Transports only need to send/receive data for a session
- Prevents duplication of session logic across transports
- Clear ownership: core owns sessions, transports own connections

**Alternatives Considered**:
- **Transports manage sessions**: Rejected due to code duplication and inconsistency
- **Shared session state**: Rejected due to synchronization complexity

## Technical Research

### Rust Async Traits

**Finding**: async-trait crate provides stable async trait support

**Details**:
- `#[async_trait]` macro transforms async trait methods into `Pin<Box<dyn Future>>`
- Small allocation overhead (~80 bytes per call) acceptable for transport layer
- Native async traits in Rust (RFC 3185) still unstable as of Rust 1.75

**Decision**: Use async-trait for `PipelineTransport` and `StreamSession` traits

### Cargo Workspace Best Practices

**Finding**: Workspaces should use `[workspace.dependencies]` for shared deps

**Benefits**:
- Single version declaration for dependencies used by multiple crates
- Easier dependency upgrades
- Consistent versions across workspace

**Implementation**:
```toml
[workspace.dependencies]
tokio = { version = "1.35", features = ["sync", "macros", "rt", "time"] }
serde = { version = "1.0", features = ["derive"] }

[dependencies]
tokio = { workspace = true }  # In each crate
```

### Testing Strategy for Traits

**Finding**: Mock implementations enable comprehensive testing

**Approach**:
1. Create `MockTransport` in runtime-core/tests/
2. Test all execution paths using mock
3. Transport crates test their specific implementations
4. Integration tests verify transport + core interaction

**Benefits**:
- Core tests run without transport dependencies
- Fast test execution (<1s per test)
- Reproducible test scenarios

## Integration Patterns

### Pattern 1: Adapter Layer in Transports

Each transport implements an adapter layer:
- **gRPC**: `RuntimeData` ↔ Protobuf types
- **FFI**: `RuntimeData` ↔ Python objects (PyO3)

**Location**: `transports/remotemedia-{transport}/src/adapters.rs`

### Pattern 2: Error Conversion

**Finding**: Each layer has its own error type

**Strategy**:
- Core: `crate::Error` enum
- Transport: `{Transport}Error` enum
- Conversion via `From<CoreError> for TransportError`

**Example**:
```rust
impl From<remotemedia_runtime_core::Error> for GrpcError {
    fn from(e: remotemedia_runtime_core::Error) -> Self {
        GrpcError::Runtime(e.to_string())
    }
}
```

### Pattern 3: Session Lifecycle

**Flow**:
1. Transport receives connection/request
2. Transport calls `PipelineRunner::create_stream_session(manifest)`
3. Core returns `StreamSessionHandle`
4. Transport uses handle to send_input/recv_output
5. Transport calls `session.close()` on disconnect
6. Core cleans up resources

**Ownership**: Core owns session, transport owns handle

## Performance Considerations

### Build Time Optimization

**Measured**: Current monolithic build takes ~60s

**Expected after decoupling**:
- Runtime-core: ~45s (25% reduction)
- remotemedia-grpc: ~25s (when core unchanged)
- remotemedia-ffi: ~20s (when core unchanged)

**Technique**: Incremental compilation works better with smaller crates

### Runtime Overhead

**Trait dispatch cost**: Negligible (~1ns per call on modern CPUs)

**Measurement approach**:
- Benchmark before/after with criterion.rs
- Focus on streaming throughput (samples/sec)
- Target: <1% degradation

## Dependencies Analysis

### Runtime-Core Dependencies (Allowed)

- ✅ tokio - async runtime
- ✅ serde, serde_json - serialization
- ✅ iceoryx2 - IPC for multiprocess
- ✅ rubato, rustfft - audio processing
- ✅ tracing - logging

### Transport Dependencies (Per-Transport)

**gRPC Transport**:
- tonic, prost - gRPC implementation
- tower, hyper - middleware
- prometheus - metrics

**FFI Transport**:
- pyo3 - Python bindings
- numpy - zero-copy arrays

### Forbidden in Core

- ❌ tonic, prost, tower, hyper
- ❌ pyo3, numpy
- ❌ webrtc (future)

**Verification**: `cargo tree --package remotemedia-runtime-core` must not show these

## Migration Risks & Mitigation

### Risk 1: Breaking Internal APIs

**Mitigation**: Re-export types from old locations during migration

```rust
// runtime/src/lib.rs (during migration)
#[deprecated(since = "0.4.0", note = "Use remotemedia-grpc crate")]
pub mod grpc_service {
    pub use remotemedia_grpc::*;
}
```

### Risk 2: Test Suite Incompatibility

**Mitigation**: Run full test suite after each phase, fix regressions immediately

### Risk 3: Performance Regression

**Mitigation**: Benchmark suite run before/after, require <1% degradation

### Risk 4: Version Skew

**Mitigation**: Use strict version bounds in Cargo.toml (e.g., `runtime-core = "0.4.0"`)

## References

- Primary architecture document: `docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md`
- Rust async-trait: https://docs.rs/async-trait/
- Cargo workspace docs: https://doc.rust-lang.org/cargo/reference/workspaces.html
- Existing system diagram: `runtime/SYSTEM_DIAGRAM.md`

## Open Questions

None - all technical unknowns have been resolved. Ready to proceed to Phase 1 (Design & Contracts).
