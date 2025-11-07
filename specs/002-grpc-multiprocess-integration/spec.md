# Feature Specification: gRPC Multiprocess Integration

**Feature Branch**: `002-grpc-multiprocess-integration`
**Created**: 2025-11-05
**Status**: Draft
**Input**: User description: "Execute the multiprocess python nodes / multiprocess executor from the @runtime\src\grpc_service\ following the same manifest @runtime\schemas\manifest.v1.json used in our client, @examples\nextjs-tts-app\lib\socket-handler.ts"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Execute Manifest Pipeline with Multiprocess Python Nodes (Priority: P1)

A client application (web app, CLI tool) submits a pipeline manifest to the gRPC service that includes Python-based AI nodes (Whisper ASR, LFM2 audio model, VibeVoice TTS). The service executes these Python nodes using the multiprocess executor, allowing concurrent processing without GIL contention, while maintaining compatibility with the existing manifest format.

**Why this priority**: This is the core value proposition - enabling existing clients to benefit from multiprocess execution without changing their integration. The Next.js speech-to-speech app can immediately see latency improvements from 10+ seconds to under 500ms.

**Independent Test**: Submit a manifest with 2+ Python nodes to the gRPC service ExecutePipeline endpoint. Verify that:
- Pipeline completes successfully
- Python nodes execute concurrently (overlap in execution time)
- Total execution time is less than sequential execution
- Results match expected output format

**Acceptance Scenarios**:

1. **Given** a manifest with Python nodes (Whisper, LFM2, VibeVoice), **When** client calls ExecutePipeline via gRPC, **Then** service spawns separate processes for each Python node and executes them concurrently
2. **Given** a multiprocess pipeline is executing, **When** monitoring resource usage, **Then** multiple Python processes are visible with independent memory spaces
3. **Given** a pipeline with connected Python nodes, **When** execution completes, **Then** data flows correctly between nodes via shared memory IPC
4. **Given** a Python node in the manifest, **When** execution begins, **Then** node initializes within the configured timeout (default 30s)

---

### User Story 2 - Mixed Executor Pipeline Execution (Priority: P2)

A client submits a manifest containing a mix of Python nodes (AI models), native Rust nodes (audio processing), and optionally WASM nodes. The service intelligently routes each node to the appropriate executor based on node type, enabling hybrid pipelines that leverage the strengths of each execution environment.

**Why this priority**: Enables flexibility in pipeline design. Audio preprocessing (resampling, VAD) can use fast native nodes while AI models use multiprocess Python execution.

**Independent Test**: Create a manifest with:
- 1 native Rust node (AudioChunkerNode)
- 2 Python nodes (ASR, TTS)
- Connections between them

Submit to gRPC service and verify:
- All nodes execute in their appropriate executors
- Data transfers work across executor boundaries
- End-to-end pipeline produces correct results

**Acceptance Scenarios**:

1. **Given** a manifest with mixed node types, **When** service parses the manifest, **Then** it correctly identifies Python nodes by node_type and routes them to multiprocess executor
2. **Given** a pipeline with Rust and Python nodes, **When** execution proceeds, **Then** data converts correctly between native and shared memory IPC formats
3. **Given** a Python node connected to a Rust node, **When** data flows between them, **Then** latency remains under 2ms for typical audio chunks (1024 samples)

---

### User Story 3 - Configure Multiprocess Execution Parameters (Priority: P3)

A client specifies execution parameters for multiprocess nodes directly in the manifest, such as maximum processes per session, channel capacity, and initialization timeout. The service applies these configurations when creating the multiprocess executor, allowing per-pipeline resource control.

**Why this priority**: Provides operational flexibility without requiring service restarts. Different pipelines can have different resource constraints based on use case.

**Independent Test**: Submit two manifests with different multiprocess configurations:
- Manifest A: max 5 processes, 50 channel capacity
- Manifest B: max 20 processes, 200 channel capacity

Verify each pipeline respects its configured limits.

**Acceptance Scenarios**:

