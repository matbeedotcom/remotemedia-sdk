# Phase 0: Research & Technical Decisions

## IPC Technology Selection

**Decision**: iceoryx2 for shared memory IPC
**Rationale**:
- True zero-copy data transfer without serialization
- Microsecond-level latency (<100µs overhead requirement)
- Built-in publish-subscribe pattern matches pipeline architecture
- Cross-platform support (Linux, Windows)
- Rust-native with excellent safety guarantees
**Alternatives considered**:
- gRPC: Too much overhead (serialization, network stack)
- Unix sockets: Data copying required, platform-specific
- mmap: Lower-level, would need custom protocol implementation

## Python-Rust Integration

**Decision**: PyO3 for Python-Rust binding
**Rationale**:
- Mature ecosystem with production usage
- Supports both embedding Python and creating extensions
- Clean API for GIL management
- Zero-overhead FFI when properly used
- Active maintenance and community
**Alternatives considered**:
- CFFI: More complex, manual memory management
- ctypes: Less safe, more boilerplate
- Separate processes only: Would require additional IPC layer

## Process Management Strategy

**Decision**: Child process spawning with std::process
**Rationale**:
- Direct parent-child relationship for lifecycle management
- OS-level process isolation for fault containment
- Clean termination via process groups
- Event-driven monitoring via process exit signals
**Alternatives considered**:
- Process pools: Unnecessary complexity for on-demand spawning
- Docker containers: Too much overhead for local execution
- Threads with separate interpreters: GIL still shared in CPython

## Shared Memory Layout

**Decision**: Fixed header + variable payload design
**Rationale**:
- Header contains type, size, and metadata
- Payload stored contiguously for cache efficiency
- Supports audio (PCM), video (raw frames), tensors
- Alignment-aware for SIMD operations
**Alternatives considered**:
- Separate channels per data type: More complex routing
- Protobuf messages: Serialization overhead
- Arrow/Parquet: Overkill for streaming data

## Backpressure Mechanism

**Decision**: Blocking writes when buffer full
**Rationale**:
- Prevents data loss
- Natural flow control
- Simple to implement and reason about
- Matches real-time processing semantics
**Alternatives considered**:
- Dropping frames: Unacceptable for audio continuity
- Dynamic buffer growth: Unbounded memory risk
- Rate limiting: Artificial constraint on processing

## Process Health Monitoring

**Decision**: Event-driven via SIGCHLD/process exit
**Rationale**:
- Zero overhead during normal operation
- Immediate notification of failures
- OS-guaranteed delivery
- No polling loops consuming CPU
**Alternatives considered**:
- Heartbeat messages: Network overhead
- Health check endpoints: Polling required
- Shared memory flags: Risk of stale state

## Error Recovery Strategy

**Decision**: Full pipeline termination on node failure
**Rationale**:
- Data integrity over partial functionality
- Predictable behavior for users
- Simplifies state management
- Clear error boundaries
**Alternatives considered**:
- Automatic restart: Risk of crash loops
- Degraded operation: Complex partial pipeline logic
- Hot spare processes: Resource waste

## Memory Management

**Decision**: Pre-allocated shared memory segments
**Rationale**:
- Predictable memory usage
- No allocation during data transfer
- Reduced fragmentation
- Better cache locality
**Alternatives considered**:
- Dynamic allocation: Performance unpredictability
- Memory pools: Additional complexity
- OS page cache: Less control over lifetime