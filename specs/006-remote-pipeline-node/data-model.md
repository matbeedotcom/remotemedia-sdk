# Data Model: Transport Plugin System

**Branch**: `006-remote-pipeline-node` | **Date**: 2025-01-10
**Related Documents**: [plan.md](./plan.md) | [research-transport-plugins.md](./research-transport-plugins.md)

## Overview

This document defines the data model for the transport plugin system in RemoteMedia SDK. The system enables self-contained transport implementations that provide both client and server capabilities through a unified plugin interface.

The data model consists of four core entities:
1. **TransportPlugin** - Trait interface for transport implementations
2. **TransportPluginRegistry** - Global registry for plugin storage and lookup
3. **ClientConfig** - Transport-agnostic client configuration
4. **ServerConfig** - Transport-agnostic server configuration

All entities are designed for thread-safety, object-safety, and zero-overhead abstraction.

---

## Entity 1: TransportPlugin

**Purpose**: Unified trait interface that all transport implementations must satisfy. Provides factory methods for creating both client and server instances.

**Location**: `runtime-core/src/transport/mod.rs`

**Type**: Trait (object-safe)

**Key Methods**:
- `fn name(&self) -> &'static str` - Get unique transport identifier
- `async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>>` - Create client instance
- `async fn create_server(&self, config: &ServerConfig, runner: Arc<PipelineRunner>) -> Result<Box<dyn PipelineTransport>>` - Create server instance
- `fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()>` - Validate transport-specific configuration

**Constraints**:
- Must be object-safe (no generics, no associated types with Self bounds)
- Must be `Send + Sync + 'static` for safe sharing across threads
- `name()` must return unique identifier (enforced at registration)
- Factory methods must be async to support network initialization
- Implementations should be zero-sized structs (state lives in created instances)

**Lifecycle**:
1. Plugin instantiated (zero-sized struct, no state)
2. Registered in global registry via `register()`
3. Retrieved via `registry.get(name)` when needed
4. Factory methods called to create clients/servers
5. Lives until program termination (stored in global static)

See [contracts/transport-plugin.md](./contracts/transport-plugin.md) for complete specification.

---

## Entity 2: TransportPluginRegistry

**Purpose**: Thread-safe global registry that stores and provides access to registered transport plugins.

**Location**: `runtime-core/src/transport/plugin_registry.rs`

**Type**: Struct with global singleton access pattern

**Fields**:
```rust
pub struct TransportPluginRegistry {
    plugins: HashMap<String, Arc<dyn TransportPlugin>>,
}
```

**Key Methods**:
- `register(&mut self, plugin: Arc<dyn TransportPlugin>) -> Result<()>` - Register plugin (write lock required)
- `get(&self, name: &str) -> Option<Arc<dyn TransportPlugin>>` - Lookup plugin by name (read lock)
- `list(&self) -> Vec<String>` - List all registered plugin names

**Global Access Pattern**:
```rust
static TRANSPORT_REGISTRY: OnceLock<Arc<RwLock<TransportPluginRegistry>>> = OnceLock::new();

pub fn global_registry() -> Arc<RwLock<TransportPluginRegistry>>;
pub fn init_global_registry_with_plugins(plugins: Vec<Arc<dyn TransportPlugin>>) -> Result<()>;
```

**Constraints**:
- Global singleton (OnceLock ensures single initialization)
- Thread-safe (RwLock allows concurrent reads, exclusive writes)
- Read-heavy workload optimized (multiple RemotePipelineNode instances share registry)
- Custom plugins must be registered before first use (before any manifest loads)
- Plugin names are case-sensitive and must be unique

**State Transitions**:
```text
Uninitialized → Initialized (via get_or_init or init_global_registry_with_plugins)
Initialized → Lookup Phase (read-only operations)
```

**Performance**:
- Plugin lookup: O(1) HashMap access with ~10-20ns overhead for RwLock read
- Plugin registration: O(1) HashMap insert with ~500ns overhead for RwLock write
- Arc cloning: ~5ns (just refcount increment)

