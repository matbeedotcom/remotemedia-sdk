# Native Rust Acceleration Specification

## ADDED Requirements

### Requirement: Complete Rust Pipeline Executor
The system SHALL provide a complete async pipeline execution engine in Rust.

#### Scenario: Parse and validate manifest
- **GIVEN** JSON manifest string
- **WHEN** executor parses manifest
- **THEN** manifest SHALL be validated against schema and parsed into graph structure

#### Scenario: Build execution graph
- **GIVEN** validated manifest
- **WHEN** executor builds graph
- **THEN** graph SHALL contain all nodes and edges with topological sort order

#### Scenario: Execute nodes in order
- **GIVEN** execution graph
- **WHEN** executor runs pipeline
- **THEN** nodes SHALL execute in topological order with data flowing between them

#### Scenario: Handle cyclic dependencies
- **GIVEN** manifest with cycle (A → B → A)
- **WHEN** executor validates graph
- **THEN** executor SHALL return error identifying cycle

### Requirement: Audio Processing Nodes (Rust Native)
The system SHALL provide high-performance Rust implementations of common audio processing operations.

#### Scenario: Voice Activity Detection
- **GIVEN** audio buffer (f32 array, 16kHz)
- **WHEN** VADNode processes audio with threshold -30dB
- **THEN** node SHALL return segments list with start/end times and energy levels
- **AND** processing SHALL complete in <50μs per 30ms frame

#### Scenario: Audio Resampling
- **GIVEN** audio buffer at 48kHz
- **WHEN** ResampleNode resamples to 16kHz with "high" quality
- **THEN** output SHALL be 1/3 length of input
- **AND** processing SHALL complete in <2ms per second of audio

#### Scenario: Format Conversion
- **GIVEN** audio buffer as i16 samples
- **WHEN** FormatConverterNode converts to f32
- **THEN** samples SHALL be normalized to [-1.0, 1.0] range
- **AND** conversion SHALL be zero-copy where possible

### Requirement: Error Handling with Retry
The system SHALL provide comprehensive error handling with retry policies for transient failures.

#### Scenario: Retry transient errors
- **GIVEN** node execution fails with timeout error
- **WHEN** executor applies exponential backoff retry policy
- **THEN** executor SHALL retry up to 3 times with delays 100ms, 200ms, 400ms

#### Scenario: Propagate non-retryable errors
- **GIVEN** node execution fails with parse error
- **WHEN** executor evaluates error type
- **THEN** executor SHALL immediately return error without retry

#### Scenario: Circuit breaker for persistent failures
- **GIVEN** node fails 5 consecutive times
- **WHEN** executor detects pattern
- **THEN** executor SHALL skip node and mark as failed

### Requirement: Performance Monitoring
The system SHALL track execution metrics for all nodes and export as JSON.

#### Scenario: Record node execution time
- **GIVEN** pipeline execution
- **WHEN** each node completes
- **THEN** executor SHALL record execution time with microsecond precision

#### Scenario: Track memory usage
- **GIVEN** pipeline execution
- **WHEN** monitoring system checks memory
- **THEN** executor SHALL record peak memory usage per node

#### Scenario: Export metrics as JSON
- **GIVEN** pipeline execution completes
- **WHEN** user requests metrics
- **THEN** executor SHALL export JSON with total time, node times, memory usage

### Requirement: Zero-Copy Data Flow
The system SHALL minimize data copying between Python and Rust.

#### Scenario: Zero-copy numpy input
- **GIVEN** Python numpy array passed to Rust
- **WHEN** Rust node accesses array via rust-numpy
- **THEN** no copy SHALL occur (borrow via PyO3)

#### Scenario: Zero-copy format conversion
- **GIVEN** audio format conversion (f32 ↔ i16)
- **WHEN** FormatConverterNode uses bytemuck
- **THEN** conversion SHALL use zero-copy casts where possible

### Requirement: Python SDK Transparency
The system SHALL accelerate pipelines with zero code changes required.

#### Scenario: Existing pipeline works unchanged
- **GIVEN** Python pipeline using AudioResampleNode
- **WHEN** user runs pipeline.run()
- **THEN** Rust ResampleNode SHALL execute automatically
- **AND** user code requires ZERO changes

#### Scenario: Automatic runtime selection
- **GIVEN** manifest with runtime_hint: "auto"
- **WHEN** executor selects runtime
- **THEN** Rust native SHALL be used if available, else CPython

## REMOVED Requirements

### Requirement: RustPython VM Support
**Reason**: CPython via PyO3 is superior (full stdlib, faster, simpler)  
**Migration**: Delete `runtime/src/python/vm.rs` and `rustpython_executor.rs`

### Requirement: WASM Browser Execution
**Reason**: No user demand, premature optimization  
**Migration**: Archive `openspec/changes/implement-pyo3-wasm-browser/` branch

### Requirement: WebRTC P2P Transport
**Reason**: gRPC sufficient for current use cases  
**Migration**: Defer indefinitely, use simple gRPC for remote execution

### Requirement: Pipeline Mesh Architecture
**Reason**: Over-engineered for current scale  
**Migration**: Defer until multi-datacenter deployment required

## Notes

### Performance Targets

| Operation | Python Baseline | Rust Target | Measured |
|-----------|-----------------|-------------|----------|
| VAD (30ms frames) | 5ms/frame | <50μs/frame | TBD |
| Resample 48kHz→16kHz | 100ms/sec | <2ms/sec | TBD |
| Format i16→f32 | 10ms/1M samples | <100μs/1M samples | TBD |
| FFI overhead | N/A | <1μs/call | ✅ 0.8μs |

### Simplification Wins

- **Code reduction**: -70% (35,000 → 15,000 LoC)
- **Complexity**: Single runtime path (Rust + CPython)
- **Maintenance**: No multi-target builds
- **Testing**: Single test suite (no WASM/WebRTC)

### Dependencies

- `tokio` - Async runtime
- `PyO3 0.26` - Python FFI (existing)
- `rubato` - Audio resampling
- `rustfft` - FFT for VAD
- `bytemuck` - Zero-copy format conversion
- `thiserror` + `anyhow` - Error handling
- `criterion` - Benchmarking

### Testing Strategy

1. **Unit tests**: Each node (Rust tests)
2. **Integration tests**: Python → Rust → Python roundtrip
3. **Performance tests**: Regression detection with criterion
4. **Compatibility tests**: All existing examples work unchanged
