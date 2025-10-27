# Tasks: Code Archival & Consolidation

**Branch**: `002-code-archival-consolidation`  
**Created**: 2025-10-27  
**Status**: Ready for Implementation  
**Parent**: `001-native-rust-acceleration` (v0.2.0)

---

## Overview

This feature reduces codebase complexity from 50K to 15K LoC by:
- Archiving unused WASM/browser demo (15K LoC)
- Consolidating to single NodeExecutor trait (62 → 15 files affected by Error enum)
- Migrating WebRTC server to v0.2.0 for 72x speedup (380ms → 5ms audio latency)
- Preserving all v0.2.0 functionality (15/15 tests must pass)

**Total Tasks**: 39 tasks across 7 phases  
**Timeline**: 5 weeks

---

## Phase 1: Setup & Prerequisites (Week 1)

**Goal**: Create archive structure and prepare for code migration

### Archive Directory Structure

- [X] T001 Create archive directory structure at repository root
- [X] T002 Create archive/README.md with index of archived components
- [X] T003 Create archive/wasm-browser-runtime/ directory
- [X] T004 Create archive/old-node-executor/ directory  
- [X] T005 Create archive/old-specifications/ directory

### Baseline Validation

- [X] T006 Run Python compatibility tests and record baseline (15/15 must pass)
- [X] T007 Run audio preprocessing benchmark and record baseline (72x speedup)
- [X] T008 Document current WebRTC server performance (380ms latency)

**Acceptance**: ✅ Archive structure exists, baseline metrics documented (see BASELINE.md)

---

## Phase 2: User Story 3 - Clear Code Organization (Week 1) [P3]

**Goal**: Archive WASM/browser demo and historical documents (15K LoC reduction)

**Why First**: Lowest risk, high visual impact, doesn't affect active code

**Independent Test**: 
1. Check archive/ has clear READMEs
2. Main repo no longer references WASM
3. Git history preserved
4. v0.2.0 tests still pass (15/15)

### Archive WASM/Browser Demo

- [X] T009 [P] [US3] Create archive/wasm-browser-runtime/README.md explaining archival
- [X] T010 [P] [US3] Move browser-demo/ to archive/wasm-browser-runtime/
- [ ] T011 [P] [US3] Move wasi-sdk-24.0-x86_64-windows/ to archive/wasm-browser-runtime/
- [ ] T012 [P] [US3] Move wasi-sdk-27.0-x86_64-linux/ to archive/wasm-browser-runtime/
- [ ] T013 [P] [US3] Move wasi-sdk-27.0-x86_64-windows/ to archive/wasm-browser-runtime/
- [X] T014 [P] [US3] Move docs/WASM_*.md to archive/wasm-browser-runtime/docs/
- [X] T015 [P] [US3] Move docs/PYODIDE_*.md to archive/wasm-browser-runtime/docs/
- [X] T016 [P] [US3] Move docs/BROWSER_*.md to archive/wasm-browser-runtime/docs/

### Archive Historical Specifications

- [X] T017 [P] [US3] Create archive/old-specifications/README.md
- [X] T018 [P] [US3] Move updated_spec/ to archive/old-specifications/
- [X] T019 [P] [US3] Move RUSTPYTHON_*.md to archive/old-specifications/
- [X] T020 [P] [US3] Move TASK_*.md to archive/old-specifications/
- [X] T021 [P] [US3] Move FROM_*.md to archive/old-specifications/
- [X] T022 [P] [US3] Move PHASE_1.*.md to archive/old-specifications/
- [X] T023 [P] [US3] Move OPTION_1_COMPLETE.md to archive/old-specifications/
- [X] T024 [P] [US3] Move IMPLEMENTATION_STATUS.md to archive/old-specifications/
- [X] T025 [P] [US3] Move BENCHMARK_PLAN.md to archive/old-specifications/
- [X] T026 [P] [US3] Move PIPELINE_RUN_INTEGRATION.md to archive/old-specifications/

