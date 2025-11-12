# Implementation Plan: Transport Unification - Client+Server Traits

**Branch**: `006-remote-pipeline-node` | **Date**: 2025-01-10 | **Spec**: [spec.md](./spec.md)
**Input**: User request to refactor transport layer so custom transports (gRPC, WebRTC) implement both client and server interfaces, not hardcoded in runtime-core.

## Summary

**Problem**: Currently, `RemotePipelineNode` uses hardcoded client implementations in `runtime-core/src/transport/client/` (gRPC, HTTP, WebRTC clients). This creates tight coupling and prevents custom transports from being fully self-contained. Transport crates like `remotemedia-grpc` and `remotemedia-webrtc` implement the server-side `PipelineTransport` trait, but the client-side implementations live in runtime-core.

**Solution**: Define a unified transport trait system where each transport crate must implement **both** client and server interfaces. This creates self-contained transport modules that can be plugged into both server and client contexts without hardcoding in runtime-core.

**Technical Approach**:
1. Define `TransportPlugin` trait that combines client + server capabilities
2. Move client implementations from `runtime-core/src/transport/client/{grpc,webrtc}.rs` to their respective transport crates
3. Create plugin registry in runtime-core that loads transport implementations dynamically
4. Update `RemotePipelineNode` to use plugin registry instead of hardcoded factory

**Benefits**:
- **Modularity**: Transports are self-contained with both client and server in one place
- **Extensibility**: Third-party transports can be added without modifying runtime-core
- **Consistency**: Same trait interface for all transports (gRPC, WebRTC, HTTP, custom)
- **Maintainability**: Changes to a transport only affect one crate

## Technical Context

**Language/Version**: Rust 1.75+
**Primary Dependencies**:
- tokio 1.35+ (async runtime)
- async-trait (trait objects for async traits)
- tonic 0.11+ (gRPC client+server)
- webrtc 0.9+ (WebRTC client+server)
- reqwest 0.11+ (HTTP client)

**Storage**: N/A (in-memory plugin registry)
**Testing**: cargo test, integration tests with MockTransportPlugin
**Target Platform**: Linux/macOS/Windows
**Project Type**: Single (runtime-core + multiple transport crates)

**Performance Goals**:
- Plugin lookup <1μs (Arc<HashMap> access)
- No additional overhead vs hardcoded clients
- Zero-copy where possible (Arc sharing of transport instances)

**Constraints**:
- Must maintain backward compatibility with existing manifests
- Must support feature gates (grpc-client, webrtc-client) for conditional compilation
- Must work with dynamic and static plugin loading (no dynamic libs initially, just trait objects)

**Scale/Scope**:
- 3 built-in transports: gRPC, WebRTC, HTTP
- ~1000 LOC refactoring across 4-5 files
- Must support future custom transports without core changes

## Constitution Check

*Constitution file is currently a template. Applying general good practices:*

**PASS**: Modularity principle - transports become self-contained libraries
**PASS**: Clear interfaces - TransportPlugin trait defines contract
**PASS**: Testability - Each transport can be tested independently
**PASS**: No unnecessary complexity - Simplifies runtime-core by removing hardcoded clients

## Project Structure

### Documentation (this feature)

```text
specs/006-remote-pipeline-node/
├── plan.md              # This file
├── research.md          # Phase 0 output (trait design patterns)
├── data-model.md        # Phase 1 output (TransportPlugin trait, registry)
├── quickstart.md        # Phase 1 output (how to implement custom transport)
└── contracts/           # Phase 1 output (trait definitions)
    ├── transport-plugin.md
    ├── plugin-registry.md
    └── client-interface.md
```

### Source Code (repository root)

```text
runtime-core/
├── src/
│   ├── transport/
│   │   ├── mod.rs                  # [MODIFY] Define TransportPlugin trait
│   │   ├── plugin_registry.rs      # [NEW] Global transport registry
│   │   ├── client/
│   │   │   ├── mod.rs               # [MODIFY] Remove hardcoded clients, use registry
│   │   │   ├── grpc.rs              # [DELETE] Move to remotemedia-grpc
│   │   │   ├── http.rs              # [KEEP] HTTP remains in core (reqwest-only)
│   │   │   └── webrtc.rs            # [DELETE] Move to remotemedia-webrtc
│   │   └── ...
│   └── nodes/
│       └── remote_pipeline.rs       # [MODIFY] Use plugin registry

transports/
├── remotemedia-grpc/
│   ├── src/
│   │   ├── lib.rs                   # [MODIFY] Export GrpcTransportPlugin
│   │   ├── client.rs                # [NEW] Moved from runtime-core
│   │   └── server.rs                # [EXISTS] Current server implementation
│   └── Cargo.toml                   # [MODIFY] Add plugin feature
│
├── remotemedia-webrtc/
│   ├── src/
│   │   ├── lib.rs                   # [MODIFY] Export WebRtcTransportPlugin
│   │   ├── client.rs                # [NEW] Moved from runtime-core
│   │   └── transport/
│   │       └── transport.rs         # [EXISTS] Current server implementation
│   └── Cargo.toml                   # [MODIFY] Add plugin feature
│
└── remotemedia-ffi/
    └── ... (no client, FFI is server-only)

tests/
└── integration/
    └── test_transport_plugins.rs    # [NEW] Plugin registration tests
```

