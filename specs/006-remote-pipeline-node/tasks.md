# Tasks: Transport Plugin Refactoring

**Feature Branch**: `006-remote-pipeline-node`
**Input**: Design documents from `/specs/006-remote-pipeline-node/`
**Prerequisites**: plan.md, spec.md, research-transport-plugins.md, data-model.md, contracts/, quickstart.md

**Organization**: This is an architectural refactoring to unify transport implementations. Tasks are organized by implementation phases to enable incremental delivery and testing.

## Format: `[ID] [P?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- Include exact file paths in descriptions

---

## Phase 1: Foundation - TransportPlugin Trait & Registry (8 tasks)

**Purpose**: Define core trait interfaces and registry infrastructure that all subsequent phases depend on.

**⚠️ CRITICAL**: No transport refactoring can begin until this phase is complete.

- [X] T001 Define TransportPlugin trait in `runtime-core/src/transport/mod.rs` with methods: name(), create_client(), create_server(), validate_config()
- [X] T002 Define ClientConfig struct in `runtime-core/src/transport/mod.rs` with fields: address, auth_token, timeout_ms, extra_config (JSON)
- [X] T003 Define ServerConfig struct in `runtime-core/src/transport/mod.rs` with fields: address, tls_config
- [X] T004 Create TransportPluginRegistry struct in `runtime-core/src/transport/plugin_registry.rs` (new file) with HashMap storage and RwLock wrapper
- [X] T005 [P] Implement plugin_registry::register() method with duplicate name detection
- [X] T006 [P] Implement plugin_registry::get() method returning Arc<dyn TransportPlugin>
- [X] T007 [P] Implement plugin_registry::list() method returning Vec<String>
- [X] T008 Implement global_registry() accessor function with lazy initialization via OnceLock

**Checkpoint**: TransportPlugin trait and registry compiled successfully. Registry can store and retrieve plugins by name.

---

## Phase 2: gRPC Transport Plugin (6 tasks)

**Purpose**: Refactor gRPC transport to use plugin system, moving client code from runtime-core to remotemedia-grpc crate.

