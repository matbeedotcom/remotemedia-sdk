# Feature Specification: Docker-Based Node Execution with iceoryx2 IPC

**Feature Branch**: `009-docker-node-execution`
**Created**: 2025-11-11
**Status**: Draft
**Input**: User description: "The capability for a Node to be executed in a docker container with its own python environment, with full iceoryx2 IPC shared memory support: https://iceoryx.io/v2.0.2/examples/icedocker/ This should allow us to share data between containers and the host - hopefully"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Deploy Isolated Python Node Environment (Priority: P1)

A pipeline developer needs to run a Python processing node that requires specific Python package versions that conflict with other nodes or the host system. They configure the node to run in a Docker container with its own Python environment while maintaining full data transfer performance with the host-based pipeline runtime.

**Why this priority**: This is the core value proposition - environment isolation is the primary driver for containerization. Without this, the feature has no purpose.

**Independent Test**: Can be fully tested by configuring a single node to run in a container, sending data from the host runtime, and verifying the node processes the data and returns results via shared memory IPC at the same performance level as multiprocess nodes.

**Acceptance Scenarios**:

1. **Given** a pipeline manifest specifies a node with executor type "docker" and a Python environment definition, **When** the pipeline is initialized, **Then** the system creates a Docker container with the specified Python environment and the node becomes ready to process data
2. **Given** a Docker-based node is running, **When** the host runtime sends audio data to the node, **Then** the node receives the data via shared memory IPC without serialization overhead
3. **Given** a Docker-based node processes data and yields outputs, **When** outputs are generated, **Then** the host runtime receives outputs via shared memory IPC and routes them to downstream nodes
4. **Given** a pipeline session terminates, **When** cleanup is initiated, **Then** the Docker container stops gracefully and releases all shared memory resources

---

### User Story 2 - Support Multiple Concurrent Container Nodes (Priority: P2)

A pipeline developer creates a streaming pipeline with multiple Python nodes, each requiring different Python environments (e.g., one using PyTorch 1.x, another using PyTorch 2.x). Each node runs in its own isolated Docker container while all sharing data with the host runtime and each other via iceoryx2 IPC.

**Why this priority**: This enables real-world production scenarios where environment conflicts are common. It validates that the architecture scales beyond a single container.

**Independent Test**: Can be tested by creating a pipeline with 2-3 Docker-based nodes with different Python environments, sending data through the pipeline, and verifying data flows correctly between host and all containers.

**Acceptance Scenarios**:

1. **Given** a pipeline manifest specifies multiple nodes with executor type "docker" and different Python environment definitions, **When** the pipeline is initialized, **Then** the system creates separate Docker containers for each node with isolated environments
2. **Given** multiple Docker-based nodes are running, **When** data flows through the pipeline, **Then** each node processes data in sequence and outputs are correctly routed between containers and host
3. **Given** Docker-based nodes are processing data concurrently, **When** monitoring system resources, **Then** each container's resource usage is isolated and measurable independently
4. **Given** one Docker-based node fails, **When** the failure is detected, **Then** other Docker-based nodes continue operating without disruption

---

### User Story 3 - Persist and Reuse Container Images (Priority: P3)

A pipeline developer frequently runs the same pipeline configuration with the same Python node environments. The system builds Docker images once and reuses them across multiple pipeline sessions, reducing startup time from minutes to seconds.

**Why this priority**: This is an optimization for developer experience but not strictly necessary for basic functionality. Initial implementation can rebuild containers each time.

**Independent Test**: Can be tested by running the same pipeline configuration twice and measuring that the second run starts significantly faster due to image reuse.

**Acceptance Scenarios**:

1. **Given** a Docker-based node configuration is used for the first time, **When** the pipeline is initialized, **Then** the system builds a Docker image and tags it with a unique identifier based on the environment definition
2. **Given** the same node configuration is used again, **When** the pipeline is initialized, **Then** the system detects the existing Docker image and reuses it instead of rebuilding
3. **Given** a Python environment definition changes, **When** the pipeline is initialized, **Then** the system builds a new Docker image rather than reusing the old one
4. **Given** accumulated Docker images consume significant storage, **When** cleanup is requested, **Then** the system removes unused images while preserving those used by active or recent sessions

---

### Edge Cases

