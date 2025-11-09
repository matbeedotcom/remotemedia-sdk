# Feature Specification: Code Archival & Consolidation

**Feature Branch**: `002-code-archival-consolidation`  
**Created**: 2025-10-27  
**Status**: Draft  
**Parent**: Built on top of `001-native-rust-acceleration` (v0.2.0)  
**Input**: User requirement: "Archive unused WASM/browser code while preserving WebRTC production server. Consolidate to single NodeExecutor trait. Reduce codebase from 50K to 15K LoC."

---

## Context & Motivation

### Current State (Post v0.2.0)

The remotemedia-sdk has accumulated significant complexity:
- **50,000+ lines of code**
- **3 execution runtimes**: Rust native (active), WASM/Pyodide (unused), RustPython (deleted but refs remain)
- **2 NodeExecutor traits**: `executor::node_executor::NodeExecutor` (new, clean) vs `nodes::NodeExecutor` (old, requires adapters)
- **62 files break** when Error enum changes due to dual trait system
- **WASM browser demo**: 15,000 LoC tech demo with no production users
- **WebRTC server**: Production system suffering from slow audio processing (380ms latency causing choppy responses)

### The Problem

**For the maintainer**:
- 70% of codebase is unused (WASM, old traits, historical docs)
- Every Error enum change requires updating 62 files
- Unclear what's production vs experimental
- High maintenance burden blocks new features

**For the WebRTC production use case**:
- 32-core server cannot keep up with real-time audio (choppy responses)
- Audio preprocessing takes 380ms per 10s chunk (too slow)
- v0.2.0 Rust acceleration exists (72x faster) but WebRTC server uses old API
- Current implementation uses slow Python nodes instead of fast Rust nodes

### The Solution

**Archive unused code**:
- Move WASM/browser demo to `archive/` (not deleted, just inactive)
- Archive old NodeExecutor trait and adapters
- Archive historical specification documents
- Clear READMEs explaining what was archived and why

**Consolidate architecture**:
- Single NodeExecutor trait: `executor::node_executor::NodeExecutor`
- Error enum changes affect 15 files instead of 62
- Clear separation: active code vs archived code