1. **Given** a manifest with multiprocess configuration in metadata, **When** service creates the executor, **Then** it applies the specified limits
2. **Given** a pipeline exceeds configured process limit, **When** attempting to add another node, **Then** service returns a resource limit error
3. **Given** no multiprocess configuration in manifest, **When** service creates the executor, **Then** it uses default values from runtime.toml

---

### Edge Cases

- What happens when a Python node in the manifest crashes during execution? (Service should terminate the entire pipeline and return an error to the client)
- How does the system handle manifests with invalid node_type references? (Service validates manifest before execution and returns validation error)
- What happens when a client disconnects mid-execution? (Service cleans up all spawned processes and releases resources)
- How does the system handle Python nodes that fail to initialize within timeout? (Service returns initialization timeout error and terminates session)
- What happens when shared memory IPC channels fill up due to slow consumers? (Backpressure mechanism blocks producers until space available)
- How does the service handle manifests requesting more resources than available? (Returns resource limit error before starting execution)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Service MUST parse manifest.v1.json format and extract node definitions, connections, and metadata
- **FR-002**: Service MUST identify Python nodes by node_type pattern (e.g., ends with "Node" suffix and registered in Python SDK)
- **FR-003**: Service MUST route Python nodes to the multiprocess executor while routing other nodes to appropriate executors
- **FR-004**: Service MUST create a multiprocess executor instance per session with isolated process pools
- **FR-005**: Service MUST apply multiprocess configuration from manifest metadata (if provided) or use defaults
- **FR-006**: Service MUST validate manifest schema before execution and return clear validation errors
- **FR-007**: Service MUST handle data transfer between nodes in different executors (native ↔ multiprocess)
- **FR-008**: Service MUST terminate all processes when a session ends (normal completion, error, or client disconnect)
- **FR-009**: Service MUST expose initialization progress for Python nodes via streaming gRPC response
- **FR-010**: Service MUST enforce resource limits (max processes, memory) per session as configured
- **FR-011**: Service MUST clean up shared memory IPC channels when sessions terminate
- **FR-012**: Service MUST support both streaming and batch execution modes with multiprocess nodes

### Non-Functional Requirements

- **NFR-001**: Manifest parsing and validation must complete in under 100ms for typical pipelines (< 10 nodes)
- **NFR-002**: Process spawning overhead must not exceed 150ms per Python node
- **NFR-003**: Data transfer between executors must add less than 2ms latency per hop
- **NFR-004**: Service must support at least 10 concurrent sessions with multiprocess pipelines
- **NFR-005**: Memory overhead per session must not exceed 50MB (excluding model loading)

### Key Entities *(include if feature involves data)*

- **ExecutorRegistry**: Maps node types to executor implementations (multiprocess, native, WASM), populated at service startup
- **SessionExecutionContext**: Tracks executor instances, process handles, and IPC channels for a specific session
- **ManifestConfiguration**: Parsed representation of manifest metadata including multiprocess settings
- **NodeExecutionPlan**: Ordered sequence of nodes with assigned executors, derived from manifest connections

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Clients can execute existing manifests through gRPC service without modification to manifest format
- **SC-002**: Pipelines with 3+ Python nodes complete in under 60% of sequential execution time due to concurrent processing
- **SC-003**: Service successfully handles 10 concurrent sessions, each running multiprocess pipelines, without process interference
- **SC-004**: End-to-end latency for speech-to-speech pipeline (Whisper → LFM2 → VibeVoice) reduces from 10+ seconds to under 500ms
- **SC-005**: 99% of manifest validation errors are detected before spawning any processes
- **SC-006**: Service cleans up all processes within 5 seconds of session termination (measured from cleanup request to all processes stopped)
- **SC-007**: Data loss rate is zero across executor boundaries (verified through checksums on large audio/tensor payloads)
- **SC-008**: Client applications experience no API breaking changes when multiprocess execution is enabled

## Scope *(mandatory)*

### In Scope

