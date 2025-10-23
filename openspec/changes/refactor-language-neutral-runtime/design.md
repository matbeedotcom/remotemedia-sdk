# Design Document: Language-Neutral Runtime Architecture

## Context

RemoteMedia SDK began as a Python-first framework for distributed audio/video processing. While successful, the architecture has inherent limitations:

- **Execution Model**: Pure Python execution limits performance and portability
- **Transport**: gRPC works for RPC but is suboptimal for real-time media
- **Distribution**: No standardized packaging or caching mechanism
- **Security**: Limited isolation for untrusted code

The refactoring aims to transform RemoteMedia into a **language-neutral runtime** following modern distributed systems patterns:

> **Python (SDL) → Rust (Executor) → WASM (Sandbox) → WebRTC (Transport) → OCI (Distribution)**

### Stakeholders
- **SDK Users**: Developers building AI/media pipelines
- **Node Authors**: Contributors creating custom processing nodes
- **Service Operators**: Teams deploying RemoteMedia services
- **Platform Engineers**: Those integrating RemoteMedia into larger systems

### Constraints
- **Backward Compatibility**: Existing Python nodes must continue working
- **Performance**: Rust runtime must achieve ≥2x speedup vs pure Python
- **Developer Experience**: Zero-config defaults, opt-in complexity
- **Security**: All remote code must run in verified sandboxes
- **Timeline**: ~5-6 months for full implementation

## Goals / Non-Goals

### Goals
1. **Portable Execution**: Run pipelines on servers, edge devices, and browsers
2. **Real-Time Streaming**: WebRTC transport for low-latency audio/video
3. **Security Isolation**: WASM sandbox for untrusted code
4. **Zero Configuration**: Auto-discovery, auto-caching, auto-transport
5. **OCI Distribution**: Standard packaging format for pipelines
6. **Performance**: 2x+ speedup via Rust execution
7. **Backward Compatibility**: RustPython layer for existing nodes

### Non-Goals
1. **Immediate Python Deprecation**: Python remains first-class citizen
2. **Breaking All APIs**: Most APIs remain compatible with extensions
3. **Custom Transport Protocol**: Use standard WebRTC/gRPC
4. **Proprietary Registry**: Support standard OCI registries
5. **Rewriting All Nodes**: Use RustPython for compatibility
6. **Browser-First Design**: Browser support is bonus, not primary

## Decisions

### Decision 1: Rust for Runtime Execution
**Choice**: Rust as the primary execution engine

**Rationale**:
- **Performance**: Native performance with zero-cost abstractions
- **Safety**: Memory safety without GC pauses
- **Concurrency**: Excellent async/await with tokio
- **WASM Ecosystem**: First-class WASM tooling (Wasmtime, Wasmer)
- **WebRTC**: Mature webrtc-rs library

**Alternatives Considered**:
- **C++**: Rejected due to memory safety concerns and slower development
- **Go**: Rejected due to GC pauses impacting real-time processing
- **Pure Python**: Rejected due to performance limitations
- **JVM (Kotlin/Java)**: Rejected due to startup time and memory overhead

**Trade-offs**:
- ✅ Better performance and safety
- ✅ Modern concurrency primitives
- ❌ Steeper learning curve for contributors
- ❌ Additional build complexity

### Decision 2: RustPython for Backward Compatibility
**Choice**: Embed RustPython VM for existing Python nodes

**Rationale**:
- **Zero Migration**: Existing nodes work without changes
- **Pure Rust**: No CPython dependency in runtime
- **Gradual Migration**: Users can migrate to WASM/Rust over time
- **Security**: Better sandboxing than CPython

**Alternatives Considered**:
- **PyO3/CPython Embedding**: Rejected due to GIL and deployment complexity
- **Force Rewrite**: Rejected due to breaking change impact
- **Python Transpilation**: Rejected due to incomplete compatibility

**Trade-offs**:
- ✅ Backward compatible
- ✅ No CPython dependency
- ❌ RustPython has some stdlib gaps
- ❌ Slightly slower than CPython for some workloads

