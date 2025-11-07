# Feature Specification: Multi-Process Node Execution

**Feature Branch**: `001-multiprocess-python-nodes`
**Created**: 2025-11-04
**Status**: Draft
**Input**: User description: "Multi-process Python nodes with independent GILs using iceoryx2 shared memory and PyO3 for zero-copy, deadlock-free concurrent execution"

## Clarifications

### Session 2025-11-04
- Q: When multiple Python nodes are needed for a pipeline, how should the system spawn processes? → A: On-demand during session init
- Q: When a Python node process crashes unexpectedly, what recovery action should the system take? → A: Terminate entire pipeline
- Q: When the shared memory pool for inter-process communication is exhausted, how should the system respond? → A: Block upstream with backpressure
- Q: What is the maximum number of Python node processes allowed per session? → A: Configurable per deployment, default no limit
- Q: How frequently should the system check Python node process health/liveness? → A: Event-driven only (no polling)

## User Scenarios & Testing

### User Story 1 - Run Multiple AI Models Concurrently (Priority: P1)

Users can run multiple AI-powered nodes (speech recognition, text-to-speech, language models) in the same pipeline simultaneously without performance degradation or system hangs.

**Why this priority**: This is the core capability that unblocks real-time speech-to-speech applications. Without concurrent execution, users experience unacceptable delays (10+ seconds) when multiple AI models need to process data in sequence.

**Independent Test**: Can be fully tested by creating a pipeline with 2+ Python-based AI nodes and verifying they process data concurrently without blocking each other, delivering end-to-end latency under 500ms.

**Acceptance Scenarios**:

1. **Given** a pipeline with LFM2-Audio and VibeVoice TTS nodes, **When** user sends audio input, **Then** both nodes initialize and process data without either blocking the other
2. **Given** multiple concurrent streaming sessions each using Python nodes, **When** sessions overlap in execution, **Then** all sessions continue processing without delays or hangs
3. **Given** a Python node fails during processing, **When** error occurs, **Then** other nodes in the pipeline continue operating normally

---

### User Story 2 - Fast Pipeline Initialization (Priority: P2)

Users receive immediate feedback on pipeline initialization status, with all AI models loaded and ready before the first data arrives.

**Why this priority**: Eliminates the "cold start" problem where the first utterance has 10-30 second delay. Users expect responsive systems, and silent waiting periods create poor experience.

**Independent Test**: Can be fully tested by measuring time from session creation to "ready" state, verifying it completes within predictable timeframe (10-30s) with progress updates sent every 2 seconds.

**Acceptance Scenarios**:

1. **Given** user creates a new streaming session, **When** initialization begins, **Then** system sends real-time status updates for each node being loaded
2. **Given** all nodes are initializing, **When** one node fails to load, **Then** user receives error message identifying which node failed and why
3. **Given** models are pre-loaded, **When** first data chunk arrives, **Then** processing begins within 50ms

---

### User Story 3 - Efficient Data Transfer Between Nodes (Priority: P1)

Audio and video data flows between pipeline nodes without duplication or serialization overhead, maintaining low latency (<1ms inter-node transfer) even for large payloads.

**Why this priority**: Real-time audio/video processing requires minimal latency. Copying multi-megabyte buffers between nodes would add 10-100ms per hop, making real-time interaction impossible.

**Independent Test**: Can be fully tested by measuring latency between node output and next node input for various payload sizes (1KB to 10MB), verifying transfer time is independent of payload size.

**Acceptance Scenarios**:

1. **Given** a pipeline with 5 nodes processing 10-second audio buffers (3.84 MB), **When** data flows through the pipeline, **Then** inter-node transfer adds less than 1ms latency per hop
2. **Given** high-resolution video frames (1920x1080), **When** transferred between nodes, **Then** frames arrive without quality loss or corruption
3. **Given** concurrent pipelines sharing the system, **When** multiple data transfers occur simultaneously, **Then** memory usage remains constant (no duplication)

---

### Edge Cases

- What happens when a Python node crashes mid-processing?
  - Entire pipeline is terminated immediately
  - All node processes are cleaned up
  - User receives error notification identifying the failed node
- What happens when system memory is exhausted?
  - New node processes fail to spawn with clear error message
  - Existing nodes continue operating
  - System does not hang or deadlock
- What happens when two nodes try to initialize simultaneously?
  - Both initialize in parallel without interfering
  - Shared resources are properly managed
  - No race conditions or deadlocks occur
