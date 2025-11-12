# Contract: TransportPlugin Trait

**Feature**: 006-remote-pipeline-node | **Date**: 2025-01-10
**Related**: [data-model.md](../data-model.md) | [plan.md](../plan.md)

## Overview

The `TransportPlugin` trait defines the unified interface that all transport implementations (gRPC, WebRTC, HTTP, custom) must satisfy. It provides factory methods for creating both client and server instances, enabling self-contained transport modules.

This is a Rust trait with async methods, designed to be object-safe for storage in trait objects.

---

## Trait Definition

**Location**: `runtime-core/src/transport/mod.rs`

```rust
use async_trait::async_trait;
use std::sync::Arc;
use crate::{Result, manifest::Manifest, transport::{ClientConfig, ServerConfig, PipelineRunner}};
use crate::transport::client::{PipelineClient, ClientStreamSession};
use crate::transport::session::{PipelineTransport, StreamSession};

/// Unified transport plugin interface
///
/// All transport implementations must implement this trait to integrate
/// with the RemoteMedia runtime. The trait is object-safe and designed
/// for storage in trait objects (Arc<dyn TransportPlugin>).
///
/// # Object Safety
///
/// This trait is intentionally object-safe, which means:
/// - All methods take &self (not self or &mut self)
/// - No generic methods or associated types with Self bounds
/// - All return types use trait objects (Box<dyn Trait>)
///
/// # Thread Safety
///
/// Implementations must be Send + Sync + 'static to allow safe sharing
/// across async tasks and threads.
///
/// # Example Implementation
///
/// ```rust
/// pub struct GrpcTransportPlugin;
///
/// #[async_trait]
/// impl TransportPlugin for GrpcTransportPlugin {
///     fn name(&self) -> &'static str {
///         "grpc"
///     }
///
///     async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
///         let client = GrpcPipelineClient::new(&config.endpoint, config.auth_token.clone()).await?;
///         Ok(Box::new(client))
///     }
///
///     async fn create_server(
///         &self,
///         config: &ServerConfig,
///         runner: Arc<PipelineRunner>,
///     ) -> Result<Box<dyn PipelineTransport>> {
///         let addr = config.bind_addr.parse()?;
///         let server = GrpcServer::new(addr, runner)?;
///         Ok(Box::new(server))
///     }
///
///     fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()> {
///         // gRPC has no extra config requirements
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait TransportPlugin: Send + Sync + 'static {
    /// Get transport name for registry lookup
    ///
    /// Returns a static string identifier that uniquely identifies this transport.
    /// Used as the key when registering/looking up plugins in the registry.
    ///
    /// # Returns
    ///
    /// A static string slice that uniquely identifies this transport.
    /// Common values: "grpc", "webrtc", "http"
    ///
    /// # Examples
    ///
    /// ```rust
    /// let plugin = GrpcTransportPlugin;
    /// assert_eq!(plugin.name(), "grpc");
    /// ```
    ///
    /// # Constraints
    ///
    /// - Must be unique across all registered plugins
    /// - Must be lowercase (convention for consistency)
    /// - Must be static (outlives the plugin instance)
    /// - Typically matches the protocol name
    fn name(&self) -> &'static str;

    /// Create a client instance for this transport
    ///
    /// Factory method that initializes a client capable of executing remote
    /// pipelines via this transport protocol. This method is async to support
    /// transports that require network initialization (e.g., gRPC channel connection).
    ///
    /// # Arguments
    ///
    /// * `config` - Transport-agnostic client configuration containing:
    ///   - `endpoint`: Transport-specific connection string
    ///   - `auth_token`: Optional authentication token
    ///   - `extra_config`: Transport-specific JSON configuration
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn PipelineClient>)` - Initialized and ready-to-use client
    /// * `Err(Error::ConfigError)` - Invalid configuration (endpoint, auth, extra_config)
    /// * `Err(Error::Transport)` - Network initialization failed (connection refused, DNS error, etc.)
    ///
    /// # Errors
    ///
    /// ## Configuration Errors
    ///
    /// * `Error::ConfigError` - Invalid endpoint format for this transport
    /// * `Error::ConfigError` - Missing required fields in extra_config
    /// * `Error::ConfigError` - Invalid extra_config structure
    ///
    /// ## Network Errors
    ///
    /// * `Error::Transport` - Failed to connect to endpoint
    /// * `Error::Transport` - DNS resolution failed
    /// * `Error::Transport` - TLS handshake failed (for secure transports)
    /// * `Error::Transport` - Authentication failed (invalid token)
    ///
    /// # Examples
    ///
    /// ## gRPC Client Creation
    ///
    /// ```rust
    /// let config = ClientConfig {
    ///     endpoint: "localhost:50051".to_string(),
    ///     auth_token: Some("bearer-token".to_string()),
    ///     extra_config: None,
    /// };
    ///
    /// let plugin = GrpcTransportPlugin;
    /// let client = plugin.create_client(&config).await?;
    /// ```
    ///
    /// ## WebRTC Client Creation
    ///
    /// ```rust
    /// let config = ClientConfig {
    ///     endpoint: "wss://signaling.example.com".to_string(),
    ///     auth_token: Some("signaling-token".to_string()),
    ///     extra_config: Some(serde_json::json!({
    ///         "ice_servers": [
    ///             { "urls": "stun:stun.l.google.com:19302" }
    ///         ]
    ///     })),
    /// };
    ///
    /// let plugin = WebRtcTransportPlugin;
    /// let client = plugin.create_client(&config).await?;
    /// ```
    ///
    /// # Implementation Notes
    ///
    /// - Clients should be ready to use immediately after creation
    /// - Connection pooling can be implemented internally
    /// - Transports may perform lazy initialization (connect on first use)
    /// - Clients should implement health checking via PipelineClient::health_check()
    async fn create_client(
        &self,
        config: &ClientConfig,
    ) -> Result<Box<dyn PipelineClient>>;

    /// Create a server instance for this transport
    ///
    /// Factory method that initializes a server capable of receiving and
    /// executing pipeline requests via this transport protocol. The server
    /// uses the provided PipelineRunner to execute incoming requests.
    ///
    /// # Arguments
    ///
    /// * `config` - Transport-agnostic server configuration containing:
    ///   - `bind_addr`: Address to bind server socket
    ///   - `auth_config`: Optional authentication configuration
    ///   - `extra_config`: Transport-specific JSON configuration
    /// * `runner` - Pipeline runner for executing received requests
    ///
    /// # Returns
    ///
    /// * `Ok(Box<dyn PipelineTransport>)` - Initialized server ready to accept requests
    /// * `Err(Error::ConfigError)` - Invalid configuration
    /// * `Err(Error::Transport)` - Server initialization failed
    ///
    /// # Errors
    ///
    /// ## Configuration Errors
    ///
    /// * `Error::ConfigError` - Invalid bind address (e.g., "invalid:port")
    /// * `Error::ConfigError` - Port already in use
    /// * `Error::ConfigError` - Invalid authentication configuration
    /// * `Error::ConfigError` - Invalid extra_config structure
    ///
    /// ## Network Errors
    ///
    /// * `Error::Transport` - Failed to bind socket (permission denied, port in use)
    /// * `Error::Transport` - Failed to start listener
    /// * `Error::Transport` - TLS certificate loading failed (for secure transports)
    ///
    /// # Examples
    ///
    /// ## gRPC Server Creation
    ///
    /// ```rust
    /// let config = ServerConfig {
    ///     bind_addr: "0.0.0.0:50051".to_string(),
    ///     auth_config: Some(AuthConfig::BearerToken {
    ///         valid_tokens: vec!["token1".to_string()],
    ///     }),
    ///     extra_config: Some(serde_json::json!({
    ///         "max_concurrent_streams": 100
    ///     })),
    /// };
    ///
    /// let runner = Arc::new(PipelineRunner::new(registry));
    /// let plugin = GrpcTransportPlugin;
    /// let server = plugin.create_server(&config, runner).await?;
    /// ```
    ///
    /// ## WebRTC Server Creation
    ///
    /// ```rust
    /// let config = ServerConfig {
    ///     bind_addr: "0.0.0.0:8080".to_string(),
    ///     auth_config: None,
    ///     extra_config: Some(serde_json::json!({
    ///         "ice_servers": [
    ///             { "urls": "stun:stun.l.google.com:19302" }
    ///         ],
    ///         "max_peers": 50
    ///     })),
    /// };
    ///
    /// let runner = Arc::new(PipelineRunner::new(registry));
    /// let plugin = WebRtcTransportPlugin;
    /// let server = plugin.create_server(&config, runner).await?;
    /// ```
    ///
    /// # Implementation Notes
    ///
    /// - Servers should bind sockets during creation (not lazily)
    /// - Servers should validate authentication configuration during creation
    /// - Servers should start accepting connections immediately after creation
    /// - Servers should gracefully handle shutdown via PipelineTransport::shutdown()
    async fn create_server(
        &self,
        config: &ServerConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>>;

    /// Validate transport-specific configuration
    ///
    /// Called during manifest validation to catch configuration errors early,
    /// before attempting client/server creation. This allows the system to
    /// fail fast with clear error messages.
    ///
    /// # Arguments
    ///
    /// * `extra_config` - Transport-specific JSON configuration from
    ///   ClientConfig::extra_config or ServerConfig::extra_config
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration is valid for this transport
    /// * `Err(Error::ConfigError)` - Configuration is invalid (with detailed message)
    ///
    /// # Default Implementation
    ///
    /// The default implementation accepts any configuration (no validation).
    /// Transports should override this method if they have required fields
    /// or specific structure requirements.
    ///
    /// # Examples
    ///
    /// ## WebRTC Validation (requires ICE servers)
    ///
    /// ```rust
    /// fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()> {
    ///     if let Some(ice_servers) = extra_config.get("ice_servers") {
    ///         let servers: Vec<IceServerConfig> = serde_json::from_value(ice_servers.clone())
    ///             .map_err(|e| Error::ConfigError(format!("Invalid ice_servers: {}", e)))?;
    ///
    ///         if servers.is_empty() {
    ///             return Err(Error::ConfigError(
    ///                 "WebRTC requires at least one ICE server (STUN/TURN)".to_string()
    ///             ));
    ///         }
    ///     } else {
    ///         return Err(Error::ConfigError(
    ///             "WebRTC requires 'ice_servers' in extra_config".to_string()
    ///         ));
    ///     }
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ## HTTP Validation (timeout must be positive)
    ///
    /// ```rust
    /// fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()> {
    ///     if let Some(timeout) = extra_config.get("timeout_ms") {
    ///         let timeout_val: u64 = timeout.as_u64()
    ///             .ok_or_else(|| Error::ConfigError("timeout_ms must be a positive integer".to_string()))?;
    ///
    ///         if timeout_val == 0 {
    ///             return Err(Error::ConfigError("timeout_ms must be greater than 0".to_string()));
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ## gRPC Validation (no extra config needed)
    ///
    /// ```rust
    /// fn validate_config(&self, _extra_config: &serde_json::Value) -> Result<()> {
    ///     // gRPC has no required extra_config fields
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Implementation Notes
    ///
    /// - Validation is called before create_client/create_server
    /// - Should check for required fields and valid structure
    /// - Should provide clear, actionable error messages
    /// - Should NOT perform network operations (validation only)
    /// - Default implementation is provided for transports with no extra config
    fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()> {
        // Default: accept any config (no validation)
        let _ = extra_config;
        Ok(())
    }
}
```

---

## Object Safety Requirements

For a trait to be object-safe (usable as `dyn TransportPlugin`), it must satisfy:

### ✅ Method Receivers

All methods must have `&self` receiver (not `self` or `&mut self`):
```rust
fn name(&self) -> &'static str;  // ✅ Valid
async fn create_client(&self, ...) -> Result<...>;  // ✅ Valid (async-trait expands to &self)
```

### ✅ No Generics

Methods cannot have generic type parameters:
```rust
fn create<T>(&self) -> T;  // ❌ Not object-safe
fn create(&self) -> Box<dyn Trait>;  // ✅ Object-safe
```

### ✅ No Associated Types with Self Bounds

Associated types cannot reference Self:
```rust
type Client: PipelineClient;  // ❌ Not object-safe
fn create(&self) -> Box<dyn PipelineClient>;  // ✅ Object-safe
```

### ✅ Return Types

Can return trait objects but not Self:
```rust
fn clone(&self) -> Self;  // ❌ Not object-safe
fn create(&self) -> Box<dyn PipelineClient>;  // ✅ Object-safe
```

---

## Error Handling Contract

### Configuration Errors

Returned when plugin configuration is invalid:

```rust
// Invalid endpoint format
Error::ConfigError("Invalid gRPC endpoint: must be host:port".to_string())