### Decision 3: WebRTC for Media Transport
**Choice**: WebRTC as primary transport for streaming pipelines

**Rationale**:
- **Real-Time**: Built for low-latency audio/video
- **NAT Traversal**: Built-in STUN/TURN support
- **Browser Support**: Enables browser-based pipelines
- **Encryption**: DTLS-SRTP by default
- **Adaptive**: Automatic quality adjustment

**Alternatives Considered**:
- **gRPC Streaming**: Keep as fallback, but lacks media optimizations
- **Raw UDP/RTP**: Rejected due to complexity and lack of NAT traversal
- **WebSockets**: Rejected due to higher latency vs WebRTC
- **Custom Protocol**: Rejected due to reinventing the wheel

**Trade-offs**:
- ✅ Optimal for real-time media
- ✅ Browser compatibility
- ✅ Built-in security
- ❌ More complex signaling setup
- ❌ Firewall/NAT complexity

### Decision 4: Wasmtime for WASM Runtime
**Choice**: Wasmtime as the WASM execution engine

**Rationale**:
- **Bytecode Alliance**: Industry-standard, well-maintained
- **Performance**: Cranelift JIT compiler
- **Security**: Strong isolation guarantees
- **WASI Support**: Complete WASI implementation
- **Rust Integration**: Excellent Rust bindings

**Alternatives Considered**:
- **Wasmer**: Rejected due to less mature Rust API
- **wasm3**: Rejected due to interpreter-only (no JIT)
- **V8**: Rejected due to JavaScript coupling and size

**Trade-offs**:
- ✅ Best-in-class performance
- ✅ Strong security model
- ✅ Active development
- ❌ Larger binary size
- ❌ Compile-time overhead (mitigated by caching)

### Decision 5: OCI-Compatible Package Format
**Choice**: `.rmpkg` format based on OCI standards

**Rationale**:
- **Standardization**: Reuse existing registry infrastructure
- **Tooling**: Leverage Docker registry, Harbor, etc.
- **Signing**: Built-in content trust mechanisms
- **Distribution**: CDN support, layer caching
- **Familiarity**: Developers know OCI/Docker

**Alternatives Considered**:
- **Custom Archive Format**: Rejected due to NIH syndrome
- **Python Wheels**: Rejected due to lack of multi-language support
- **Flatpak/Snap**: Rejected due to OS package focus
- **NPM Packages**: Rejected due to JavaScript coupling

**Trade-offs**:
- ✅ Industry standard
- ✅ Existing tooling
- ✅ Registry ecosystem
- ❌ Slightly heavier than custom format
- ❌ Learning curve for non-Docker users

### Decision 6: JSON for Pipeline Manifests
**Choice**: JSON as the manifest serialization format

**Rationale**:
- **Universality**: Every language can parse JSON
- **Readability**: Human-readable for debugging
- **Tooling**: Excellent editor support, validation
- **Web Compatibility**: JavaScript-native
- **Schema Validation**: JSON Schema for validation

**Alternatives Considered**:
- **YAML**: Rejected due to parsing ambiguity
- **TOML**: Rejected due to less universal support
- **Protobuf**: Rejected due to poor human readability
- **MessagePack**: Considered for internal binary format

**Trade-offs**:
- ✅ Universal support
- ✅ Human-readable
- ✅ Strong tooling
- ❌ Larger than binary formats
- ❌ No comments (use separate docs)

### Decision 7: Capability-Based Security for WASM
**Choice**: Capability tokens for WASM node permissions

**Rationale**:
- **Principle of Least Privilege**: Explicit permission grants
- **Composability**: Capabilities can be delegated
- **Auditability**: Clear permission trail
- **WASI Alignment**: Matches WASI security model
- **Fine-Grained**: Per-resource control

**Alternatives Considered**:
- **Sandboxing Only**: Rejected as insufficient for resource access
- **ACLs**: Rejected due to coarse granularity
- **User-Based**: Rejected due to multi-tenant complexity

**Trade-offs**:
- ✅ Strong security model
- ✅ Clear permissions
- ❌ More configuration needed
- ❌ Learning curve for developers

