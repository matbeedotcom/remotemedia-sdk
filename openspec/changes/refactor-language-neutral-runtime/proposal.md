# Proposal: Refactor to Language-Neutral Runtime Architecture

## Why

RemoteMedia SDK currently operates as a Python-centric framework with gRPC-based remote execution. While functional, this architecture has limitations:

1. **Limited portability**: Python nodes cannot execute in browsers, edge devices, or constrained environments
2. **Transport inflexibility**: gRPC works well for RPC but is suboptimal for real-time media streaming
3. **Packaging complexity**: No standardized way to package, distribute, and cache pipeline artifacts
4. **Execution model rigidity**: All nodes execute in Python runtime, limiting security isolation and cross-language interoperability

The vision is to evolve RemoteMedia into a **language-neutral runtime for distributed AI pipelines** following the model:

> **Python as authoring language → Rust as executor → WASM as sandbox → WebRTC as transport → OCI as distribution**

This positions RemoteMedia as "OCI for AI pipelines" with transparent remote execution, portable WASM sandboxing, and real-time WebRTC streaming.

## What Changes

This is a **major architectural refactoring** spanning multiple subsystems:

### 1. **Rust Runtime Executor** (NEW)
- Rust-based pipeline execution engine that interprets JSON manifests
- Manages RustPython VMs for Python node execution (backward compatibility)
- Hosts WASM runtime (Wasmtime/Wasmer) for portable node execution
- Handles pipeline orchestration, concurrency, and lifecycle management

### 2. **Pipeline Packaging System** (NEW)
- `.rmpkg` OCI-style package format containing:
  - `manifest.json` - Pipeline SDL (System Description Language)
  - `modules/` - Node binaries (.wasm, .pynode)
  - `models/` - Optional model weights
  - `meta/` - Signatures, provenance, runtime metadata
- CLI commands: `remotemedia build`, `remotemedia push`, `remotemedia pull`
- Automatic caching in `~/.remotemedia/pipelines/<sha256>/`

### 3. **WebRTC Transport Layer** (NEW)
- Replace/augment gRPC with WebRTC for real-time media streaming
- Automatic peer negotiation and signaling
- Data channels for pipeline control messages
- Media tracks for audio/video streams
- DTLS-SRTP end-to-end encryption
- **Pipeline Mesh Architecture**: Pipelines connect to other pipelines over WebRTC
  - Each pipeline can be both producer and consumer
  - Cascading execution (client → server → GPU executor)
  - Dynamic topology with hot-swapping
  - Load balancing across pipeline instances

### 4. **WASM Execution Sandbox** (NEW)
- Portable node execution in WASM sandbox
- Strict resource limits (memory, CPU, execution time)
- RustPython VM for Python nodes (maintains compatibility)
- Security isolation for untrusted code

### 5. **Python SDK Changes** (MODIFIED)
- Pipeline serialization to JSON manifest via `p.serialize()`
- `Pipeline.export()` to build `.rmpkg` packages
- `Pipeline.push()` to publish to registry
- New node types: `WasmNode`, `RustNode`
- Enhanced `RemoteExecutorConfig` with WebRTC support
- **BREAKING**: `Pipeline.run()` behavior changes to support Rust runtime

### 6. **Developer Experience** (MODIFIED)
- **Zero-config by default**: Auto-discovery, auto-caching, auto-transport
- One-liner usage: `p.run()` handles everything
- `host` parameter for remote execution: `HFPipelineNode("model", host="ai.example.com")`
- Optional `.remotemedia/config.json` for advanced users only

### 7. **Registry & Distribution** (NEW)
- OCI-compatible registry for `.rmpkg` artifacts
- Signature verification before execution
- Peer-to-peer pipeline transfer via WebRTC data channels
- Automatic cache warming on reference

## Impact

### Affected Capabilities (New Specs)
- **runtime-executor**: Rust-based pipeline execution engine
- **python-rust-interop**: FFI layer, data marshaling, RustPython VM management
- **pipeline-packaging**: OCI-style packaging and distribution
- **webrtc-transport**: Real-time streaming transport layer
- **pipeline-mesh**: Pipeline-to-pipeline connectivity and mesh architecture
- **capability-scheduling**: Automatic executor selection based on resource requirements
- **wasm-sandbox**: Portable, isolated node execution

