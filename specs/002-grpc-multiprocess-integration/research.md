# Phase 0: Research & Technical Decisions

## Executor Routing Strategy

**Decision**: Registry-based routing with node_type pattern matching
**Rationale**:
- Existing gRPC service already parses node_type from manifest
- MultiprocessExecutor (spec 001) is registered for specific Python node types
- Pattern matching allows backward compatibility without manifest changes
- Registry pattern enables future executor types without service refactor
**Alternatives considered**:
- Explicit executor field in manifest: Would break backward compatibility
- Runtime detection via Python module imports: Too slow, adds 100+ms overhead
- Hardcoded node type list: Not extensible, requires recompilation for new nodes

## Manifest Configuration Extension

**Decision**: Use manifest metadata field for multiprocess configuration
**Rationale**:
- manifest.v1.json schema allows "additionalProperties" in metadata
- No breaking changes to existing schema
- Per-pipeline configuration without service restart
- Falls back to runtime.toml defaults if not specified
**Alternatives considered**:
- New manifest schema version: Would break existing clients
- gRPC request headers: Not persisted with manifest, harder to debug
- Environment variables: Not per-session, potential conflicts

## Data Conversion Strategy

**Decision**: Zero-copy where possible, serialize at executor boundaries
**Rationale**:
- Native Rust nodes use in-memory buffers (Arc<Vec<T>>)
- Multiprocess nodes use iceoryx2 shared memory
- Convert only when crossing native ↔ multiprocess boundary
- AudioBuffer conversion via memcpy is <1ms for typical chunks
**Alternatives considered**:
- Always use shared memory: Would require all native nodes to use iceoryx2, major refactor
- Always serialize: Would negate multiprocess performance benefits
- Custom protocol: Unnecessary complexity, existing types work

## Process Lifecycle Integration

**Decision**: Integrate with gRPC session management (async_pipeline.rs)
**Rationale**:
- Existing session infrastructure tracks connection state
- Process cleanup can piggyback on session termination
- Client disconnects already trigger resource cleanup
- Async pipeline handles backpressure and flow control
**Alternatives considered**:
- Separate process manager service: Would duplicate session tracking logic
- Process pool shared across sessions: Violates isolation requirements from spec
- Static process allocation: Wastes resources, doesn't scale

## Node Type Identification

**Decision**: Suffix-based pattern matching with registry fallback
**Rationale**:
- Python nodes follow "XyzNode" naming convention (WhisperNode, LFM2Node)
- Check registry first for explicit mappings
- Fall back to suffix pattern if not found
- Allows manual overrides via registry for ambiguous cases
**Alternatives considered**:
- Parse Python source code: Too slow, fragile
- Require manifest annotation: Breaks backward compatibility
- Try-catch with first executor: Fails noisily, poor UX

## Error Handling Strategy

**Decision**: Fail-fast with full pipeline termination
**Rationale**:
- Aligns with existing gRPC service error semantics
- Prevents partial state inconsistencies
- Clear error boundary for clients
- Matches behavior from spec 001 (multiprocess executor)
**Alternatives considered**:
- Graceful degradation: Complex state management, unclear semantics
- Automatic retry: Risk of crash loops, hides underlying issues
- Partial success responses: Clients not designed for partial results

## Initialization Progress Streaming

**Decision**: Extend existing StreamingPipelineService
**Rationale**:
- Service already streams NodeResult messages
- Add InitProgressUpdate message type to proto
- Clients already handle bidirectional streaming
- No new RPC endpoints required
**Alternatives considered**:
- Separate progress RPC: Extra connection overhead
- Polling-based status checks: Higher latency, worse UX
- Event stream via WebSocket: Would require additional transport

## Resource Limit Enforcement

**Decision**: Two-tier limits (global + per-session)
**Rationale**:
- Global limits protect service stability (max total processes)
- Per-session limits honor manifest configuration
- Check both before spawning processes
- Global limits take precedence over manifest
**Alternatives considered**:
- Only global limits: Less flexibility for different use cases
- Only per-session limits: Service instability risk
- Dynamic limits based on load: Too complex, unpredictable behavior

## Integration with Existing Executor

**Decision**: Wrap existing Executor with executor selection logic
**Rationale**:
- Existing Executor in execution.rs already handles native nodes
- Add ExecutorBridge layer that routes to native or multiprocess
- Minimal changes to existing ExecutePipeline RPC handler
- Preserves backward compatibility for non-Python pipelines
**Alternatives considered**:
- Replace existing Executor entirely: High risk, large refactor
- Parallel implementation with feature flag: Code duplication, maintenance burden
- Proxy pattern with full interception: Performance overhead

## Data Model Serialization

**Decision**: Use existing RuntimeData format with IPC adapter
**Rationale**:
- RuntimeData already supports audio, text, tensor, JSON
- Multiprocess executor (spec 001) uses RuntimeData internally
- Add IPCAdapter to convert RuntimeData ↔ iceoryx2 messages
- No changes to node interfaces
**Alternatives considered**:
- New data format for multiprocess: Would require dual implementations
- Protobuf for all data transfer: Serialization overhead defeats zero-copy goal
- Direct memory sharing for all: Not feasible across executors

## Configuration Loading Priority

**Decision**: Manifest > runtime.toml > defaults (in that order)
**Rationale**:
- Manifest configuration most specific (per-pipeline)
- runtime.toml provides service-wide defaults
- Hardcoded defaults as final fallback
- Clear precedence order for operators
**Alternatives considered**:
- Only manifest: No service-wide defaults possible
- Only runtime.toml: No per-pipeline flexibility
- Environment variables: Not declarative, harder to audit