## Architecture Decisions

### Critical Design Principle: Transparent Execution

**User-Facing Simplicity, Internal Sophistication**

The fundamental design principle is: **Users write Python, see Python, think Python - while execution happens in Rust.**

```python
# User writes this simple Python code:
p = Pipeline("my_pipeline")
p.add(AudioSource())
p.add(MyProcessingNode())
p.add(AudioSink())
p.run()  # ← This is where magic happens

# Behind the scenes (100% transparent):
# 1. Pipeline.__init__ builds Python object graph
# 2. Pipeline.run() triggers:
#    a. Automatic serialization to JSON manifest
#    b. Rust runtime FFI call with manifest
#    c. RustPython executes nodes in Rust context
#    d. Results marshaled back to Python
# 3. User receives results as Python objects
```

**Key Points**:
- **No manifest files visible**: Users never see or edit JSON manifests
- **No API changes**: `Pipeline.run()` signature stays the same
- **No import changes**: Same `from remotemedia import Pipeline`
- **Drop-in replacement**: Swap runtime, code stays identical

**Optional Advanced Usage** (Opt-In Only):
```python
# Advanced users CAN inspect/export if desired:
manifest = p.serialize()        # See the manifest
p.export("pipeline.rmpkg")      # Build package
p.push("oci://registry/name")   # Publish

# But 99% of users never need to do this
```

This transparent execution model is critical because:
1. **Zero Migration Cost**: Existing code works unchanged
2. **Progressive Enhancement**: Adopt new features when needed
3. **Familiar Mental Model**: Users think in Python, not manifests
4. **Debugging Simplicity**: Stack traces show Python code, not JSON

### Layer Separation
```
┌─────────────────────────────────────────────┐
│         Python SDK (Authoring)              │  User writes pipelines
├─────────────────────────────────────────────┤
│      Rust Runtime (Execution)               │  Orchestrates nodes
├─────────────────────────────────────────────┤
│  RustPython VM    │    WASM Sandbox         │  Node execution
├─────────────────────────────────────────────┤
│   WebRTC Transport   │   gRPC Transport     │  Network communication
├─────────────────────────────────────────────┤
│       OCI Registry / Cache                  │  Package distribution
└─────────────────────────────────────────────┘
```

**Rationale**: Clear separation of concerns enables:
- Independent evolution of layers
- Testing isolation
- Platform portability
- Multi-language support

### Data Flow Architecture

**Local Execution**:
```
Python Pipeline.run()
  → Serialize to JSON manifest
    → Rust Runtime loads manifest
      → Instantiate nodes (RustPython/WASM)
        → Execute pipeline graph
          → Return results to Python
```

**Remote Execution**:
```
Python Pipeline.run(remote_config)
  → Serialize pipeline + config
    → Establish WebRTC connection
      → Transfer manifest to remote
        → Remote Rust Runtime executes
          → Stream results back via WebRTC
```

**Pipeline Mesh (Multi-Tier)**:
```
Client Browser (WASM Pipeline)
  [MicSource] → [Preprocess]
    ↓ WebRTC
Server Pipeline (Python/Rust)
  [ASR] → [EmotionDetector]
    ↓ WebRTC
GPU Executor Pipeline
  [LLMInference] → [TTSSynthesis]
    ↓ WebRTC
Server Pipeline
  [AudioRenderer]
    ↓ WebRTC
Client Browser
  [AudioSink]
```

**Key Insight**: Each → WebRTC boundary is a **pipeline calling another pipeline**.
The system creates a mesh where pipelines are first-class network peers, not just execution contexts.

### Module Organization

**Rust Runtime** (`runtime/`):
```
runtime/
├── src/
│   ├── executor/       # Pipeline execution engine
│   ├── manifest/       # JSON parsing & validation
│   ├── nodes/          # Node type implementations
│   ├── transport/      # WebRTC & gRPC clients
│   ├── wasm/           # WASM runtime integration
│   ├── python/         # RustPython integration
│   ├── cache/          # Package caching
│   └── registry/       # OCI registry client
├── tests/              # Integration tests
└── benches/            # Performance benchmarks
```