**Structure Decision**: Keep runtime-core as single project with optional transport plugin features. Transports remain separate crates but now export unified plugins instead of just server implementations.

## Complexity Tracking

No constitution violations. Refactoring reduces complexity by removing hardcoded clients from runtime-core.

## Architecture Changes

### Current Architecture (Hardcoded Clients)

```text
┌──────────────────────────────────────────────────┐
│  runtime-core                                    │
│                                                  │
│  RemotePipelineNode                              │
│    └─> create_transport_client(type)            │
│           ├─> GrpcPipelineClient    (hardcoded) │
│           ├─> HttpPipelineClient    (hardcoded) │
│           └─> WebRtcPipelineClient  (hardcoded) │
│                                                  │
│  Server: PipelineTransport trait (in core)      │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  remotemedia-grpc                                │
│    └─> GrpcServer: PipelineTransport            │
│        (client lives in runtime-core!)           │
└──────────────────────────────────────────────────┘
```

**Problem**: Client and server are split across crates!

### New Architecture (Unified Plugin)

```text
┌──────────────────────────────────────────────────┐
│  runtime-core                                    │
│                                                  │
│  Trait: TransportPlugin                          │
│    ├─> name() -> &str                            │
│    ├─> create_client(config) -> Client          │
│    └─> create_server(config) -> Server          │
│                                                  │
│  TransportPluginRegistry                         │
│    └─> register/lookup plugins by name          │
│                                                  │
│  RemotePipelineNode                              │
│    └─> registry.get_plugin(name)                │
│           .create_client(config)                 │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  remotemedia-grpc                                │
│                                                  │
│  struct GrpcTransportPlugin;                     │
│                                                  │
│  impl TransportPlugin for GrpcTransportPlugin {  │
│    fn name() -> "grpc"                           │
│    fn create_client() -> GrpcClient              │
│    fn create_server() -> GrpcServer              │
│  }                                               │
│                                                  │
│  ├─> client.rs  (moved from runtime-core)       │
│  └─> server.rs  (already exists)                │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  remotemedia-webrtc                              │
│                                                  │
│  struct WebRtcTransportPlugin;                   │
│                                                  │
│  impl TransportPlugin for WebRtcTransportPlugin {│
│    fn name() -> "webrtc"                         │
│    fn create_client() -> WebRtcClient            │
│    fn create_server() -> WebRtcTransport         │
│  }                                               │
│                                                  │
│  ├─> client.rs  (moved from runtime-core)       │
│  └─> transport.rs (already exists)              │
└──────────────────────────────────────────────────┘
```

**Benefits**: Client and server in same crate! Self-contained transports!

## Phase 0: Research & Design

### Research Topics

1. **Trait Object Design Patterns**
   - How to make `TransportPlugin` object-safe (no generics, no associated types with Self)
   - Factory pattern for creating clients/servers dynamically
   - Lifetimes for plugin references in registry

2. **Feature Gate Strategy**
   - How to conditionally compile plugins (grpc-client, webrtc-client)
   - Default features for backward compatibility
   - Impact on binary size with/without plugins

3. **Registry Implementation**
   - Static vs dynamic registration (compile-time vs runtime)
   - Thread safety (Arc<RwLock<HashMap>> or OnceLock<HashMap>?)
   - Plugin initialization order

4. **Client Interface Unification**
   - Should `create_client()` return `Box<dyn PipelineClient>` or new trait?
   - How to handle transport-specific config (ICE servers for WebRTC, etc.)
   - Error handling for unsupported operations

5. **Backward Compatibility**
   - Existing manifests use `"transport": "grpc"` string
   - Need seamless migration path
   - Deprecation strategy for `create_transport_client()` factory