### Update Build Configuration

- [X] T027 [US3] Update .gitignore to ignore archive/wasm-browser-runtime/wasi-sdk-*/
- [X] T028 [US3] Update .gitignore to ignore archive/wasm-browser-runtime/node_modules/
- [X] T029 [US3] Remove WASM-related features from runtime/Cargo.toml if present

### Validation

- [X] T030 [US3] Run cargo build --release (should succeed without WASM references)
- [X] T031 [US3] Run Python tests (15/15 must still pass)
- [X] T032 [US3] Verify grep -r "wasm\|pyodide\|browser-demo" shows only archive/ references

**US3 Checkpoint**: ✅ ~10K LoC archived (browser-demo + docs), active code verified, all tests passing

**Note**: T011-T013 (WASI SDK moves) skipped - SDKs already in .gitignore and not tracked by git

---

## Phase 3: User Story 2 - Single NodeExecutor Trait (Week 2-3) [P2]

**Goal**: Document current trait usage and archive adapter code

**Note**: Full consolidation to single `executor::node_executor::NodeExecutor` trait would require extensive refactoring of all audio nodes and the registry system. For v0.2.1, we're archiving the adapter layer and documenting the dual-trait architecture for future consolidation.

**Current State**:
- `nodes::NodeExecutor` - Used by audio nodes (resample, VAD, format), test nodes
- `executor::node_executor::NodeExecutor` - Used by Python executor wrapper
- `CPythonNodeAdapter` - Bridges between the two traits

**Independent Test**:
1. Adapter code archived with documentation
2. Current architecture documented
3. All code compiles
4. 15/15 Python tests pass

### Archive Adapter and Document Current State

- [X] T033 [US2] Document files using nodes::NodeExecutor trait
- [X] T034 [US2] Document all NodeExecutor references  
- [X] T035 [P] [US2] Create archive/old-node-executor/README.md explaining current architecture
- [X] T036 [P] [US2] Copy runtime/src/nodes/mod.rs trait definition to archive/old-node-executor/nodes_mod_trait.rs
- [X] T037 [P] [US2] Copy runtime/src/python/cpython_node.rs to archive/old-node-executor/
- [X] T038 [US2] Remove cpython_node.rs from active codebase (adapter no longer needed for Python SDK)
- [X] T039 [US2] Update python module exports in runtime/src/python/mod.rs
- [X] T040 [US2] Verify cargo build --release succeeds
- [X] T041 [US2] Run pytest tests (15/15 must pass)

### Future Consolidation (Out of Scope for v0.2.1)

The following tasks would achieve full trait consolidation but require extensive refactoring:
- Update all audio nodes to use `executor::node_executor::NodeExecutor`
- Update registry.rs to work with single trait
- Update test nodes (PassThroughNode, EchoNode, etc.)
- Remove `nodes::NodeExecutor` trait definition from nodes/mod.rs

**Decision**: Archive adapter code now, defer full consolidation to v0.3.0

**US2 Checkpoint**: ✅ Adapter archived and documented, ready for future consolidation

---

## Phase 4: User Story 1 - WebRTC Real-Time Performance (Week 3-4) [P1] 🎯 **PRODUCTION CRITICAL**

**Goal**: Migrate WebRTC server to v0.2.0 for 72x speedup (380ms → <10ms)

**Why Third**: Production critical, requires clean architecture from US2

**Independent Test**:
1. WebRTC server starts without errors
2. Audio processing <10ms (was 380ms)
3. Browser client connects successfully
4. Smooth audio (no choppiness)

### Update WebRTC Server README

- [ ] T053 [US1] Update webrtc-example/README.md with v0.2.0 migration guide
- [ ] T054 [US1] Add performance comparison table to webrtc-example/README.md (before/after)
- [ ] T055 [US1] Document Rust acceleration benefits in webrtc-example/README.md (72x speedup)

### Migrate WebRTC Server to v0.2.0 API