**Python SDK** (`python-client/remotemedia/`):
```
remotemedia/
├── core/
│   ├── pipeline.py     # ADD: serialize(), export()
│   ├── node.py         # ADD: WasmNode, RustNode
│   └── manifest.py     # NEW: Manifest generation
├── remote/
│   ├── webrtc.py       # NEW: WebRTC transport
│   └── config.py       # UPDATE: WebRTC support
├── packaging/
│   └── builder.py      # NEW: .rmpkg builder
└── cli/
    └── build.py        # NEW: build, push, pull commands
```

## Risks / Trade-offs

### Risk 1: RustPython Compatibility Gaps
**Risk**: Some Python stdlib features may not work in RustPython

**Impact**: Medium - Could break existing nodes

**Mitigation**:
- Comprehensive compatibility testing before MVP
- Document unsupported features clearly
- Provide CPython fallback option for development
- Contribute fixes to RustPython project

**Contingency**:
- Use PyO3 for problematic nodes
- Provide WASM compilation path as alternative

### Risk 2: WebRTC Signaling Complexity
**Risk**: WebRTC requires signaling infrastructure

**Impact**: Medium - Deployment complexity

**Mitigation**:
- Provide default hosted signaling service
- Document self-hosted signaling setup
- Auto-fallback to gRPC if WebRTC unavailable
- Support multiple signaling protocols (WebSocket, HTTP long-poll)

**Contingency**:
- Use gRPC as primary, WebRTC as opt-in

### Risk 3: WASM Performance Overhead
**Risk**: WASM may be slower than native for some workloads

**Impact**: Low-Medium - Depends on workload

**Mitigation**:
- AOT compilation where supported
- Module caching to reduce cold-start
- Benchmark critical paths
- Allow native Rust nodes for performance-critical code

**Contingency**:
- Provide native Rust node path
- Keep Python execution option

### Risk 4: Breaking Changes During Migration
**Risk**: Users may face unexpected breakage

**Impact**: High - User frustration, adoption friction

**Mitigation**:
- Maintain dual runtime support during transition (6+ months)
- Provide automated migration tools
- Comprehensive testing of existing examples
- Clear deprecation timeline and warnings
- Detailed migration guides

**Contingency**:
- Extend transition period
- Provide commercial support option

### Risk 5: Increased Build Complexity
**Risk**: Rust toolchain adds build complexity

**Impact**: Medium - Developer onboarding friction

**Mitigation**:
- Provide pre-built binaries for common platforms
- Docker images with all dependencies
- Clear setup documentation
- CI/CD templates
- Dev container configurations

**Contingency**:
- Offer cloud-hosted build service

## Migration Plan

### Phase 0: Preparation (Week 1-2)
1. Set up Rust workspace
2. Create feature branch
3. Begin RustPython compatibility testing
4. Document current architecture baselines

### Phase 1: MVP Runtime (Week 3-10)
1. Implement Rust runtime core
2. Add RustPython integration
3. Port 3 example pipelines
4. Validate backward compatibility
5. **Milestone**: Existing Python pipelines run in Rust runtime

### Phase 2: WebRTC Transport (Week 11-16)
1. Integrate WebRTC library
2. Build signaling server
3. Implement media track streaming
4. Add transport selection logic
5. **Milestone**: Real-time audio streaming via WebRTC

### Phase 3: WASM Sandbox (Week 17-20)
1. Integrate Wasmtime
2. Implement resource limits
3. Add WASI support
4. Create compilation tools
5. **Milestone**: First WASM node executes successfully

### Phase 4: OCI Packaging (Week 21-25)
1. Define .rmpkg format
2. Implement build/push/pull CLI
3. Set up example registry
4. Add caching system
5. **Milestone**: Package published and consumed from registry

### Phase 5: Polish & Release (Week 26-28)
1. Migration tools
2. Comprehensive testing
3. Documentation
4. Performance optimization
5. **Milestone**: Public beta release