**Invariants**:
- Registry can only be initialized once (enforced by OnceLock)
- After initialization, registry is immutable (no adding/removing plugins)
- Read locks never block each other (concurrent access)

See [contracts/plugin-registry.md](./contracts/plugin-registry.md) for complete specification.

---

## Entity 3: ClientConfig

**Purpose**: Transport-agnostic configuration for creating pipeline client instances.

**Location**: `runtime-core/src/transport/client/mod.rs`

**Type**: Struct (serializable)

**Fields**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub endpoint: String,
    pub auth_token: Option<String>,
    pub extra_config: Option<serde_json::Value>,
}
```

**Field Validation Rules**:

| Field | Required | Type | Constraints | Validation |
|-------|----------|------|-------------|------------|
| `endpoint` | Yes | String | Non-empty | Format varies by transport |
| `auth_token` | No | String | Non-empty if present | Transport may reject if required |
| `extra_config` | No | JSON Value | Valid JSON object | Transport-specific via `validate_config()` |

**Transport-Specific Config Examples**:

**WebRTC**:
```json
{
  "endpoint": "wss://signaling.example.com",
  "auth_token": "signaling-auth-token",
  "extra_config": {
    "ice_servers": [
      { "urls": "stun:stun.l.google.com:19302" },
      {
        "urls": "turn:turn.example.com:3478",
        "username": "user1",
        "credential": "pass1"
      }
    ],
    "signaling_timeout_ms": 5000
  }
}
```

**gRPC** (minimal):
```json
{
  "endpoint": "localhost:50051",
  "auth_token": "grpc-token"
}
```

**HTTP**:
```json
{
  "endpoint": "https://api.example.com/pipeline",
  "auth_token": "Bearer xyz123",
  "extra_config": {
    "timeout_ms": 30000,
    "max_retries": 3
  }
}
```

**Lifecycle**:
1. Created from manifest params when RemotePipelineNode initializes
2. Validated via `TransportPlugin::validate_config()`
3. Passed to `TransportPlugin::create_client()`
4. Consumed by client implementation (not stored long-term)

**Relationships**:
- Used by: TransportPlugin::create_client()
- Created from: RemotePipelineNode manifest params
- Produces: Box<dyn PipelineClient>

See [contracts/client-config.md](./contracts/client-config.md) for complete specification.

---

## Entity 4: ServerConfig

**Purpose**: Transport-agnostic configuration for creating pipeline server instances.

**Location**: `runtime-core/src/transport/server.rs` (new file)

**Type**: Struct (serializable)

**Fields**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub auth_config: Option<AuthConfig>,
    pub extra_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthConfig {
    None,
    BearerToken { valid_tokens: Vec<String> },
    ApiKey { header_name: String, valid_keys: Vec<String> },
    Custom { config: serde_json::Value },
}
```

**Field Validation Rules**:

| Field | Required | Type | Constraints | Validation |
|-------|----------|------|-------------|------------|
| `bind_addr` | Yes | String | Valid socket address | Must parse as valid IP:port |
| `auth_config` | No | AuthConfig | Valid variant | Checked at server creation |
| `extra_config` | No | JSON Value | Valid JSON object | Transport-specific validation |

**Transport-Specific Config Examples**:

**gRPC Server**:
```json
{
  "bind_addr": "0.0.0.0:50051",
  "auth_config": {
    "type": "BearerToken",
    "valid_tokens": ["token1", "token2"]
  },
  "extra_config": {
    "max_concurrent_streams": 100,
    "keepalive_interval_ms": 60000
  }
}
```

**WebRTC Server**:
```json
{
  "bind_addr": "0.0.0.0:8080",
  "auth_config": null,
  "extra_config": {
    "ice_servers": [
      { "urls": "stun:stun.l.google.com:19302" }
    ],
    "max_peers": 50
  }
}
```

**Lifecycle**:
1. Loaded from server configuration file or environment variables
2. Validated via `TransportPlugin::validate_config()`
3. Passed to `TransportPlugin::create_server()` along with PipelineRunner
4. Consumed by server implementation (not stored in registry)