// Missing required field
Error::ConfigError("WebRTC requires 'ice_servers' in extra_config".to_string())

// Invalid JSON structure
Error::ConfigError("Invalid ice_servers format: expected array of objects".to_string())
```

### Transport Errors

Returned when network/protocol operations fail:

```rust
// Connection failed
Error::Transport("Failed to connect to localhost:50051: connection refused".to_string())

// DNS resolution failed
Error::Transport("Failed to resolve hostname: unknown host".to_string())

// TLS error
Error::Transport("TLS handshake failed: certificate verification failed".to_string())
```

---

## Implementation Checklist

When implementing a custom transport plugin, ensure:

- [ ] Struct is zero-sized (no fields) or minimal state
- [ ] Implements Send + Sync + 'static bounds
- [ ] `name()` returns unique lowercase identifier
- [ ] `create_client()` validates endpoint format
- [ ] `create_client()` handles auth_token appropriately
- [ ] `create_client()` parses extra_config into transport-specific types
- [ ] `create_server()` validates bind_addr format
- [ ] `create_server()` implements auth_config checking
- [ ] `validate_config()` checks required fields in extra_config
- [ ] Error messages are clear and actionable
- [ ] Created clients implement PipelineClient trait
- [ ] Created servers implement PipelineTransport trait

---

## Usage Examples

### Registering a Plugin

```rust
use remotemedia_runtime_core::transport::plugin_registry::global_registry;