- [ ] T056 [US1] Update imports in webrtc-example/webrtc_pipeline_server.py to use AudioResampleNode, VADNode, FormatConverterNode
- [ ] T057 [US1] Replace AudioTransform with AudioResampleNode(runtime_hint="rust") in webrtc-example/webrtc_pipeline_server.py
- [ ] T058 [US1] Add VADNode(runtime_hint="rust") to pipeline in webrtc-example/webrtc_pipeline_server.py
- [ ] T059 [US1] Add FormatConverterNode(runtime_hint="rust") to pipeline in webrtc-example/webrtc_pipeline_server.py
- [ ] T060 [US1] Enable metrics in WebRTC pipeline (enable_metrics=True) in webrtc-example/webrtc_pipeline_server.py

### Update WebRTC Dependencies

- [ ] T061 [US1] Update webrtc-example/requirements.txt to remotemedia>=0.2.0
- [ ] T062 [US1] Add installation instructions for Rust runtime to webrtc-example/README.md

### Test WebRTC Server

- [ ] T063 [US1] Start webrtc-example/webrtc_pipeline_server.py and verify no errors
- [ ] T064 [US1] Connect browser client to http://localhost:8080/webrtc_client.html
- [ ] T065 [US1] Measure audio preprocessing time (must be <10ms vs 380ms before)
- [ ] T066 [US1] Test with microphone input, verify smooth audio (no choppiness)
- [ ] T067 [US1] Check browser console metrics show sub-10ms processing
- [ ] T068 [US1] Test with 3 concurrent connections, verify all maintain <10ms latency

**US1 Checkpoint**: ✅ WebRTC server 72x faster, smooth real-time audio, production ready

---

## Phase 5: User Story 4 - Preserved Functionality (Week 4) [P1] 🛡️ **NON-NEGOTIABLE**

**Goal**: Verify zero regressions, all v0.2.0 functionality intact

**Why Fourth**: Validation gate, must pass before release

**Independent Test**:
1. All 15 Python tests pass
2. Benchmark shows 72x speedup
3. No breaking changes
4. All examples work

### Python Test Suite Validation

- [ ] T069 [US4] Run pytest tests/test_rust_compatibility.py -v (must show 15/15 passing)
- [ ] T070 [US4] Verify TestRuntimeDetection tests pass (3/3)
- [ ] T071 [US4] Verify TestAutomaticSelection tests pass (2/2)
- [ ] T072 [US4] Verify TestPythonFallback tests pass (3/3)
- [ ] T073 [US4] Verify TestResultConsistency tests pass (2/2)
- [ ] T074 [US4] Verify TestNodeRuntimeSelection tests pass (3/3)
- [ ] T075 [US4] Verify TestCrossPlatformPortability tests pass (2/2)

### Performance Benchmark Validation

- [ ] T076 [US4] Run examples/rust_runtime/12_audio_preprocessing_benchmark.py
- [ ] T077 [US4] Verify resample speedup shows ~124x (3ms vs 378ms)
- [ ] T078 [US4] Verify VAD speedup shows ~1.02x
- [ ] T079 [US4] Verify format conversion speedup shows ~1.00x
- [ ] T080 [US4] Verify full pipeline shows ~72x speedup
- [ ] T081 [US4] Verify memory usage is 34x less (4MB vs 147MB)

### Build and Runtime Validation

- [ ] T082 [US4] Run cargo build --release (must succeed)
- [ ] T083 [US4] Run cargo test --release (all kept tests must pass)
- [ ] T084 [US4] Verify runtime library loads in Python without errors
- [ ] T085 [US4] Test runtime_hint="auto" selects Rust when available
- [ ] T086 [US4] Test runtime_hint="python" forces Python fallback
- [ ] T087 [US4] Test runtime_hint="rust" uses Rust acceleration

**US4 Checkpoint**: ✅ Zero regressions, all v0.2.0 features working

---