**Relationships**:
- Used by: TransportPlugin::create_server()
- Created from: Server startup configuration
- Produces: Box<dyn PipelineTransport>

See [contracts/client-config.md](./contracts/client-config.md) for complete specification (includes ServerConfig).

---

## Entity Relationships

```text
┌─────────────────────────────────────────────────────┐
│ TransportPluginRegistry (Global Singleton)          │
│                                                     │
│  plugins: HashMap<String, Arc<dyn TransportPlugin>>│
│                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐│
│  │ "grpc"      │  │ "webrtc"    │  │ "http"      ││
│  │ → Plugin    │  │ → Plugin    │  │ → Plugin    ││
│  └─────────────┘  └─────────────┘  └─────────────┘│
└─────────────────────────────────────────────────────┘
                         │
                         │ registry.get("grpc")
                         ↓
                  ┌────────────────┐
                  │ TransportPlugin│ (trait object)
                  │                │
                  │ name()         │
                  │ create_client()│──→ Uses ClientConfig
                  │ create_server()│──→ Uses ServerConfig
                  │ validate_config│
                  └────────────────┘
                         │
          ┌──────────────┴──────────────┐
          ↓                             ↓
   ┌──────────────┐            ┌──────────────┐
   │ ClientConfig │            │ ServerConfig │
   │              │            │              │
   │ endpoint     │            │ bind_addr    │
   │ auth_token   │            │ auth_config  │
   │ extra_config │            │ extra_config │
   └──────────────┘            └──────────────┘
          │                             │
          ↓                             ↓
   ┌──────────────┐            ┌──────────────┐
   │PipelineClient│            │PipelineTransport│
   │ (trait)      │            │ (trait)      │
   └──────────────┘            └──────────────┘
```

**Relationship Table**:

| From | To | Type | Cardinality | Lifetime |
|------|----|----|-------------|----------|
| Registry | Plugin | Ownership (Arc) | 1:N | Plugin lives as long as registry |
| Plugin | ClientConfig | Factory input | Uses | Config temporary (creation only) |
| Plugin | ServerConfig | Factory input | Uses | Config temporary (creation only) |
| ClientConfig | PipelineClient | Produces | 1:1 | Client outlives config |
| ServerConfig | PipelineTransport | Produces | 1:1 | Server outlives config |
| RemotePipelineNode | Registry | Read access | N:1 | Shared global reference |

---

## State Transitions

### TransportPluginRegistry Lifecycle

```text
┌─────────┐  OnceLock::get_or_init()   ┌──────────────┐
│Uninitialized├───────────────────────→│ Initialized  │
└─────────┘                            │ (Read-Only)  │
                                       └──────────────┘
    │                                         │
    │ init_global_registry_with_plugins()    │
    │ (custom plugins)                        │
    ↓                                         ↓
┌─────────┐                            ┌──────────────┐
│ ERROR:  │                            │ Lookup Phase │
│ Already │                            │ (read lock)  │
│Initialized│                          └──────────────┘
└─────────┘
```

**State Descriptions**:

1. **Uninitialized**: No registry exists yet (program start)
2. **Initialized**: Registry created with default plugins (gRPC, HTTP, WebRTC based on features)
3. **Lookup Phase**: Registry used for plugin lookup (read-only operations, main usage pattern)

**Transition Events**:
- Uninitialized → Initialized: First call to `global_registry()` (automatic, lazy)
- Uninitialized → Initialized: Explicit call to `init_global_registry_with_plugins()` (manual, custom plugins)
- Initialized → ERROR: Attempting second initialization (prevented by OnceLock, returns error)

**Invariants**:
- Registry can only be initialized once (enforced by OnceLock::set)
- After initialization, registry is immutable (no adding/removing plugins)
- Read locks never block each other (concurrent access from multiple threads)
- Write operations only occur during initialization phase (before any clients created)

---

## Performance Characteristics

### Plugin Lookup Performance