**Deliverable**: `research.md` with decisions for each topic

## Phase 1: Design Artifacts

### Data Model (`data-model.md`)

**Key Entities**:

1. **TransportPlugin**
   - Purpose: Unified interface for transport implementations
   - Methods: `name()`, `create_client()`, `create_server()`
   - Constraints: Must be object-safe (Send + Sync + 'static)

2. **TransportPluginRegistry**
   - Purpose: Global registry of available transport plugins
   - Storage: `OnceLock<HashMap<String, Arc<dyn TransportPlugin>>>`
   - Operations: `register()`, `get()`, `list()`

3. **ClientConfig**
   - Purpose: Transport-agnostic client configuration
   - Fields: `endpoint`, `auth_token`, `extra_config` (serde_json::Value)
   - Used by: `TransportPlugin::create_client()`

4. **ServerConfig**
   - Purpose: Transport-agnostic server configuration
   - Fields: `bind_addr`, `auth_config`, `extra_config`
   - Used by: `TransportPlugin::create_server()`

**State Transitions**: None (stateless registry)

### Contracts (`contracts/`)

**File**: `transport-plugin.md`
```rust
/// Unified transport plugin interface
#[async_trait]
pub trait TransportPlugin: Send + Sync + 'static {
    /// Get transport name (e.g., "grpc", "webrtc", "http")
    fn name(&self) -> &'static str;

    /// Create a client instance for this transport
    async fn create_client(
        &self,
        config: ClientConfig,
    ) -> Result<Box<dyn PipelineClient>>;

    /// Create a server instance for this transport
    async fn create_server(
        &self,
        config: ServerConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>>;

    /// Validate configuration for this transport
    fn validate_config(&self, config: &serde_json::Value) -> Result<()>;
}
```

**File**: `plugin-registry.md`
```rust
/// Global registry for transport plugins
pub struct TransportPluginRegistry {
    plugins: HashMap<String, Arc<dyn TransportPlugin>>,
}

impl TransportPluginRegistry {
    /// Register a transport plugin
    pub fn register(&mut self, plugin: Arc<dyn TransportPlugin>) -> Result<()>;

    /// Get plugin by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn TransportPlugin>>;

    /// List all registered plugin names
    pub fn list(&self) -> Vec<String>;
}

/// Get global plugin registry
pub fn global_registry() -> &'static TransportPluginRegistry;
```

**File**: `client-interface.md`
```rust
/// Transport-agnostic client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Endpoint URL/address
    pub endpoint: String,

    /// Optional authentication token
    pub auth_token: Option<String>,

    /// Transport-specific configuration
    /// - gRPC: unused
    /// - WebRTC: { "ice_servers": [...] }
    /// - HTTP: unused
    pub extra_config: Option<serde_json::Value>,
}

/// Transport-agnostic server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Bind address (e.g., "0.0.0.0:50051")
    pub bind_addr: String,

    /// Authentication configuration
    pub auth_config: Option<AuthConfig>,

    /// Transport-specific configuration
    pub extra_config: Option<serde_json::Value>,
}
```

### Quickstart (`quickstart.md`)

**Target Audience**: Developers implementing custom transports

**Scenarios**:
1. Registering built-in plugins at startup
2. Implementing a custom transport plugin
3. Using plugin registry in RemotePipelineNode
4. Testing custom transports

**Example Code**:
```rust
// Implementing a custom transport
struct MyCustomPlugin;

#[async_trait]
impl TransportPlugin for MyCustomPlugin {
    fn name(&self) -> &'static str {
        "mycustom"
    }

    async fn create_client(&self, config: ClientConfig) -> Result<Box<dyn PipelineClient>> {
        let client = MyCustomClient::new(&config.endpoint).await?;
        Ok(Box::new(client))
    }

    async fn create_server(&self, config: ServerConfig, runner: Arc<PipelineRunner>)
        -> Result<Box<dyn PipelineTransport>>
    {
        let server = MyCustomServer::new(&config.bind_addr, runner).await?;
        Ok(Box::new(server))
    }

    fn validate_config(&self, config: &serde_json::Value) -> Result<()> {
        // Validate custom config schema
        Ok(())
    }
}

// Registering at startup
fn main() {
    let registry = global_registry_mut();
    registry.register(Arc::new(MyCustomPlugin))?;

    // Now "mycustom" transport is available
}
```

## Phase 2: Task Generation

Tasks will be generated via `/speckit.tasks` command.

**Estimated Task Breakdown**:
- **Phase 1 (Foundation)**: 8 tasks
  - Define TransportPlugin trait
  - Create plugin registry
  - Move PipelineClient trait to public API
  - Update error types

- **Phase 2 (gRPC Refactor)**: 6 tasks
  - Move grpc.rs from runtime-core to remotemedia-grpc
  - Implement GrpcTransportPlugin
  - Update remotemedia-grpc/lib.rs exports
  - Register GrpcTransportPlugin in tests

- **Phase 3 (WebRTC Refactor)**: 6 tasks
  - Move webrtc.rs from runtime-core to remotemedia-webrtc
  - Implement WebRtcTransportPlugin
  - Update remotemedia-webrtc/lib.rs exports
  - Register WebRtcTransportPlugin in tests

- **Phase 4 (HTTP Handling)**: 4 tasks
  - Keep HTTP in runtime-core (no separate crate)
  - Implement HttpTransportPlugin in runtime-core
  - Register as built-in plugin

- **Phase 5 (RemotePipelineNode Integration)**: 6 tasks
  - Update RemotePipelineNode::get_client() to use registry
  - Remove create_transport_client() factory function
  - Update manifest validation to check plugin availability
  - Update error messages

- **Phase 6 (Testing)**: 8 tasks
  - Test plugin registration
  - Test client creation via plugins
  - Test transport-specific config validation
  - Integration tests with all transports

**Total**: ~38 tasks

## Migration Strategy

### For Users (Backward Compatible)

**No manifest changes required!** Existing manifests work as-is:

```yaml
nodes:
  - id: remote_tts
    node_type: RemotePipelineNode
    params:
      transport: "grpc"  # Still works!
      endpoint: "localhost:50051"
```

Internally, `RemotePipelineNode` now looks up "grpc" in plugin registry instead of hardcoded factory.

### For Transport Developers (Breaking Changes)

If you maintain a custom transport:

1. **Before** (separate client in runtime-core):
   ```rust
   // In your-transport crate
   impl PipelineTransport for YourTransport { ... }

   // In runtime-core (you had to submit PR!)
   impl PipelineClient for YourClient { ... }
   ```

2. **After** (unified plugin):
   ```rust
   // In your-transport crate (self-contained!)
   struct YourTransportPlugin;

   impl TransportPlugin for YourTransportPlugin {
       fn name(&self) -> &'static str { "yourtransport" }
       async fn create_client(...) -> Box<dyn PipelineClient> { ... }
       async fn create_server(...) -> Box<dyn PipelineTransport> { ... }
   }
   ```

### Deprecation Timeline

1. **Phase 1**: Introduce `TransportPlugin` trait alongside existing factories
2. **Phase 2**: Mark `create_transport_client()` as `#[deprecated]`
3. **Phase 3**: Built-in transports use plugin system
4. **Phase 4** (Future): Remove deprecated factory (breaking change, major version bump)

## Dependencies

**New dependencies**: None! Uses existing dependencies.

**Feature gates** (existing):
- `grpc-client` (optional) - enables gRPC transport plugin
- `webrtc` (optional) - enables WebRTC transport plugin

**Removed dependencies from runtime-core**:
- `tonic` moves to optional dependency (only needed if grpc-client feature enabled)
- `webrtc` already optional

## Success Criteria

1. ✅ All existing tests pass without modification
2. ✅ Manifests with `"transport": "grpc"` work exactly as before
3. ✅ Can register custom transport plugin without modifying runtime-core
4. ✅ Binary size without feature gates is smaller (no tonic/webrtc compiled)
5. ✅ Plugin lookup adds <1μs overhead vs hardcoded clients
6. ✅ Integration tests demonstrate custom transport plugin
7. ✅ Documentation shows how to implement custom transport
8. ✅ gRPC and WebRTC client code lives in their respective crates

## Open Questions

1. Should `TransportPlugin::create_client()` be async or sync factory?
   - Async: Allows initialization with network calls (more flexible)
   - Sync: Simpler, initialization happens in client constructor
   - **Decision needed in Phase 0 research**

2. How to handle plugin initialization order and dependencies?
   - Static registration via `inventory` crate?
   - Manual registration in main()?
   - **Decision needed in Phase 0 research**

3. Should we support dynamic library plugins (dlopen)?
   - Not in MVP, but design should allow future extension
   - **Out of scope for initial implementation**

4. Error handling when plugin not found?
   - Return `Error::TransportNotAvailable(name)`?
   - Suggest available transports in error message?
   - **Decision needed in Phase 1 design**