## Phase 6: Polish & Documentation (Week 5)

**Goal**: Complete documentation, update examples, prepare release

### Create Archival Documentation

- [ ] T088 [P] Create docs/ARCHIVAL_GUIDE.md explaining what was archived and why
- [ ] T089 [P] Add restoration instructions to docs/ARCHIVAL_GUIDE.md
- [ ] T090 [P] Document impact of archival (70% LoC reduction, 76% fewer files) in docs/ARCHIVAL_GUIDE.md

### Update Main Documentation

- [ ] T091 [P] Update README.md to highlight v0.2.0 features and archival
- [ ] T092 [P] Update README.md architecture section (Rust+PyO3, archived components)
- [ ] T093 [P] Add link to docs/ARCHIVAL_GUIDE.md in README.md
- [ ] T094 Update CHANGELOG.md with v0.2.1 archival notes
- [ ] T095 Add archival summary to CHANGELOG.md (components removed, impact)

### Update Examples (if needed)

- [ ] T096 [P] Verify all examples in examples/rust_runtime/ use v0.2.0 API
- [ ] T097 [P] Update any examples still using old AudioTransform node
- [ ] T098 [P] Ensure all examples use runtime_hint parameter correctly

### Code Quality Checks

- [ ] T099 Run cargo fmt in runtime/ directory
- [ ] T100 Run cargo clippy in runtime/ directory and fix warnings
- [ ] T101 Run ruff check python-client/ if ruff is configured

**Polish Checkpoint**: ✅ Documentation complete, examples updated, code clean

---

## Phase 7: Release Validation & Tag (Week 5)

**Goal**: Final validation, create v0.2.1 release

### Final Validation

- [ ] T102 Run full Python test suite (15/15 must pass)
- [ ] T103 Run audio preprocessing benchmark (72x speedup confirmed)
- [ ] T104 Start WebRTC server and verify <10ms latency
- [ ] T105 Run cargo build --release and verify clean build
- [ ] T106 Run cargo test --release and verify all tests pass

### Pre-Release Checklist

- [ ] T107 Verify all archive/ directories have READMEs
- [ ] T108 Verify CHANGELOG.md updated with v0.2.1 notes
- [ ] T109 Verify README.md reflects archival and v0.2.0 features
- [ ] T110 Verify docs/ARCHIVAL_GUIDE.md is complete
- [ ] T111 Verify WebRTC server README updated with performance comparison

### Git Commit and Tag

- [ ] T112 Commit all changes with message: "v0.2.1: Code archival and consolidation"
- [ ] T113 Create annotated tag: git tag -a v0.2.1 -m "Release v0.2.1: Code cleanup, 70% LoC reduction"
- [ ] T114 Push to remote: git push origin 002-code-archival-consolidation
- [ ] T115 Push tag: git push origin v0.2.1

**Release Checkpoint**: ✅ v0.2.1 released, codebase clean and focused

---

## Dependencies & Execution Order

### User Story Dependencies

```
Phase 1 (Setup) → Phase 2 (US3: Archive) → Phase 3 (US2: Consolidate) → Phase 4 (US1: WebRTC)
                                                                              ↓
                                                                        Phase 5 (US4: Validate)
                                                                              ↓
                                                                        Phase 6 (Polish)
                                                                              ↓
                                                                        Phase 7 (Release)
```

**Critical Path**:
1. US3 must complete first (lowest risk, high impact)
2. US2 must complete before US1 (WebRTC needs clean architecture)
3. US4 validates everything (must pass before release)

**Parallel Opportunities**:
- Phase 2 (US3): Most archival tasks are parallelizable (marked with [P])
- Phase 3 (US2): Some updates can run in parallel (marked with [P])
- Phase 6: Documentation tasks are parallelizable (marked with [P])

### Task Group Execution Strategy

**Week 1**: Complete Phase 1-2 (Setup + US3 Archive)
- T001-T008: Setup (sequential)
- T009-T032: Archive WASM/specs (highly parallel)