- [X] T009 Move `runtime-core/src/transport/client/grpc.rs` to `transports/remotemedia-grpc/src/client.rs` (copy content, don't delete original yet)
- [X] T010 Create GrpcTransportPlugin struct in `transports/remotemedia-grpc/src/plugin.rs` implementing TransportPlugin trait
- [X] T011 [P] Implement GrpcTransportPlugin::create_client() calling GrpcPipelineClient::new() with endpoint and auth_token from ClientConfig
- [X] T012 [P] Implement GrpcTransportPlugin::create_server() calling GrpcServer::new() with bind_addr from ServerConfig
- [X] T013 Export GrpcTransportPlugin from `transports/remotemedia-grpc/src/lib.rs` with pub use statement
- [X] T014 Update register_default_plugins() documentation in `runtime-core/src/transport/plugin_registry.rs` to explain application-level registration pattern (avoiding circular dependencies)

**Checkpoint**: GrpcTransportPlugin compiles and can be registered. Old grpc.rs client still exists (backward compatibility maintained).

---

## Phase 3: WebRTC Transport Plugin (6 tasks)

**Purpose**: Refactor WebRTC transport to use plugin system, moving client code from runtime-core to remotemedia-webrtc crate.

- [X] T015 Move `runtime-core/src/transport/client/webrtc.rs` to `transports/remotemedia-webrtc/src/client.rs` (copy content, don't delete original yet)
- [X] T016 Create WebRtcTransportPlugin struct in `transports/remotemedia-webrtc/src/plugin.rs` implementing TransportPlugin trait
- [X] T017 [P] Implement WebRtcTransportPlugin::create_client() extracting ice_servers from extra_config JSON and passing to WebRtcPipelineClient
- [X] T018 [P] Implement WebRtcTransportPlugin::create_server() extracting WebRTC config from extra_config and passing to WebRtcTransport
- [X] T019 [P] Implement WebRtcTransportPlugin::validate_config() to check ice_servers structure in extra_config
- [X] T020 Export WebRtcTransportPlugin from `transports/remotemedia-webrtc/src/lib.rs` (already exported, verified no circular dependencies)

**Checkpoint**: WebRtcTransportPlugin compiles and can be registered. Old webrtc.rs client still exists (backward compatibility maintained).

---

## Phase 4: HTTP Transport Plugin (4 tasks)

**Purpose**: Create HTTP/REST transport with SSE streaming as separate crate following gRPC/WebRTC pattern.

- [X] T021 Create remotemedia-http crate with complete structure (Cargo.toml, lib.rs, error.rs)
- [X] T022 [P] Move HTTP client to remotemedia-http and add SSE streaming support (client.rs with HttpPipelineClient and HttpStreamSession)
- [X] T023 [P] Implement HTTP server with SSE endpoints (server.rs with POST /execute, POST /stream, GET /stream/:id/output SSE, DELETE /stream/:id)
- [X] T024 Create HttpTransportPlugin implementing TransportPlugin trait (plugin.rs with create_client and create_server support)

**Checkpoint**: All three transports (gRPC, WebRTC, HTTP) are now available as plugins with full streaming support.

---

## Phase 5: RemotePipelineNode Integration (6 tasks)

**Purpose**: Update RemotePipelineNode to use plugin registry instead of hardcoded client factory.

- [X] T025 Update RemotePipelineNode::get_client() in `runtime-core/src/nodes/remote_pipeline.rs` to lookup plugin from registry using self.config.transport string
- [X] T026 Replace create_transport_client() factory calls with plugin.create_client(ClientConfig) in RemotePipelineNode::get_client()
- [X] T027 Add ClientConfig::from_manifest_params() helper in `runtime-core/src/transport/mod.rs` to extract endpoint, auth_token, extra_config from manifest params
- [X] T028 Update error handling in RemotePipelineNode to provide helpful message when transport not found, listing available transports via registry.list()
- [X] T029 Add transport config validation call (plugin.validate_config()) in RemotePipelineNode before creating client
- [X] T030 Mark create_transport_client() factory function as #[deprecated(since = "0.5.0")] with migration note and examples

**Checkpoint**: RemotePipelineNode uses plugin registry. Existing manifests with "transport": "grpc" work without changes.

---

## Phase 6: Testing & Polish (8 tasks)

**Purpose**: Comprehensive testing of plugin system and cleanup of deprecated code paths.

### Unit Tests

- [X] T031 [P] Add test for TransportPluginRegistry::register() with duplicate plugin names in `runtime-core/tests/test_plugin_registry.rs` (14 tests total)
- [X] T032 [P] Add test for plugin lookup (found vs not found) in `runtime-core/tests/test_plugin_registry.rs` (included in 14 tests)
- [X] T033 [P] Add test for concurrent registry access from multiple threads in `runtime-core/tests/test_plugin_registry.rs` (4 concurrency tests)

### Integration Tests

- [X] T034 Create MockTransportPlugin in `runtime-core/tests/fixtures/mock_transport_plugin.rs` with echo behavior for testing
- [X] T035 Add integration test registering custom MockTransportPlugin and using it via RemotePipelineNode in `runtime-core/tests/test_custom_transport.rs` (4 tests)
- [X] T036 Add integration test for all three transports (gRPC, WebRTC, HTTP) via plugin registry in `runtime-core/tests/test_all_transports.rs` (6 tests)

### Cleanup

- [X] T037 Delete old hardcoded client files: `runtime-core/src/transport/client/grpc.rs`, `webrtc.rs`, and `http.rs` (all moved to transport crates)
- [X] T038 Update documentation: Added plugin system to README.md and created comprehensive migration guide in docs/MIGRATION_TO_PLUGINS.md

**Checkpoint**: All tests pass (24 plugin tests). Plugin system is production-ready. Old code paths removed.

---

## Dependencies & Execution Order

### Phase Dependencies

```
Phase 1 (Foundation)
    ↓ BLOCKS ALL PHASES BELOW
    ├─→ Phase 2 (gRPC) ──┐
    ├─→ Phase 3 (WebRTC) ├─→ Phase 5 (RemotePipelineNode) → Phase 6 (Testing & Cleanup)
    └─→ Phase 4 (HTTP) ──┘
```

- **Phase 1 (Foundation)**: No dependencies - start here
- **Phase 2, 3, 4 (Transport Plugins)**: All depend on Phase 1 completion, but can proceed in parallel with each other
- **Phase 5 (RemotePipelineNode)**: Depends on at least one transport plugin (recommend completing Phase 2-4 first)
- **Phase 6 (Testing & Polish)**: Depends on all previous phases

### Within Each Phase

**Phase 1 (Foundation)**:
- T001-T003 can run in parallel (different files: mod.rs, client/mod.rs, server.rs)
- T004 depends on T001 (needs TransportPlugin trait defined)
- T005-T007 can run in parallel after T004 (different methods, same file)
- T008 depends on T004 (needs TransportPluginRegistry struct)

**Phase 2 (gRPC)**:
- T009 (file move) must complete first
- T010-T012 can run in parallel after T009 (T011 and T012 marked [P])
- T013-T014 sequential (exports then registration)

**Phase 3 (WebRTC)**:
- T015 (file move) must complete first
- T016-T019 can run in parallel after T015 (T017, T018, T019 marked [P])
- T020 last (exports and registration)

**Phase 4 (HTTP)**:
- T021 first (create plugin struct)
- T022-T023 can run in parallel after T021 (marked [P])
- T024 last (registration)

**Phase 5 (RemotePipelineNode)**:
- T025-T030 mostly sequential (all modify same file or tightly coupled logic)
- T027 can happen in parallel with T025-T026 (different file)

**Phase 6 (Testing & Polish)**:
- T031-T033 can run in parallel (marked [P], different test functions)
- T034 must complete before T035 (MockTransportPlugin needed for test)
- T036 can run in parallel with T035 (different test files)
- T037-T038 must be last (cleanup after all tests pass)

### Parallel Opportunities

Within each phase:
```bash
# Phase 1: Parallel trait and config definitions
Task T001: "Define TransportPlugin trait in runtime-core/src/transport/mod.rs"
Task T002: "Define ClientConfig struct in runtime-core/src/transport/client/mod.rs"
Task T003: "Define ServerConfig struct in runtime-core/src/transport/server.rs"

# Phase 1: Parallel registry methods
Task T005: "Implement plugin_registry::register() method"
Task T006: "Implement plugin_registry::get() method"
Task T007: "Implement plugin_registry::list() method"

# Phase 2: Parallel gRPC plugin implementation
Task T011: "Implement GrpcTransportPlugin::create_client()"
Task T012: "Implement GrpcTransportPlugin::create_server()"

# Phase 3: Parallel WebRTC plugin implementation
Task T017: "Implement WebRtcTransportPlugin::create_client()"
Task T018: "Implement WebRtcTransportPlugin::create_server()"
Task T019: "Implement WebRtcTransportPlugin::validate_config()"

# Phase 6: Parallel unit tests
Task T031: "Test duplicate plugin registration"
Task T032: "Test plugin lookup"
Task T033: "Test concurrent registry access"
```

---

## Implementation Strategy

### Recommended Approach: Sequential Phases

1. **Complete Phase 1 (Foundation)** → Verify traits and registry compile
2. **Complete Phase 2 (gRPC)** → Verify gRPC plugin works
3. **Complete Phase 3 (WebRTC)** → Verify WebRTC plugin works
4. **Complete Phase 4 (HTTP)** → Verify all three plugins available
5. **Complete Phase 5 (RemotePipelineNode)** → Verify manifests work with plugin system
6. **Complete Phase 6 (Testing & Polish)** → Verify all tests pass, cleanup old code

### Validation Checkpoints

After each phase, verify:
- ✅ Code compiles without errors
- ✅ Existing tests still pass (backward compatibility maintained)
- ✅ New functionality demonstrated (manual test or example)
- ✅ No breaking changes introduced yet (old code paths still work)

### Backward Compatibility Maintained Until Phase 6

- Phases 1-5: Old client code (`grpc.rs`, `webrtc.rs`) remains in runtime-core
- Phase 5: create_transport_client() marked deprecated but still functional
- Phase 6: After all tests pass, old code deleted (breaking change for major version)

---

## Success Criteria (from plan.md)

- [ ] All existing tests pass without modification
- [ ] Manifests with `"transport": "grpc"` work exactly as before
- [ ] Can register custom transport plugin without modifying runtime-core
- [ ] Binary size without feature gates is smaller (no tonic/webrtc compiled)
- [ ] Plugin lookup adds <1μs overhead vs hardcoded clients
- [ ] Integration tests demonstrate custom transport plugin
- [ ] Documentation shows how to implement custom transport
- [ ] gRPC and WebRTC client code lives in their respective crates

---

## Notes

- [P] tasks = different files/methods, no dependencies
- This is infrastructure refactoring (no user stories with US1, US2 labels)
- Organized by technical phases rather than user-facing features
- Each phase builds on previous phase (sequential phases, parallel tasks within phases)
- Backward compatibility maintained until Phase 6 cleanup
- Old factory function deprecated but not removed until tests pass
- Manifests require no changes (transport strings map directly to plugin names)
- Feature flags (grpc-client, webrtc-client) control which plugins are compiled