**Migrate WebRTC to v0.2.0**:
- Update WebRTC server to use Rust-accelerated audio nodes
- Enable `runtime_hint="rust"` for 72x speedup
- Fix choppy audio issue (380ms → 5ms processing time)
- Preserve all WebRTC functionality (it's production!)

### Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Total LoC | ~50,000 | ~15,000 |
| Active runtimes | 3 | 1 (Rust+PyO3) |
| NodeExecutor traits | 2 | 1 |
| Files broken by Error enum | 62 | ~15 |
| WebRTC audio latency | 380ms | <10ms |
| Maintenance burden | High | Low |

---

## User Scenarios & Testing

### User Story 1 - WebRTC Real-Time Audio Performance (Priority: P1) 🎯 **PRODUCTION CRITICAL**

**User**: Developer running WebRTC server for real-time audio transcription/response

**Current State**: 
- WebRTC server uses old v0.1.x audio nodes (AudioTransform)
- Audio preprocessing takes ~380ms per 10s chunk (librosa warm-up overhead)
- 32-core server struggles to keep up, causing choppy audio responses
- Real-time factor: 3.8 (too slow for smooth real-time)

**Desired State**:
- WebRTC server uses v0.2.0 Rust-accelerated nodes (AudioResampleNode, VADNode, FormatConverterNode)
- Audio preprocessing takes ~5ms per 10s chunk (72x faster)
- Server has headroom for additional processing
- Real-time factor: 0.0005 (can process 200x faster than real-time)
- Smooth, responsive audio without choppiness

**Why P1**: This is a **production system** currently suffering performance issues. The 72x speedup from v0.2.0 exists but WebRTC isn't using it yet. Immediate measurable impact on production quality.

**Independent Test**: 
1. Start WebRTC server with old nodes (v0.1.x API) - measure latency
2. Migrate to v0.2.0 Rust nodes - measure latency
3. Connect browser client, speak, measure end-to-end response time
4. Verify smooth audio without choppiness

**Acceptance Scenarios**:

1. **Given** WebRTC server uses v0.2.0 Rust nodes, **When** audio chunk is received, **Then** preprocessing completes in under 10ms (vs 380ms before)
2. **Given** user speaks into browser microphone, **When** audio is processed and response generated, **Then** response is smooth without choppy artifacts
3. **Given** WebRTC pipeline with metrics enabled, **When** user checks processing times, **Then** metrics show sub-10ms audio preprocessing
4. **Given** existing WebRTC client HTML, **When** server is migrated to v0.2.0, **Then** client continues to work without modifications
5. **Given** 10 concurrent WebRTC connections, **When** all are actively streaming, **Then** server maintains sub-10ms latency for all streams

**Verification**:
```python
# Before migration (v0.1.x)
pipeline.add_node(AudioTransform(output_sample_rate=16000))
# Result: ~380ms processing time, choppy audio

# After migration (v0.2.0)
pipeline.add_node(AudioResampleNode(
    target_sample_rate=16000,
    runtime_hint="rust"  # 124x faster!
))
# Result: ~3ms processing time, smooth audio
```

---

### User Story 2 - Single NodeExecutor Trait Architecture (Priority: P2)

**User**: Core maintainer making changes to Error enum

**Current State**:
- Two competing NodeExecutor traits exist
- `cpython_node.rs` acts as adapter between them
- Changing Error enum requires updating 62 files
- High friction for improvements
- Unclear which trait to use for new nodes

**Desired State**:
- Single canonical trait: `executor::node_executor::NodeExecutor`
- No adapter code needed
- Error enum changes affect ~15 files
- Clear guidance: all nodes use same trait
- New contributors know exactly where to look

**Why P2**: Reduces maintenance burden, enables faster iteration. Not blocking production but significantly improves developer experience.

**Independent Test**:
1. Make small change to Error enum (add new variant)
2. Count files that need updates
3. Verify all code compiles
4. Confirm no adapter code exists

**Acceptance Scenarios**:

1. **Given** developer searches for "NodeExecutor", **When** searching codebase, **Then** only one trait definition exists
2. **Given** cpython_executor needs to implement NodeExecutor, **When** developer adds implementation, **Then** uses `executor::node_executor::NodeExecutor` directly (no adapter)
3. **Given** Error enum gains new variant, **When** code is updated, **Then** fewer than 20 files require changes (vs 62 before)
4. **Given** all Rust code, **When** `cargo build --release` runs, **Then** build succeeds without errors
5. **Given** Python tests, **When** `pytest test_rust_compatibility.py` runs, **Then** all 15 tests pass

---

### User Story 3 - Clear Code Organization (Priority: P3)

**User**: New contributor trying to understand codebase

**Current State**:
- Mix of active and experimental code
- WASM demo files scattered throughout
- Historical docs mixed with current docs
- Unclear what's production vs prototype
- 50,000 LoC overwhelming

**Desired State**:
- Clear `archive/` directory with explanatory READMEs
- Active code is ~15,000 LoC (70% reduction)
- Documentation clearly marks what's current
- New contributor can find active code quickly
- Archived code still accessible for reference

**Why P3**: Important for onboarding and long-term maintainability, but not blocking current work.

**Independent Test**:
1. Clone repository
2. Read README.md
3. Navigate to active code (should be obvious)
4. Check archive/ (should have clear explanations)

**Acceptance Scenarios**:

1. **Given** new contributor clones repo, **When** they read main README.md, **Then** they see clear indication of active runtime (Rust+PyO3) and archived components
2. **Given** `archive/` directory exists, **When** contributor opens it, **Then** each subdirectory has README explaining what was archived and why
3. **Given** need to restore archived code, **When** following restoration instructions, **Then** process is documented step-by-step
4. **Given** running `cargo build`, **When** build executes, **Then** no references to archived WASM code
5. **Given** main documentation, **When** searching for "WASM", **Then** only archive references appear (not active features)

---

### User Story 4 - Preserved Functionality (Priority: P1) 🛡️ **NON-NEGOTIABLE**

**User**: Existing users of v0.2.0 SDK

**Current State**:
- v0.2.0 has 15/15 passing Python compatibility tests
- Audio nodes work with `runtime_hint` parameter
- Benchmarks show 72x speedup
- Examples demonstrate v0.2.0 API

**Desired State**:
- ALL v0.2.0 functionality preserved
- ALL 15 compatibility tests still pass
- ALL benchmarks still show same performance
- Zero regressions introduced by archival

**Why P1**: **NON-NEGOTIABLE**. We cannot break working v0.2.0 functionality. This is a cleanup operation, not a rewrite.

**Independent Test**:
1. Run full test suite before archival (baseline)
2. Perform archival operations
3. Run full test suite after archival (must match baseline)
4. Run benchmarks (must show same 72x speedup)

**Acceptance Scenarios**:

1. **Given** Python compatibility tests, **When** `pytest test_rust_compatibility.py` runs, **Then** all 15 tests pass (same as v0.2.0)
2. **Given** audio preprocessing benchmark, **When** executed, **Then** shows 72x speedup (same as v0.2.0)
3. **Given** existing v0.2.0 user code, **When** run against post-archival SDK, **Then** code executes without modifications
4. **Given** Phase 7 metrics, **When** pipeline runs with metrics enabled, **Then** 29μs overhead maintained
5. **Given** runtime selection, **When** Rust unavailable, **Then** automatic Python fallback still works

---

## Technical Approach

### Architecture: Before vs After

**Before (v0.2.0 state)**:
```
remotemedia-sdk/
├── browser-demo/              # 5,000 LoC - unused
├── wasi-sdk-*/                # 10,000 LoC - WASM toolchains
├── runtime/
│   ├── src/
│   │   ├── executor/
│   │   │   └── node_executor.rs  # NEW trait (good)
│   │   ├── nodes/
│   │   │   └── mod.rs            # OLD trait (bad)
│   │   ├── python/
│   │   │   ├── cpython_executor.rs  # Uses OLD trait
│   │   │   └── cpython_node.rs      # Adapter between traits
│   │   └── audio/              # ✅ Working v0.2.0 nodes
│   └── tests/                  # Mix of passing/failing
├── updated_spec/               # 3,000 LoC - historical
├── RUSTPYTHON_*.md             # Historical docs
├── webrtc-example/             # ✅ Production but using old API
└── examples/                   # ✅ v0.2.0 examples
```

**After (v0.2.1 target)**:
```
remotemedia-sdk/
├── archive/                    # NEW: Archived code
│   ├── README.md               # Index of archived components
│   ├── wasm-browser-runtime/   # WASM demo + toolchains
│   ├── old-node-executor/      # Old trait + adapters
│   └── old-specifications/     # Historical docs
├── runtime/
│   ├── src/
│   │   ├── executor/
│   │   │   └── node_executor.rs  # ONLY trait
│   │   ├── nodes/
│   │   │   ├── audio/            # ✅ v0.2.0 nodes
│   │   │   └── registry.rs
│   │   └── python/
│   │       ├── cpython_executor.rs  # Uses NEW trait
│   │       └── numpy_marshal.rs
│   └── tests/                  # Only passing tests
├── webrtc-example/             # ✅ Updated to v0.2.0 API
├── examples/                   # ✅ v0.2.0 examples
└── docs/
    ├── ARCHIVAL_GUIDE.md       # NEW: What was archived
    └── (v0.2.0 docs)
```

### Key Design Decisions

**1. Archive vs Delete**
- **Decision**: Archive (move to `archive/`) rather than delete
- **Rationale**: Git history preserves but archive/ makes clear what's inactive
- **Alternatives**: Could delete entirely, but users may want to reference

**2. WebRTC Migration Strategy**
- **Decision**: Update to v0.2.0 API in-place (same directory)
- **Rationale**: It's production code that needs to stay active
- **Alternatives**: Could rewrite from scratch, but migration preserves working code

**3. NodeExecutor Consolidation Order**
- **Decision**: Archive old trait → Update cpython_executor → Verify builds
- **Rationale**: Incremental approach reduces risk
- **Alternatives**: Big-bang rewrite (too risky)

**4. Breaking Change Policy**
- **Decision**: Zero breaking changes for SDK users
- **Rationale**: This is internal cleanup, not API change
- **Verification**: All 15 Python tests must pass

---

## Dependencies

### On v0.2.0 (001-native-rust-acceleration)
- ✅ Audio nodes: resample (124x), VAD (1.02x), format (1.00x)
- ✅ Runtime selection with `runtime_hint` parameter
- ✅ Phase 7 metrics (29μs overhead)
- ✅ 15/15 Python compatibility tests passing

### External Dependencies
- None (pure internal refactoring)

---

## Out of Scope

**Not included in this feature**:
- ❌ New functionality (no new nodes, no new features)
- ❌ Performance improvements beyond v0.2.0
- ❌ API changes (SDK API remains identical)
- ❌ SIMD optimizations (future work)
- ❌ Documentation improvements beyond archival guides
- ❌ Examples beyond WebRTC migration

**Why out of scope**: This is a cleanup operation focused on reducing complexity. New features come after we have a clean foundation.

---

## Success Criteria

### Must Have (Non-Negotiable)
- ✅ All 15 Python compatibility tests pass
- ✅ WebRTC server works with sub-10ms audio latency
- ✅ Only one NodeExecutor trait exists
- ✅ Total LoC reduced by >60%
- ✅ Zero breaking changes for SDK users

### Should Have
- ✅ Archive/ directory with clear READMEs
- ✅ ARCHIVAL_GUIDE.md documentation
- ✅ Updated CHANGELOG.md with archival notes
- ✅ WebRTC server README updated with performance comparison

### Nice to Have
- ✅ Benchmark showing Error enum changes affect fewer files
- ✅ Before/after architecture diagrams
- ✅ Migration guide for anyone using old internal APIs

---

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| WebRTC migration breaks production | MEDIUM | HIGH | Thorough testing, preserve old code in git |
| NodeExecutor consolidation breaks builds | LOW | HIGH | Incremental updates, test at each step |
| Users confused about archived code | LOW | LOW | Clear documentation, ARCHIVAL_GUIDE.md |
| Need to restore archived code later | MEDIUM | MEDIUM | Clear restoration instructions in READMEs |
| Tests fail after consolidation | LOW | HIGH | Verify tests pass before each git commit |

---

## Timeline Estimate

**Total**: 5 weeks

- Week 1: Archive WASM/browser demo
- Week 2: Consolidate NodeExecutor trait  
- Week 3: Migrate WebRTC server to v0.2.0 (PRODUCTION CRITICAL)
- Week 4: Update documentation & examples
- Week 5: Validation & release v0.2.1

---

## Version

**Spec Version**: 1.0  
**Last Updated**: 2025-10-27