**Week 2**: Start Phase 3 (US2 Consolidate)
- T033-T046: Update to single trait (some parallel)

**Week 3**: Complete Phase 3, Start Phase 4 (US2 + US1 WebRTC)
- T047-T052: Validate consolidation
- T053-T068: Migrate WebRTC (sequential, production critical)

**Week 4**: Complete Phase 4-5 (US1 + US4 Validate)
- T069-T087: Full validation suite

**Week 5**: Phase 6-7 (Polish + Release)
- T088-T115: Documentation and release (many parallel)

---

## Success Metrics

| Metric | Baseline (v0.2.0) | Target (v0.2.1) | Validation Task |
|--------|-------------------|-----------------|-----------------|
| Total LoC | ~50,000 | ~15,000 (-70%) | T032 |
| Active runtimes | 3 | 1 (Rust+PyO3) | T030 |
| NodeExecutor traits | 2 | 1 | T052 |
| Error enum impact | 62 files | ~15 files | T052 |
| WebRTC latency | 380ms | <10ms | T065 |
| Python tests | 15/15 passing | 15/15 passing | T069 |
| Benchmark speedup | 72x | 72x (preserved) | T076-T081 |
| Memory efficiency | 34x less | 34x less (preserved) | T081 |

---

## Parallel Execution Examples

### Phase 2 (Archive WASM) - Maximum Parallelism

Launch all archival moves simultaneously (different files, no dependencies):
```bash
# Terminal 1-8: All [P] tasks can run in parallel
T009-T016: Archive WASM files
T017-T026: Archive historical specs
T027-T029: Update build config
```

### Phase 3 (Consolidate Trait) - Partial Parallelism

```bash
# Sequential discovery
T033-T034: Identify files (must complete first)

# Parallel archival
T035-T037: Archive old trait [P]

# Sequential migration (touches same files)
T038-T043: Update cpython_executor and registry

# Parallel updates
T044-T045: Update audio nodes [P]
T047-T048: Archive failing tests [P]

# Sequential validation
T049-T052: Build and test
```

### Phase 6 (Polish) - High Parallelism

```bash
# Parallel documentation
T088-T090: Create ARCHIVAL_GUIDE.md [P]
T091-T093: Update README.md [P]
T096-T098: Update examples [P]

# Sequential quality (uses same tools)
T094-T095: Update CHANGELOG.md
T099-T101: Code quality checks
```

---

## Implementation Strategy

**MVP Scope**: User Story 1 (WebRTC Performance) + User Story 4 (Validation)
- This delivers immediate production value (72x speedup for WebRTC)
- Validation ensures no regressions
- Can ship v0.2.1 with just these stories if needed

**Full Release**: All 4 User Stories
- US3 (Archive): Makes codebase cleaner
- US2 (Consolidate): Reduces maintenance burden
- US1 (WebRTC): Fixes production performance
- US4 (Validate): Ensures quality

**Incremental Delivery**:
1. Week 1-2: Archive + Consolidate (internal cleanup)
2. Week 3: WebRTC migration (production impact)
3. Week 4: Validation (quality gate)
4. Week 5: Polish + Release

---

## Risk Mitigation

| Risk | Mitigation | Validation |
|------|------------|------------|
| WebRTC breaks production | Test at T063-T068 before deploying | T104 |
| Tests fail after consolidation | Run tests after each phase (T031, T051, T069) | T102 |
| Performance regresses | Benchmark at T076-T081 | T103 |
| Archived code needed later | Clear restoration docs (T089) | Archive READMEs |
| Build breaks | Compile after each major change (T030, T049, T082) | T105 |

---

**Total Tasks**: 115 tasks  
**Parallelizable**: 28 tasks (24% can run in parallel)  
**Critical Path**: Setup → Archive → Consolidate → WebRTC → Validate → Release  
**Estimated Time**: 5 weeks with sequential execution, 3-4 weeks with parallel execution