### Rollback Strategy
- Feature flags for each major component
- Ability to disable Rust runtime and use pure Python
- Gradual rollout via opt-in environment variables
- Automated regression testing
- Version pinning for stability

### Deprecation Timeline
- **Month 0**: Announce refactoring plans
- **Month 3**: MVP runtime released (opt-in)
- **Month 6**: Full features complete, recommend adoption
- **Month 9**: Rust runtime becomes default
- **Month 12**: Python-only mode deprecated (still supported)
- **Month 18**: Python-only mode removed

## Performance Targets

### Latency Goals
- **Pipeline Cold Start**: <500ms (vs 2s Python)
- **Node Execution Overhead**: <5ms per node
- **WebRTC Connection Setup**: <2s
- **Package Cache Lookup**: <10ms

### Throughput Goals
- **Audio Processing**: ≥10x real-time (process 10s audio in 1s)
- **Video Processing**: 30fps at 1080p with 3-node pipeline
- **Data Throughput**: ≥100MB/s per connection

### Resource Goals
- **Memory**: ≤2x overhead vs pure Python
- **CPU**: ≥50% reduction vs Python for I/O-bound
- **Binary Size**: Runtime <50MB, minimal node <1MB

## Open Questions

### Q1: Registry Hosting Strategy
**Question**: Self-hosted, managed service, or hybrid?

**Options**:
1. **Self-hosted only**: Users deploy own registries
2. **Managed service**: Anthropic/vendor hosts public registry
3. **Hybrid**: Public registry + self-host option

**Recommendation**: Start with hybrid - public registry for examples, self-host for production

**Decision Needed By**: Phase 4 start

### Q2: WASM Compilation Default
**Question**: Should nodes be compiled to WASM by default?

**Options**:
1. **Manual opt-in**: Users explicitly compile to WASM
2. **Automatic**: Build system auto-compiles suitable nodes
3. **Hybrid**: Auto-compile for packages, manual for development

**Recommendation**: Hybrid approach

**Decision Needed By**: Phase 3 end

### Q3: Signaling Service Provider
**Question**: Who operates the default signaling service?

**Options**:
1. **Public service**: Free, hosted by project
2. **Third-party**: Use existing services (Twilio, etc.)
3. **Self-hosted only**: No default, users deploy own

**Recommendation**: Public service for development, encourage self-hosting for production

**Decision Needed By**: Phase 2 start

### Q4: Python Version Support
**Question**: Which Python versions to support in RustPython?

**Options**:
1. **3.9+ only**: Match current SDK
2. **3.7+**: Broader compatibility
3. **3.11+**: Latest features

**Recommendation**: 3.9+ to match current SDK, document limitations

**Decision Needed By**: Phase 1 start

### Q5: Versioning Strategy
**Question**: How to version manifests and packages?

**Options**:
1. **Semantic versioning**: Major.minor.patch
2. **Date-based**: YYYY.MM.DD
3. **Schema versioning**: v1, v2, etc.

**Recommendation**: Schema versioning for manifests, semver for packages

**Decision Needed By**: Phase 1 middle

## Success Metrics

### Adoption Metrics
- 80% of existing examples run without changes by MVP
- 50% of users adopt Rust runtime within 3 months of GA
- 10+ community-contributed WASM nodes within 6 months

### Performance Metrics
- 2x speedup vs Python baseline
- <100ms p95 latency for remote execution
- <5% overhead for WASM vs native

### Developer Experience Metrics
- <10 min from install to running first pipeline
- <3 line changes for typical migration
- >4.0 satisfaction score (1-5 scale)

### Reliability Metrics
- <0.1% crash rate in production
- <5s recovery time for transient failures
- >99.9% uptime for hosted services

## References

- **RustPython**: https://github.com/RustPython/RustPython
- **Wasmtime**: https://wasmtime.dev/
- **webrtc-rs**: https://github.com/webrtc-rs/webrtc
- **OCI Spec**: https://github.com/opencontainers/distribution-spec
- **WASI**: https://wasi.dev/
- **Updated Specs**: `updated_spec/remotemedia_dev_spec.md`, `updated_spec/remotemedia_remote_exec_spec.md`
