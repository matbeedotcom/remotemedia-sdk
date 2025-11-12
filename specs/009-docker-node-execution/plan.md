# Implementation Plan: Docker-Based Node Execution with iceoryx2 IPC

**Branch**: `009-docker-node-execution` | **Date**: 2025-11-11 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/009-docker-node-execution/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This feature enables executing Python pipeline nodes in isolated Docker containers while maintaining zero-copy data transfer performance with the host runtime via iceoryx2 shared memory IPC. The implementation extends the existing multiprocess execution architecture (`runtime-core/src/python/multiprocess/`) to support containerized node execution with shared containers across sessions, container image caching, and strict resource limit enforcement.

## Technical Context

**Language/Version**: Rust 1.87 (runtime-core), Python 3.9-3.11 (node containers)
**Primary Dependencies**:
- Docker SDK/API client (Rust): `bollard` or `shiplift` [NEEDS CLARIFICATION: which Rust Docker client library?]
- iceoryx2 0.7.0 (existing, for shared memory IPC)
- tokio 1.35 (existing async runtime)
- serde/serde_json (existing, for manifest parsing)

**Storage**:
- Docker images (local Docker daemon storage)
- Container image cache metadata (in-memory HashMap with persistence to disk for reuse across runtime restarts) [NEEDS CLARIFICATION: persistence strategy?]
- Session-to-container reference counting (in-memory, tied to GLOBAL_SESSIONS)

**Testing**:
- `cargo test` (unit tests for Docker executor, container lifecycle)
- Integration tests (container + iceoryx2 IPC with real Python nodes)
- Benchmark tests (latency comparison: multiprocess vs docker)

**Target Platform**: Linux x86_64 (initial implementation - Windows/macOS out of scope per spec)

**Project Type**: Single workspace project (runtime-core library)

**Performance Goals**:
- Container startup: <5 seconds for cached images (SC-005)
- Data transfer latency: within 5ms of multiprocess nodes (SC-001)
- End-to-end pipeline latency: <100ms for 3 Docker nodes (SC-002)
- Zero-copy memory transfer validated (SC-003)

**Constraints**:
- Must maintain backward compatibility with existing multiprocess executor API
- Docker daemon must be available on host
- Linux-only initial implementation (iceoryx2 + Docker volume mounts)
- Strict resource limits enforced via Docker hard limits

**Scale/Scope**:
- Support 5+ concurrent pipeline sessions with Docker nodes (SC-007)
- Handle 3+ Docker nodes per pipeline
- 100+ consecutive sessions without resource leaks (SC-006)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Since no custom constitution file exists, applying standard Rust library development principles:

### Library Design Principles

**✅ PASS**: Feature extends existing `runtime-core` library with new `docker` executor module
- Follows existing pattern: `runtime-core/src/python/multiprocess/` → `runtime-core/src/python/docker/`
- Maintains transport-agnostic design (core runtime feature, not transport-specific)

**✅ PASS**: No new external services or projects required
- Integrates with existing Docker daemon (external dependency, not our service)
- Reuses existing iceoryx2 IPC infrastructure

### Testing Requirements

**✅ PASS**: Test-first approach applicable
- Unit tests for Docker executor lifecycle, image building, container management
- Integration tests for end-to-end data flow (host → container → host via iceoryx2)
- Contract tests for manifest schema extensions (new "docker" executor type)

**⚠️ REVIEW**: Integration testing complexity
- Requires Docker daemon in CI environment
- Requires iceoryx2 RouDi broker setup in tests
- May need Docker-in-Docker for container isolation in CI

### Performance & Observability

**✅ PASS**: Performance targets measurable
- All success criteria have quantifiable metrics (SC-001 through SC-008)
- Benchmark suite to compare multiprocess vs docker latency

**✅ PASS**: Observability via existing tracing infrastructure
- Container logs streamed to host via stdout/stderr (FR-011)
- Existing `tracing` framework captures Docker executor events
- Resource violations logged with details (FR-017)

### Simplicity & Complexity

**⚠️ NEEDS JUSTIFICATION**: Additional executor type increases system complexity
- **Why needed**: Environment isolation is unsolvable without containerization (Python package conflicts)
- **Simpler alternative rejected**: Virtual environments don't provide process-level isolation or resource limits
- **Complexity contained**: Reuses 90% of multiprocess executor patterns (IPC threads, session routing)

## Project Structure

### Documentation (this feature)