// Get global registry
let registry = global_registry();

// Register custom plugin
{
    let mut lock = registry.write().unwrap();
    lock.register(Arc::new(MyCustomTransportPlugin))?;
}

// Now "mycustom" transport is available in manifests
```

### Creating a Client via Plugin

```rust
// Lookup plugin
let registry = global_registry();
let lock = registry.read().unwrap();
let plugin = lock.get("grpc").unwrap();

// Create client
let config = ClientConfig {
    endpoint: "localhost:50051".to_string(),
    auth_token: None,
    extra_config: None,
};

let client = plugin.create_client(&config).await?;

// Use client
let result = client.execute_unary(manifest, input).await?;
```

### Creating a Server via Plugin

```rust
// Lookup plugin
let registry = global_registry();
let lock = registry.read().unwrap();
let plugin = lock.get("grpc").unwrap();

// Create server
let config = ServerConfig {
    bind_addr: "0.0.0.0:50051".to_string(),
    auth_config: None,
    extra_config: None,
};

let runner = Arc::new(PipelineRunner::new(node_registry));
let server = plugin.create_server(&config, runner).await?;

// Server is now accepting connections
```

---

## Performance Considerations

### Plugin Lookup

- O(1) HashMap lookup: ~10-20ns
- Arc cloning: ~5ns (refcount increment)
- Total overhead: <50ns (negligible)

### Client/Server Creation

- Network initialization dominates (1-10ms typical)
- Plugin abstraction adds <1μs overhead
- Box allocation: ~50ns (one-time cost)

### Memory Overhead

- Plugin trait object: 16 bytes (fat pointer)
- Arc wrapper: 16 bytes
- Created instances: Transport-specific

---

## Testing

### Unit Tests

```rust
#[tokio::test]
async fn test_plugin_name() {
    let plugin = GrpcTransportPlugin;
    assert_eq!(plugin.name(), "grpc");
}