- What happens when a Docker container fails to start due to environment errors (e.g., missing base image, invalid package specifications)?
- How does the system handle Docker daemon being unavailable or not responding?
- What happens when a shared Docker container crashes mid-session - how are all sessions using that container notified and recovered?
- How are iceoryx2 shared memory resources cleaned up if a shared container terminates unexpectedly (e.g., OOM killed due to exceeding memory limits)?
- What happens when the host system runs out of resources to create additional containers?
- How does the system handle conflicting port bindings if nodes require network access?
- What happens when a custom base image is provided but fails validation (missing iceoryx2 libraries) - is there a fallback or immediate failure?
- How are Python dependencies that require system libraries (e.g., libsndfile, ffmpeg) handled in the container - must they be specified in the manifest?
- What happens if two sessions start simultaneously with the same node configuration - does the second wait for the first to finish building/starting the container?
- How does reference counting handle race conditions when the last session terminates while a new session is starting?
- What happens when a container is killed due to strict resource limits while processing critical data - is the data lost or can it be retried?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support executing Python nodes in isolated Docker containers while maintaining the existing multiprocess execution architecture for non-containerized nodes
- **FR-002**: System MUST enable data transfer between host runtime and Docker-based nodes using iceoryx2 shared memory IPC with the same zero-copy performance as multiprocess nodes
- **FR-003**: System MUST support data transfer between multiple Docker-based nodes running in separate containers via iceoryx2 shared memory IPC
- **FR-004**: System MUST allow pipeline manifests to specify node executor type as "docker" with environment configuration including Python version, system dependencies, and Python package requirements
- **FR-005**: System MUST mount required host filesystem paths (/tmp, /dev) into Docker containers to enable iceoryx2 IPC functionality across container boundaries
- **FR-006**: System MUST register each Docker-based node with the iceoryx2 broker (RouDi) using unique runtime names to prevent conflicts across containers
- **FR-007**: System MUST handle Docker container lifecycle (create, start, stop, remove) as part of pipeline session initialization and cleanup
- **FR-008**: System MUST validate that Docker daemon is available and accessible before attempting to create containerized nodes
- **FR-009**: System MUST propagate node failures from Docker containers to the host runtime's error handling and retry mechanisms
- **FR-010**: System MUST ensure each Docker-based node's iceoryx2 channels use session-scoped naming (format: `{session_id}_{node_id}_input/output`) to prevent cross-session conflicts
- **FR-011**: System MUST stream container logs (stdout/stderr) to the host runtime's logging system for debugging and monitoring
- **FR-012**: System MUST share running containers across multiple pipeline sessions that use identical node configurations (same Python environment, dependencies, and node type), routing each session's data to the shared container via unique session-scoped iceoryx2 channels
- **FR-013**: System MUST provide standard Docker base images with iceoryx2 pre-installed for common Python versions (3.9, 3.10, 3.11), and MUST support custom base images for advanced users with documented requirements (iceoryx2 client libraries installed, specific system paths accessible)
- **FR-014**: System MUST enforce resource limits for Docker containers strictly using Docker's hard limit mechanism - containers that exceed configured memory or CPU limits are terminated immediately to prevent resource exhaustion
- **FR-015**: System MUST maintain a reference count for shared containers - containers are only stopped and removed when all sessions using them have terminated
- **FR-016**: System MUST validate custom Docker base images before use by checking for required iceoryx2 client library installation and accessible system paths (/tmp, /dev)
- **FR-017**: System MUST provide clear error messages when containers are terminated due to resource limit violations, including which limit was exceeded and by how much

### Key Entities

- **Dockerized Node Configuration**: Specifies executor type, Python version, system dependencies, Python packages, resource limits, and optional custom base image. Defined in pipeline manifest and used to build or select Docker images.
- **Container Session Instance**: Represents a running Docker container for a specific node within a pipeline session. Tracks container ID, health status, iceoryx2 channel names, and lifecycle state.
- **iceoryx2 IPC Channel**: Shared memory communication channel for data transfer between host runtime and Docker containers, or between containers. Uses session-scoped naming to prevent conflicts.
- **Container Image Cache**: Registry of built Docker images identified by environment configuration hash. Enables reuse across pipeline sessions to reduce startup time.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Pipeline developers can configure a Python node to run in Docker with a custom Python environment and successfully process streaming audio data with the same latency characteristics as multiprocess nodes (within 5ms difference)
- **SC-002**: A pipeline with 3 Docker-based nodes (each with different Python environments) can process streaming audio data end-to-end with total latency under 100ms
- **SC-003**: Docker-based nodes achieve zero-copy data transfer performance - memory usage remains constant regardless of data volume (no duplication), validated by monitoring container memory during 1-minute streaming session
- **SC-004**: System handles Docker container failures gracefully - when a container crashes, error is reported within 2 seconds and retry logic is triggered automatically
- **SC-005**: Container startup time for a previously-built image is under 5 seconds from pipeline initialization to node ready state
- **SC-006**: System correctly cleans up all Docker containers and shared memory resources when a pipeline session terminates - verified by zero orphaned containers or iceoryx2 service files after 100 consecutive sessions
- **SC-007**: Multiple pipeline sessions can run concurrently with Docker-based nodes without resource conflicts or data routing errors - validated by running 5 sessions simultaneously for 10 minutes
- **SC-008**: Developers can inspect container logs through the host runtime's logging interface - all stdout/stderr from containerized nodes appears in host logs with correct timestamps and node identifiers

## Assumptions

- Docker daemon is installed and running on the host system
- Host system has sufficient resources (CPU, memory, disk) to run multiple containers concurrently
- iceoryx2 RouDi broker can be accessed from both host and containers via shared /tmp mount
- Python node code is available to be copied into containers during image build
- Network connectivity is not required for basic node execution (nodes communicate only via IPC)
- Linux host system (initial implementation) - Windows/macOS support is out of scope
- Standard base images are pre-built and available locally or can be pulled from a registry
- Custom base images provided by users have compatible iceoryx2 client library versions with the host's RouDi version
- Container resource limits are set appropriately by users based on node workload characteristics
- Shared containers can handle multiple concurrent sessions without performance degradation (session data is isolated via iceoryx2 channel naming)

## Out of Scope

- GPU access from Docker containers (future enhancement)
- Kubernetes or container orchestration platform support (Docker-only initially)
- Custom networking configurations for containers (host network mode assumed)
- Container image distribution via registries (local build only)
- Support for non-Python runtimes in containers (Rust, C++, etc.)
- Live migration of containers between hosts
- Distributed pipeline execution across multiple physical hosts