```text
specs/009-docker-node-execution/
├── plan.md              # This file (/speckit.plan command output)
├── spec.md              # Feature specification (completed)
├── research.md          # Phase 0 output (pending - Docker client library selection, image build strategies)
├── data-model.md        # Phase 1 output (pending - DockerExecutor, ContainerInstance entities)
├── quickstart.md        # Phase 1 output (pending - developer guide for Docker nodes)
├── contracts/           # Phase 1 output (pending - manifest schema extensions)
│   └── manifest-docker-extension.yaml  # Docker executor manifest schema
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
runtime-core/
├── src/
│   ├── python/
│   │   ├── multiprocess/          # Existing (reuse patterns)
│   │   │   ├── multiprocess_executor.rs
│   │   │   ├── process_manager.rs
│   │   │   ├── ipc_channel.rs
│   │   │   ├── data_transfer.rs
│   │   │   └── health_monitor.rs
│   │   │
│   │   └── docker/                # New module for this feature
│   │       ├── mod.rs             # Public API, exports DockerExecutor
│   │       ├── docker_executor.rs # Implements StreamingNodeExecutor trait
│   │       ├── container_manager.rs  # Docker container lifecycle (create/start/stop/remove)
│   │       ├── image_builder.rs   # Docker image build/cache logic
│   │       ├── container_registry.rs # Shared container reference counting
│   │       ├── ipc_bridge.rs      # Adapts multiprocess IPC patterns for containers
│   │       ├── config.rs          # Docker node configuration from manifest
│   │       └── health_check.rs    # Container health monitoring
│   │
│   ├── executor/
│   │   ├── executor_bridge.rs     # Update to register DockerExecutor
│   │   └── node_executor.rs       # May need minor updates for Docker executor
│   │
│   └── manifest/
│       └── manifest.rs            # Extend ManifestNode schema for Docker config
│
├── tests/
│   ├── integration/
│   │   ├── test_docker_executor.rs      # End-to-end Docker node execution
│   │   ├── test_docker_multiprocess.rs  # Docker + multiprocess nodes together
│   │   ├── test_docker_ipc.rs           # iceoryx2 IPC with containers
│   │   └── test_docker_shared_containers.rs  # Container sharing across sessions
│   │
│   └── unit/
│       ├── test_docker_image_builder.rs
│       ├── test_docker_container_manager.rs
│       └── test_docker_config.rs
│
└── benches/
    └── bench_docker_latency.rs    # Compare multiprocess vs docker latency

docker/                            # New top-level directory for Docker assets
├── base-images/                   # Dockerfiles for standard base images
│   ├── python39.Dockerfile        # Python 3.9 + iceoryx2
│   ├── python310.Dockerfile       # Python 3.10 + iceoryx2
│   └── python311.Dockerfile       # Python 3.11 + iceoryx2
│
└── scripts/
    ├── build-base-images.sh       # Build all standard base images
    └── validate-custom-image.sh   # Validate custom base image requirements

examples/
└── docker-node/                   # Example Docker-based pipeline
    ├── manifest.yaml              # Pipeline with Docker executor nodes
    ├── custom_node.py             # Example Python node
    └── README.md                  # Usage guide
```

**Structure Decision**: Extended `runtime-core/src/python/` with new `docker/` module parallel to `multiprocess/`. This mirrors the existing executor pattern and allows maximum code reuse (IPC infrastructure, data transfer, health monitoring). Docker-specific assets (base images, build scripts) live in a new top-level `docker/` directory to keep runtime-core clean.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| New Docker executor module (adds ~7 new source files) | Python package version conflicts cannot be resolved without process/container isolation | Virtual environments share Python runtime version and don't provide resource limits or process isolation |
| Shared container architecture (reference counting, session routing) | Resource efficiency critical for production deployments with many sessions | Dedicated containers per session would exhaust host resources (5 sessions × 3 nodes = 15 containers vs 3 shared containers) |
| Docker dependency (external daemon, API complexity) | Container technology provides proven isolation, resource limits, and deployment portability | Building custom isolation (namespaces, cgroups) would require 10× development effort and maintenance burden |
| Custom base image validation logic | Security and compatibility risk from user-provided images | Rejecting custom images entirely eliminates flexibility for users with existing Docker workflows and specialized dependencies |

## Post-Design Constitution Re-evaluation

**Date**: 2025-11-11 (after Phase 1 design completion)

### Updated Assessment

**✅ PASS - Integration Testing Complexity**: Design includes mitigation strategies
- Docker-in-Docker for CI environments documented
- Integration tests scoped to essential scenarios (test_docker_executor.rs, test_docker_ipc.rs)
- Existing multiprocess test patterns can be adapted (minimal new complexity)

**✅ PASS - Complexity Justification**: Design validates necessity
- Shared container architecture reduces resource footprint by 80% (3 containers vs 15 for 5 sessions)
- Reuses 90% of multiprocess executor patterns (IPC threads, session routing, health monitoring)
- bollard library provides stable, maintained Docker API abstraction
- SQLite image cache adds <500 lines of code with proven performance benefits (4x size reduction, sub-second queries)

**✅ PASS - Backward Compatibility**: Zero breaking changes
- All existing manifests without `docker` field work unchanged
- Docker executor is pure opt-in via manifest field
- Multiprocess and Docker nodes can coexist in same pipeline

**✅ PASS - Security Posture**: Validation layers defined
- FR-016: Custom base image validation before use
- FR-014: Strict resource limits prevent resource exhaustion
- Non-root container users (UID 1000) enforced in Dockerfiles
- No privileged containers required (standard volume mounts sufficient)

### Final Verdict

**ALL GATES PASSED** - Feature design aligns with project principles:
- Library-first architecture maintained (extends runtime-core)
- Test-first approach applicable (unit, integration, benchmark tests defined)
- Complexity justified by unsolvable requirements (environment isolation)
- Observable via existing tracing infrastructure
- Performance targets are measurable and realistic

**Recommendation**: Proceed to task generation (`/speckit.tasks`)