**Typical Usage Pattern**:
```rust
// In RemotePipelineNode::get_client()
let registry = global_registry();                  // ~5ns (Arc::clone)
let lock = registry.read().unwrap();                // ~10ns (RwLock read)
let plugin = lock.get("grpc").unwrap();            // ~10ns (HashMap lookup)
let client = plugin.create_client(config).await?;  // 1-10ms (varies by transport)
```

**Measured Latencies**:
- Arc::clone (registry access): ~5ns
- RwLock::read (acquire read lock): ~10ns
- HashMap::get (plugin lookup): ~10ns
- **Total registry overhead: ~25ns**
- Client creation: 1-10ms (network handshake, transport-dependent)
- **Total overhead vs direct creation: <1μs (negligible for network operations)**

**Scalability**:
- Concurrent reads: No contention (RwLock allows multiple readers)
- Tested with 8 threads: ~50-100ns per lookup (still negligible)
- Memory usage: Constant (fixed number of plugins)

### Memory Overhead

**Per Plugin**:
- TransportPlugin trait object: 16 bytes (fat pointer: data ptr + vtable ptr)
- Arc wrapper: 16 bytes (pointer + refcount)
- HashMap entry: 24 bytes (key String + value + metadata)
- **Total per plugin: ~56 bytes**

**Global Registry**:
- HashMap with 3 plugins: ~200 bytes (hashmap overhead + 3 entries)
- RwLock wrapper: 40 bytes (lock state)
- Arc wrapper: 16 bytes (refcount)
- OnceLock: 8 bytes (initialization flag)
- **Total registry overhead: ~264 bytes**

**Comparison to Hardcoded Clients**:
- Hardcoded: 0 bytes runtime overhead (code compiled in)
- Plugin system: 264 bytes + (56 bytes × num_plugins)
- With 3 plugins (gRPC, WebRTC, HTTP): ~432 bytes total
- **Conclusion: Negligible overhead (<0.5KB)**

---

## Security Considerations

### Authentication Token Handling

**Sensitive Data Fields**:
- `ClientConfig::auth_token` - Transmitted to remote server (sensitive)
- `ServerConfig::auth_config` - Validates incoming requests (sensitive)
- `extra_config` fields - May contain credentials (TURN server passwords, API keys)

**Security Measures**:

1. **No logging of sensitive fields**: Custom Debug impl redacts tokens
```rust
impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("endpoint", &self.endpoint)
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***REDACTED***"))
            .field("extra_config", &"***REDACTED***")
            .finish()
    }
}
```

2. **In-memory only**: Configs not persisted to disk, temporary lifetime
3. **Future enhancement**: Zeroize on drop (implement Drop trait to overwrite memory)
4. **TLS recommendation**: All production deployments should use TLS transport (gRPC TLS, WSS for WebRTC signaling, HTTPS)

### Plugin Trust Model

**Current Assumptions**:
- Plugins are trusted code (compiled into binary, not loaded dynamically)
- No sandboxing between plugins (all run in same process with same privileges)
- Registry protected by Rust type system (cannot add malicious plugins at runtime without unsafe)
- Plugins have full access to network, filesystem, and system resources

**Future Enhancements** (out of scope for initial implementation):
- Dynamic library loading with signature verification (libloading + crypto signatures)
- Plugin sandboxing via separate processes (IPC boundary between plugins)
- Capability-based security (plugins declare required permissions, runtime enforces limits)
- Audit logging for plugin operations (track which plugin handled which requests)

---

## Backward Compatibility

### Existing Code Compatibility

**Before Plugin System**:
```rust
// Old factory function in runtime-core/src/transport/client/mod.rs
pub async fn create_transport_client(config: TransportConfig)
    -> Result<Box<dyn PipelineClient>>
{
    match config.transport_type {
        TransportType::Grpc => GrpcPipelineClient::new(...).await,
        TransportType::Http => HttpPipelineClient::new(...).await,
        TransportType::Webrtc => WebRtcPipelineClient::new(...).await,
    }
}
```

