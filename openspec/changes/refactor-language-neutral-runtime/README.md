# Refactor to Language-Neutral Runtime Architecture

## Overview

This change proposal transforms RemoteMedia SDK from a Python-centric framework into a **language-neutral runtime for distributed AI pipelines**.

**Vision**: Python as authoring language → Rust as executor → WASM as sandbox → WebRTC as transport → OCI as distribution

## Quick Links

- **Proposal**: [proposal.md](./proposal.md) - Why this change is needed and high-level overview
- **Design**: [design.md](./design.md) - Technical decisions, architecture, and trade-offs
- **Tasks**: [tasks.md](./tasks.md) - Implementation checklist across 5 phases
- **Specifications**: [specs/](./specs/) - Detailed requirements for each capability

## Key Changes

### New Capabilities (7)
1. **runtime-executor**: Rust-based pipeline execution engine with RustPython support
2. **python-rust-interop**: FFI bindings, data marshaling, and RustPython VM management
3. **pipeline-packaging**: OCI-style `.rmpkg` packaging and distribution
4. **webrtc-transport**: Real-time streaming via WebRTC with automatic NAT traversal
5. **pipeline-mesh**: Pipeline-to-pipeline connectivity creating distributed media graphs
6. **capability-scheduling**: Automatic executor selection based on node resource requirements
7. **wasm-sandbox**: Portable, secure node execution in WASM sandbox

### Modified Systems
- Python SDK: Add serialization, packaging, and WebRTC APIs
- Remote execution: Support WebRTC transport alongside gRPC
- CLI: Add `build`, `push`, `pull`, `compile` commands
- Service: Gradual migration to Rust runtime

## Implementation Timeline

| Phase | Duration | Focus | Milestone |
|-------|----------|-------|-----------|
| 1 | 6-8 weeks | Rust Runtime + RustPython | Existing pipelines run in Rust |
| 2 | 4-6 weeks | WebRTC Transport | Real-time audio streaming |
| 3 | 3-4 weeks | WASM Sandbox | First WASM node executes |
| 4 | 4-5 weeks | OCI Packaging | Package published to registry |
| 5 | 2-3 weeks | Polish & Release | Public beta |
| **Total** | **~5-6 months** | | GA release |

## Breaking Changes & User Impact

⚠️ **Internal Architecture Changes** (Transparent to Users):
- Pipeline execution model (manifest-based instead of direct Python)
  - **User Impact**: None - `pipeline.run()` still works the same way
  - Manifest generation happens automatically behind the scenes
- Node discovery (new manifest format required)
  - **User Impact**: None - handled automatically during pipeline construction

⚠️ **API Extensions** (Additive, Not Breaking):
- Remote execution API (WebRTC support added)
  - **User Impact**: None by default - gRPC remains default, WebRTC is opt-in
  - Existing code continues to work unchanged
- Package format (OCI-compatible `.rmpkg`)
  - **User Impact**: Only affects users explicitly building packages

✅ **Backward Compatibility Guarantee**:
```python
# This code continues to work with ZERO changes:
from remotemedia import Pipeline, AudioSource, AudioSink

p = Pipeline("test")
p.add(AudioSource())
p.add(AudioSink())
p.run()  # ✅ Works identically in new runtime
```

✅ **Migration Support**:
- Existing Python nodes work via RustPython (zero code changes)
- gRPC transport remains default and fully supported
- Gradual adoption: opt-in to new features as needed
- 6+ month dual runtime support during transition

## Success Criteria

- [ ] Existing examples run with **zero code changes** (transparent migration)
- [ ] Rust runtime achieves ≥2x speedup vs Python
- [ ] Same pipeline runs on: Linux, macOS, Windows, browser
- [ ] Zero-config installation: `pip install remotemedia && python pipeline.py`
- [ ] All remote pipelines execute in verified WASM sandbox

## Files in This Change

```
refactor-language-neutral-runtime/
├── README.md           # This file
├── proposal.md         # Why and what changes
├── design.md           # Technical decisions and architecture
├── tasks.md            # Implementation checklist (5 phases, 300+ tasks)
└── specs/              # Detailed requirements
    ├── runtime-executor/
    │   └── spec.md     # 9 requirements, 27 scenarios
    ├── python-rust-interop/
    │   └── spec.md     # 13 requirements, 41 scenarios (Phase 1 critical)
    ├── pipeline-packaging/
    │   └── spec.md     # 9 requirements, 27 scenarios
    ├── webrtc-transport/
    │   └── spec.md     # 10 requirements, 30 scenarios
    ├── pipeline-mesh/
    │   └── spec.md     # 14 requirements, 44 scenarios (Phase 2 critical)
    ├── capability-scheduling/
    │   └── spec.md     # 14 requirements, 44 scenarios (Phase 4 critical)
    └── wasm-sandbox/
        └── spec.md     # 10 requirements, 30 scenarios
```

## How to Review

### For Stakeholders
1. Read [proposal.md](./proposal.md) for high-level understanding
2. Review **Breaking Changes** and **Migration Path** sections
3. Check **Timeline** and **Success Criteria**
4. Provide feedback on **Open Questions**

### For Architects
1. Read [design.md](./design.md) for technical decisions
2. Review **Architecture Decisions** and **Alternatives Considered**
3. Evaluate **Risks / Trade-offs**
4. Validate **Performance Targets**

### For Engineers
1. Read [tasks.md](./tasks.md) for implementation details
2. Review relevant capability specs in [specs/](./specs/)
3. Assess feasibility and provide estimates
4. Identify potential blockers

### For Users
1. Check **Breaking Changes** impact on your pipelines
2. Review **Migration Path** for upgrade strategy
3. Test backward compatibility with examples
4. Provide feedback on developer experience

## Next Steps

**DO NOT START IMPLEMENTATION** until this proposal is reviewed and approved.

### Review Process
1. **Technical Review**: Architecture and design decisions
2. **Impact Assessment**: Breaking changes and migration plan
3. **Resource Planning**: Timeline and team allocation
4. **Approval**: Sign-off from stakeholders

### After Approval
1. Create feature branch: `feat/language-neutral-runtime`
2. Set up Rust workspace and CI/CD
3. Begin Phase 1 implementation following [tasks.md](./tasks.md)
4. Track progress with task checklist
5. Regular check-ins and demos

## Questions?

- **Architecture**: See [design.md](./design.md) decisions section
- **Requirements**: See individual specs in [specs/](./specs/)
- **Implementation**: See [tasks.md](./tasks.md) checklist
- **Timeline**: See proposal.md timeline section

## Validation Status

✅ **OpenSpec Validation**: Passed strict mode
- All requirements have scenarios
- All scenarios properly formatted
- Change structure validated
- Ready for review

---

**Status**: 🟡 Awaiting Review
**Created**: 2025-10-22
**Estimated Completion**: Q2 2026 (assuming Q4 2025 start)
**Complexity**: 🔴 High (Major architectural refactoring)
