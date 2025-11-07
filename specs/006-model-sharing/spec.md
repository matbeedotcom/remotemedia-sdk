# Feature Specification: Model Registry and Shared Memory Tensors

**Feature Branch**: `006-model-sharing`  
**Created**: 2025-01-08  
**Status**: Draft  
**Input**: User description: "Add model registry and shared memory tensor support for efficient model sharing across nodes, including process-local ModelRegistry, model worker pattern for cross-process reuse, and SHM/DLPack tensor plumbing"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Process-Local Model Sharing (Priority: P1)

Multiple nodes within the same process can share a single loaded model instance, reducing memory usage and initialization time.

**Why this priority**: This is the simplest form of model sharing that provides immediate value on all platforms (Windows/Mac/Linux) without additional infrastructure. It solves the most common case where multiple pipeline nodes need the same model.

**Independent Test**: Can be fully tested by running two nodes that use the same model in a single pipeline and verifying only one model instance is loaded in memory.

**Acceptance Scenarios**:

1. **Given** a pipeline with two nodes requiring the same model, **When** the pipeline executes, **Then** only one model instance is loaded in memory
2. **Given** a loaded model in the registry, **When** a second node requests the same model, **Then** the node receives a reference to the existing model without re-loading
3. **Given** a model is no longer needed by any nodes, **When** the last reference is released, **Then** the model is automatically unloaded from memory

---

### User Story 2 - Cross-Process Model Worker (Priority: P2)

A dedicated worker process owns a model and serves requests from multiple client processes, enabling GPU-efficient model sharing across process boundaries.

**Why this priority**: Critical for production deployments where multiple services need the same GPU-resident model. Prevents GPU memory duplication and enables better resource utilization.

**Independent Test**: Can be tested by starting a model worker process and having multiple client processes send inference requests, verifying all responses come from the single model instance.

**Acceptance Scenarios**:

1. **Given** a model worker process is running, **When** a client node sends an inference request, **Then** the worker processes it and returns results
2. **Given** multiple client processes, **When** they simultaneously request inference from the same worker, **Then** all requests are served by the single model instance
3. **Given** a model worker crashes, **When** clients attempt to connect, **Then** they receive a clear error indicating the worker is unavailable

---

### User Story 3 - Shared Memory Tensor Transfer (Priority: P2)

Tensors can be transferred between processes using shared memory, eliminating serialization overhead for large data transfers.

**Why this priority**: Essential for high-throughput pipelines processing video or large audio batches. Reduces latency and CPU usage significantly.

**Independent Test**: Can be tested by transferring a large tensor between processes and verifying zero-copy semantics through performance metrics.

**Acceptance Scenarios**:

1. **Given** a tensor in shared memory, **When** another process accesses it, **Then** no data copying occurs
2. **Given** a system without shared memory support, **When** tensor transfer is requested, **Then** the system falls back to standard serialization
3. **Given** a shared memory region, **When** all processes release it, **Then** the memory is automatically freed

---

### User Story 4 - Python Zero-Copy Integration (Priority: P3)

Python nodes can exchange tensors with the runtime using zero-copy mechanisms via DLPack or NumPy array interface.

**Why this priority**: Enables efficient integration with Python ML frameworks (PyTorch, TensorFlow) commonly used in research environments.

**Independent Test**: Can be tested by passing NumPy arrays to/from Python nodes and verifying no data copying occurs through memory profiling.

**Acceptance Scenarios**:

1. **Given** a NumPy array in Python, **When** passed to the runtime, **Then** the data is accessible without copying
2. **Given** a tensor from the runtime, **When** accessed in Python as NumPy array, **Then** it shares the same memory
3. **Given** a PyTorch tensor, **When** converted via DLPack, **Then** the runtime can access it without copying

---

### Edge Cases

- What happens when requested model exceeds available memory?
- How does system handle concurrent model loading requests for the same model?
- What happens when a model worker process dies during inference?
- How does the system handle shared memory limits on the operating system?
- What happens when processes have different tensor layout expectations (row-major vs column-major)?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a registry that maintains a single instance of each model per process
- **FR-002**: System MUST support reference counting for loaded models to enable automatic cleanup
- **FR-003**: System MUST allow models to be shared across nodes within the same process
- **FR-004**: System MUST support model worker processes that can serve inference requests from multiple clients
- **FR-005**: System MUST transfer tensors between processes using shared memory when available
- **FR-006**: System MUST provide fallback to serialization when shared memory is unavailable
- **FR-007**: System MUST support zero-copy tensor exchange with Python via standard interfaces (NumPy array interface or DLPack)
- **FR-008**: System MUST track memory usage and provide metrics for loaded models
- **FR-009**: System MUST handle worker process failures gracefully with clear error reporting
- **FR-010**: System MUST support configurable eviction policies for model unloading (LRU, reference count, manual)
- **FR-011**: System MUST enforce memory quotas per session for shared memory allocations
- **FR-012**: System MUST support batching of inference requests in model workers
- **FR-013**: System MUST provide capability detection for shared memory and GPU availability
- **FR-014**: System MUST maintain session isolation for shared resources

### Key Entities *(include if feature involves data)*

- **Model**: Represents a loaded ML model with its weights, configuration, and device placement
- **ModelHandle**: A reference-counted handle to a loaded model that nodes use for inference
- **ModelWorker**: A process that owns a model and serves inference requests
- **SharedTensor**: A tensor backed by shared memory with metadata (shape, dtype, strides, storage location)
- **ModelRegistry**: Process-local registry maintaining loaded models and their reference counts
- **SharedMemoryRegion**: A memory region that can be accessed by multiple processes

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Memory usage reduced by at least 60% when multiple nodes use the same model compared to loading separate instances
- **SC-002**: Model loading time reduced to under 100ms for subsequent nodes requesting an already-loaded model
- **SC-003**: Tensor transfer between processes achieves at least 10GB/s throughput for large tensors (>10MB) when using shared memory
- **SC-004**: Zero-copy tensor operations complete in under 1ms regardless of tensor size
- **SC-005**: Model worker can handle at least 100 concurrent inference requests without degradation
- **SC-006**: System automatically frees unused models within 30 seconds of last reference release
- **SC-007**: 95% reduction in serialization overhead for tensor transfers when shared memory is available
- **SC-008**: Python integration adds less than 5% overhead compared to native tensor operations