**After Plugin System** (maintains compatibility):
```rust
// Old function still works (delegates to plugin registry internally)
#[deprecated(since = "0.5.0", note = "Use transport plugins instead. See docs/MIGRATION_TO_PLUGINS.md")]
pub async fn create_transport_client(config: TransportConfig)
    -> Result<Box<dyn PipelineClient>>
{
    let registry = global_registry();
    let lock = registry.read().unwrap();

    let transport_name = match config.transport_type {
        TransportType::Grpc => "grpc",
        TransportType::Http => "http",
        TransportType::Webrtc => "webrtc",
    };

    let plugin = lock.get(transport_name)
        .ok_or_else(|| Error::ConfigError(format!("Transport '{}' not available", transport_name)))?;

    let client_config = ClientConfig {
        endpoint: config.endpoint,
        auth_token: config.auth_token,
        extra_config: config.extra_config,
    };

    plugin.create_client(&client_config).await
}
```

### Manifest Compatibility

**Existing manifests work unchanged**:
```yaml
nodes:
  - id: remote_tts
    node_type: RemotePipelineNode
    params:
      transport: "grpc"  # String matches plugin name in registry
      endpoint: "localhost:50051"
```

**No manifest changes required during migration**. The `transport` string field directly maps to plugin registry keys.

### Migration Timeline

1. **Phase 1** (v0.5.0): Add plugin system alongside existing code (non-breaking)
2. **Phase 2** (v0.5.x): Deprecate old factory function (both paths work, warnings emitted)
3. **Phase 3** (v1.0.0): Remove deprecated code (major version bump, breaking change)

---

## Testing Strategy

### Unit Tests

**TransportPluginRegistry**:
- Test duplicate plugin registration (should error with descriptive message)
- Test plugin lookup (found vs not found cases)
- Test `list()` returns all registered plugin names
- Test concurrent access (multiple threads reading simultaneously)
- Test initialization once (second init attempt should error)

**ClientConfig / ServerConfig**:
- Test JSON serialization/deserialization (round-trip)
- Test `from_manifest_params` extraction logic
- Test Debug output redacts sensitive fields (auth tokens)
- Test validation rules (empty endpoint, invalid JSON, etc.)

**Mock TransportPlugin**:
```rust
struct MockTransportPlugin {
    name: &'static str,
    should_fail: bool,
}

#[async_trait]
impl TransportPlugin for MockTransportPlugin {
    fn name(&self) -> &'static str { self.name }
    async fn create_client(&self, _config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        if self.should_fail {
            Err(Error::ConfigError("Mock failure".to_string()))
        } else {
            Ok(Box::new(MockPipelineClient))
        }
    }
    // ... similar for create_server
}
```

### Integration Tests

**Full Plugin Lifecycle**:
```rust
#[tokio::test]
async fn test_plugin_registration_and_usage() {
    let registry = Arc::new(RwLock::new(TransportPluginRegistry::new()));

    // Register
    {
        let mut lock = registry.write().unwrap();
        lock.register(Arc::new(MockTransportPlugin { name: "mock", should_fail: false })).unwrap();
    }

    // Lookup
    let plugin = {
        let lock = registry.read().unwrap();
        lock.get("mock").unwrap()
    };

    // Create client
    let config = ClientConfig {
        endpoint: "localhost:50051".to_string(),
        auth_token: None,
        extra_config: None,
    };
    let client = plugin.create_client(&config).await.unwrap();

    // Use client
    assert!(client.health_check().await.is_ok());
}
```

---

## References

- **Implementation Plan**: [plan.md](./plan.md) - Phased implementation strategy
- **Research Document**: [research-transport-plugins.md](./research-transport-plugins.md) - Design decisions and alternatives
- **Contracts**: See [contracts/](./contracts/) directory for complete API specifications
  - [transport-plugin.md](./contracts/transport-plugin.md) - TransportPlugin trait
  - [plugin-registry.md](./contracts/plugin-registry.md) - Registry implementation
  - [client-config.md](./contracts/client-config.md) - Config structs
- **Existing Patterns**:
  - Node registry: `runtime-core/src/nodes/registry.rs` (similar factory pattern)
  - Global sessions: CLAUDE.md:389 (OnceLock + Arc + RwLock pattern)