- Parsing and executing manifest.v1.json format pipelines with Python nodes
- Routing Python nodes to multiprocess executor based on node type identification
- Mixed-executor pipelines (Python + Rust + WASM nodes in same pipeline)
- Per-session multiprocess executor configuration via manifest metadata
- Data conversion between native memory and shared memory IPC
- Process lifecycle management (spawn, monitor, cleanup) for Python nodes
- Initialization progress streaming for Python nodes
- Resource limit enforcement per session
- Error handling and cleanup for crashed Python processes

### Out of Scope

- Changes to manifest.v1.json schema (use existing format)
- Automatic node type detection beyond node_type field matching
- Dynamic executor selection based on runtime conditions (routing is static based on node type)
- Process migration or hot-reload of running nodes
- Cross-session process pooling or reuse (each session has isolated processes)
- Custom Python interpreter paths per-node (single interpreter per executor config)
- Python node debugging or profiling integration (separate concern)
- Backup or failover executors for crashed nodes (fail-fast model)

## Constraints & Assumptions *(mandatory)*

### Technical Constraints

- Must use existing manifest.v1.json schema without breaking changes
- Must integrate with existing gRPC service architecture (server.rs, execution.rs)
- Multiprocess executor from spec 001 is already implemented and tested
- Service runs on platforms supporting iceoryx2 (Linux x64, Windows x64)
- Python 3.11+ available on system PATH or via configuration

### Business Constraints

- Existing Next.js client must work without code changes (only configuration updates)
- Implementation must not impact performance of non-Python pipelines
- Solution must support incremental rollout (feature flag to enable multiprocess execution)

### Assumptions

- Node types for Python nodes follow a consistent naming pattern (e.g., "WhisperNode", "LFM2Node", "VibeVoiceNode")
- Clients using manifests already handle streaming responses for long-running pipelines
- System resources (memory, CPU) are sufficient for configured process limits
- Python nodes registered in python-client SDK are compatible with multiprocess execution
- IPC channel capacity (default 100 messages) is sufficient for typical data flow rates
- Manifest metadata section can be extended with custom fields (per JSON schema "additionalProperties")

## Dependencies *(optional)*

### Internal Dependencies

- Spec 001 (multiprocess-python-nodes): Multiprocess executor implementation must be complete and stable
- manifest.v1.json schema: Must support metadata extensibility for configuration
- Python SDK (remotemedia.core.multiprocess): Nodes must implement MultiprocessNode interface

### External Dependencies

- iceoryx2: Shared memory IPC library must be available at runtime
- Python interpreter: Must be accessible and version 3.11 or higher
- gRPC framework: Tokio-based async runtime must support executor integration pattern

## Risks & Mitigation *(optional)*

### Technical Risks

**Risk**: Python node type identification is ambiguous (multiple nodes match pattern)
**Impact**: High - incorrect executor routing causes failures
**Mitigation**: Require explicit executor field in manifest for Python nodes, fallback to pattern matching

**Risk**: Data conversion overhead between executors negates multiprocess benefits
**Impact**: Medium - performance gains are minimal
**Mitigation**: Benchmark data conversion paths, optimize hot paths with zero-copy techniques where possible

**Risk**: Process cleanup failures leave orphaned Python processes
**Impact**: High - resource leaks over time
**Mitigation**: Implement process group termination, add health monitoring with forced kill after timeout

### Operational Risks

**Risk**: Clients specify excessive resource limits causing system instability
**Impact**: High - service becomes unavailable
**Mitigation**: Enforce global resource caps independent of manifest configuration, return errors when limits exceeded

**Risk**: Python nodes fail to initialize due to missing dependencies
**Impact**: Medium - user confusion, poor error messages
**Mitigation**: Validate Python environment at service startup, provide clear initialization error messages with dependency hints

## Future Considerations *(optional)*

- Executor affinity hints in manifest to optimize node placement
- Process pooling across sessions for faster initialization
- Dynamic executor selection based on real-time resource availability
- Python node debugging mode with process attach support
- Automatic retry with different executor on failure
- Telemetry and metrics integration for multiprocess executor performance
- Support for custom Python interpreters per-node (virtualenv paths)
