# Phase 0: Technical Research - Remote Pipeline Execution Nodes

**Date**: 2025-01-08  
**Status**: Complete

## Overview

This document captures technical research for implementing remote pipeline execution nodes. The feature enables local pipelines to delegate work to remote pipeline servers over gRPC/WebRTC/HTTP transports with production-grade reliability (retry, circuit breakers, load balancing).

## Technical Decisions

### 1. Transport Client Architecture

**Decision**: Implement `PipelineClient` trait in `runtime-core/src/transport/client/mod.rs`

**Rationale**:
- Existing `PipelineTransport` trait is server-side (transports implement it to serve requests)
- Need client-side abstraction for remote execution
- Trait allows pluggable transports without RemotePipelineNode knowing concrete types
- Follows existing pattern from gRPC/WebRTC server implementations

**Key Insight**: The codebase already has retry logic (`runtime-core/src/executor/retry.rs`) and circuit breakers that we can reuse directly.

### 2. Circuit Breaker Implementation  

**Decision**: Reuse existing `CircuitBreaker` from `runtime-core/src/executor/retry.rs`

**Why**:
- Already battle-tested (165 lines, simple state machine)
- Default: trips after 5 consecutive failures (matches spec FR-009)
- No need for external crates like failsafe-rs
- Consistent with codebase minimalism

**Enhancement needed**: Add per-endpoint circuit breaker state tracking

### 3. Retry Logic

**Decision**: Reuse `RetryPolicy` and `execute_with_retry()` from existing retry.rs

**Configuration**:
- Default: 3 attempts, exponential backoff (100ms → 200ms → 400ms)  
- Spec FR-006 requires: 3 retries, 1 second initial backoff
- Make retry config customizable via node params

### 4. Load Balancing

**Decision**: Implement round-robin as default, support least-connections

**Why NOT use tower-balance**:
- Our use case is simpler (select endpoint URL from pool)
- tower-balance requires wrapping services in tower::Service trait
- Custom implementation is ~50 lines vs learning tower ecosystem

### 5. HTTP Client

**Decision**: Use `reqwest` (already in runtime-core/Cargo.toml line 66)

**Usage**: Manifest fetching from URLs + optional HTTP transport client

### 6. Circular Dependency Detection

**Algorithm**: Depth-first search with visited/in-stack tracking (Tarjan algorithm)
- Detect cycles during manifest validation (fail fast)
- Limit recursion depth to 10 levels

### 7. Authentication

**Pattern**: Follow existing gRPC auth pattern from `transports/remotemedia-grpc/src/auth.rs`:
- gRPC: Bearer token in `authorization` metadata  
- HTTP: `Authorization: Bearer <token>` header
- WebRTC: Custom signaling message

### 8. Health Checking

**Approach**: Active probing with background tokio task
- Default interval: 5 seconds (spec FR-016)
- Protocol-specific health checks (gRPC StreamInit, HTTP /health endpoint)

## Architecture Patterns

### Existing Transport Patterns

**gRPC Transport** (`transports/remotemedia-grpc/`):
- Server implements `PipelineExecutionService` trait
- Unary RPC: `ExecutePipeline(manifest, input) -> output`
- Streaming: Bidirectional stream with session router
- Auth via `AuthConfig` interceptor

**WebRTC Transport** (`transports/remotemedia-webrtc/`):
- Peer connection management
- gRPC-based signaling for SDP/ICE
- Data channels for RuntimeData

**Key Pattern**: We need client-side mirror of these server implementations

### Node Implementation Pattern

All nodes in `runtime-core/src/nodes/`:
- Implement `AsyncStreamingNode` trait
- Factory pattern for creation
- Params parsed from `serde_json::Value`

**RemotePipelineNode will follow same pattern**

## Implementation Approach

### Phase 1: Core (P1 - User Story 1)
1. Define `PipelineClient` trait  
2. Implement `GrpcPipelineClient`
3. Implement `RemotePipelineNode` with retry/timeout
4. Add to node registry
5. Test: Local VAD → Remote TTS

### Phase 2: Reliability (P2 - User Story 2)  
1. `EndpointPool` with load balancing
2. Per-endpoint circuit breakers
3. Health checking task
4. Fallback chains
5. Test: Multi-endpoint failover

### Phase 3: Additional Transports (P3 - User Story 3)
1. `HttpPipelineClient` 
2. `WebRtcPipelineClient`
3. Remote manifest loading
4. Circular dependency detection
5. Test: Cross-org pipelines

## Error Handling

**New Error Variants** (extend runtime-core/src/error.rs):
- `RemoteExecutionFailed`
- `RemoteTimeout`  
- `CircuitBreakerOpen`
- `AllEndpointsFailed`
- `ManifestFetchFailed`

## Testing Strategy

- **Unit**: Circuit breaker, load balancer, retry logic
- **Integration**: Mock gRPC server, failover scenarios
- **E2E**: Real remote pipelines

## Next Steps

1. Create data-model.md (structs, enums)
2. Define contracts/ (API schemas, error codes)
3. Write quickstart.md (examples)

---

**Research Status**: ✅ Complete  
**Next Phase**: Phase 1 - Design Artifacts