### Affected Code (Existing)
- `python-client/remotemedia/core/pipeline.py` - Add serialization methods
- `python-client/remotemedia/core/node.py` - Support new node types
- `python-client/remotemedia/remote/` - Add WebRTC transport
- `service/` - Gradual migration to Rust runtime
- CLI tools - New `build`, `push`, `pull` commands
- **BREAKING**: Existing pipelines will need minimal updates for new runtime

### Migration Path
1. **Phase 1 (MVP)**: Rust runtime with RustPython compatibility layer (no code changes required)
2. **Phase 2**: WebRTC transport (opt-in via `transport="webrtc"`)
3. **Phase 3**: WASM sandboxing (opt-in for specific nodes)
4. **Phase 4**: Full OCI packaging and registry support
5. **Phase 5**: Deprecate pure-Python execution mode

### Breaking Changes

**Important**: While the internal execution model changes significantly, the **user-facing Python API remains largely compatible**. Users continue to write Python code and call `pipeline.run()` - the manifest generation and Rust execution happen transparently behind the scenes.

- **BREAKING** (Internal): Pipeline execution model changes from direct Python to manifest-based (transparent to users)
- **BREAKING**: Remote execution API extends to support WebRTC (additive, gRPC still works)
- **BREAKING**: Node discovery requires new manifest format (handled automatically during pipeline construction)
- **BREAKING**: Existing `.rmpkg` format (if any) replaced with OCI-compatible format

### User Experience Impact

**What Users Continue To Do (No Changes)**:
```python
from remotemedia import Pipeline, AudioSource, HFPipelineNode, AudioSink

p = Pipeline("voice_assistant")
p.add(AudioSource(device="default"))
p.add(HFPipelineNode("speech_to_text", host="ai.example.com"))
p.add(AudioSink(device="default"))

p.run()  # ✅ Still works exactly the same way!
```

**What Happens Behind The Scenes (Transparent)**:
1. `Pipeline.run()` automatically serializes to JSON manifest
2. Rust runtime loads manifest and executes via RustPython
3. Results return to Python just like before
4. User sees no difference in behavior

**Optional New Capabilities (Opt-In)**:
- `p.serialize()` - Export manifest for inspection (new)
- `p.export()` - Build `.rmpkg` package (new)
- `p.push()` - Publish to registry (new)
- `transport="webrtc"` - Use WebRTC instead of gRPC (opt-in)
- `WasmNode`, `RustNode` - New node types (opt-in)

### Non-Breaking Features
- Existing Python nodes continue to work via RustPython (zero code changes)
- gRPC transport remains default and fully supported
- Current `RemoteExecutorConfig` API extended, not replaced
- Backward compatibility layer ensures existing pipelines work unchanged

## Success Criteria

1. **Developer Experience**: Existing example pipelines run with **zero code changes** (manifest generation is transparent)
2. **Performance**: Rust runtime executes pipelines ≥2x faster than pure Python
3. **Portability**: Same pipeline runs on: Linux server, macOS client, browser (WASM)
4. **Zero Config**: `pip install remotemedia && python pipeline.py` works out-of-box
5. **Security**: All remote pipelines run in WASM sandbox with verified signatures

## Open Questions

1. **Registry hosting**: Self-hosted vs managed service vs hybrid?
2. **WASM runtime**: Wasmtime vs Wasmer vs custom?
3. **RustPython limitations**: Which Python features are unsupported?
4. **WebRTC signaling**: STUN/TURN server requirements and defaults?
5. **Migration timeline**: How long to maintain dual Python/Rust runtime support?
6. **Versioning**: Semantic versioning for manifests and `.rmpkg` format?

## Timeline Estimate

- **Phase 1 (MVP - Rust Runtime)**: 6-8 weeks
- **Phase 2 (WebRTC Transport)**: 4-6 weeks
- **Phase 3 (WASM Sandbox)**: 3-4 weeks
- **Phase 4 (OCI Packaging)**: 4-5 weeks
- **Phase 5 (Full Migration)**: 2-3 weeks
- **Total**: ~5-6 months for complete implementation

## Dependencies

- Rust 1.70+ (stable toolchain)
- RustPython 0.3+
- Wasmtime 15+ or Wasmer 4+
- WebRTC libraries (webrtc-rs or similar)
- OCI registry (Harbor, Docker Registry, or custom)

## References

- Updated spec: `updated_spec/remotemedia_dev_spec.md`
- Remote execution spec: `updated_spec/remotemedia_remote_exec_spec.md`
- Current project context: `openspec/project.md`