#[tokio::test]
async fn test_create_client_success() {
    let plugin = GrpcTransportPlugin;
    let config = ClientConfig {
        endpoint: "localhost:50051".to_string(),
        auth_token: None,
        extra_config: None,
    };

    let client = plugin.create_client(&config).await;
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_create_client_invalid_endpoint() {
    let plugin = GrpcTransportPlugin;
    let config = ClientConfig {
        endpoint: "invalid endpoint format".to_string(),
        auth_token: None,
        extra_config: None,
    };

    let client = plugin.create_client(&config).await;
    assert!(matches!(client, Err(Error::ConfigError(_))));
}

#[tokio::test]
async fn test_validate_config() {
    let plugin = WebRtcTransportPlugin;

    // Valid config
    let valid_config = serde_json::json!({
        "ice_servers": [
            { "urls": "stun:stun.l.google.com:19302" }
        ]
    });
    assert!(plugin.validate_config(&valid_config).is_ok());

    // Invalid config (missing ice_servers)
    let invalid_config = serde_json::json!({});
    assert!(plugin.validate_config(&invalid_config).is_err());
}
```

---

## References

- **Data Model**: [data-model.md](../data-model.md) - Entity definitions and relationships
- **Implementation Plan**: [plan.md](../plan.md) - Phased implementation strategy
- **Registry Contract**: [plugin-registry.md](./plugin-registry.md) - Registry API specification
- **Config Contract**: [client-config.md](./client-config.md) - ClientConfig and ServerConfig specs
- **Rust Async Trait**: https://docs.rs/async-trait/ - async-trait macro documentation
- **Object Safety**: https://doc.rust-lang.org/reference/items/traits.html#object-safety - Rust reference