- What happens when data arrives faster than nodes can process?
  - Backpressure is applied upstream (blocking)
  - Shared memory buffers fill to capacity
  - Upstream nodes block until space available

## Requirements

### Functional Requirements

- **FR-001**: System MUST execute multiple Python-based pipeline nodes concurrently without blocking or interference
- **FR-002**: System MUST detect node failures and terminate the entire pipeline cleanly when any node crashes
- **FR-003**: System MUST transfer audio data (up to 10MB buffers) between nodes with latency under 1ms per transfer
- **FR-004**: System MUST support concurrent initialization of all pipeline nodes during session setup before data processing begins (on-demand spawning)
- **FR-005**: System MUST send real-time initialization status updates to clients for each node being loaded
- **FR-006**: System MUST preserve data integrity when transferring audio, video, and tensor data between processes
- **FR-007**: System MUST handle node initialization failures gracefully, reporting errors to users without hanging
- **FR-008**: System MUST support session cleanup where all node processes terminate when session ends
- **FR-009**: System MUST track node execution status (idle, initializing, ready, processing, error, stopped) and expose to monitoring
- **FR-010**: System MUST prevent memory leaks when nodes are created, destroyed, or crash unexpectedly
- **FR-011**: System MUST apply backpressure to upstream nodes when shared memory buffers reach capacity, blocking writes until space is available
- **FR-012**: System MUST support configurable limits on maximum Python processes per session (default: no limit)
- **FR-013**: System MUST monitor process health using event-driven mechanisms (process exit signals) without periodic polling

### Key Entities

- **Node Process**: An isolated execution environment for a single pipeline node, with dedicated resources and lifecycle
  - Attributes: process ID, node type, status, memory usage, initialization timestamp
  - Lifecycle states: idle → initializing → ready → processing → stopping → stopped

- **Shared Memory Channel**: A communication pathway for zero-copy data transfer between processes
  - Attributes: channel name, buffer capacity, current occupancy, publisher count, subscriber count
  - Data flow: publisher writes → shared memory → subscriber reads (no copy)

- **RuntimeData Message**: A structured data packet containing audio, video, text, or tensor information
  - Attributes: data type, payload size, sample rate (audio), dimensions (video/tensor), session ID
  - Representation: Fixed-size header + variable-size payload in contiguous memory

## Success Criteria

### Measurable Outcomes

- **SC-001**: Users can run pipelines with 5+ Python nodes concurrently without experiencing hangs or deadlocks (100% success rate across 100 test sessions)
- **SC-002**: Pipeline initialization completes within 30 seconds for sessions with 3+ large AI models, with status updates delivered every 2 seconds
- **SC-003**: Audio data transfer between nodes maintains latency under 1ms for payloads up to 10MB (measured at 95th percentile)
- **SC-004**: System handles 10+ concurrent streaming sessions each using 3+ Python nodes without performance degradation
- **SC-005**: Node crashes result in clean pipeline termination with 100% detection rate and proper cleanup (100 crash scenarios tested)
- **SC-006**: Memory usage remains stable over 8-hour continuous operation with periodic node creation and destruction (no leaks detected)
- **SC-007**: End-to-end speech-to-speech latency improves by 50% compared to current sequential execution model

## Assumptions

- Python nodes are compute-intensive and benefit from process isolation more than lightweight utility nodes
- Average pipeline contains 3-5 Python-based AI nodes plus 3-5 lightweight processing nodes
- Typical audio buffer size is 100ms-1s at 24kHz-48kHz (9.6KB - 192KB for mono F32)
- Video frame size ranges from 480p (921KB RGB) to 1080p (6.2MB RGB)
- System has sufficient memory to load 5+ large AI models simultaneously (15-30GB total)
- Inter-process communication overhead is acceptable if under 100µs per message
- Node initialization time is dominated by model loading (5-30 seconds), not IPC setup
- Process count limits are deployment-specific with no default restriction (operators set based on resources)
- Process exit events are reliably delivered by the OS without need for polling-based health checks

## Out of Scope

- Distributed execution across multiple machines (network-based IPC)
- GPU memory sharing between processes (each process manages its own CUDA context)
- Dynamic process migration or load balancing between hosts
- Persistent node pools that survive session lifetime
- Hot-swapping or upgrading nodes without session interruption
