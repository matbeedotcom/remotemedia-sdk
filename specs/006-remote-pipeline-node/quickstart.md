# Quickstart Guide: Implementing Custom Transport Plugins

**Feature**: 006-remote-pipeline-node | **Date**: 2025-01-10
**Related**: [plan.md](./plan.md) | [data-model.md](./data-model.md) | [contracts/](./contracts/)

## Overview

This guide shows transport developers how to implement a custom transport plugin for RemoteMedia SDK. By implementing the `TransportPlugin` trait, you can create self-contained transport modules that provide both client and server capabilities.

**Target Audience**: Developers building custom transport layers (e.g., QUIC, WebSocket, custom protocols)

**Prerequisites**:
- Rust 1.75+
- Familiarity with async Rust (tokio)
- Understanding of your transport protocol

---

## Quick Example: Echo Transport

Here's a minimal working transport plugin that echoes data back (useful for testing):

```rust
use remotemedia_runtime_core::transport::{
    TransportPlugin, ClientConfig, ServerConfig,
    PipelineClient, PipelineTransport, ClientStreamSession, StreamSession,
    TransportData, PipelineRunner,
};
use async_trait::async_trait;
use std::sync::Arc;

// 1. Define plugin struct (zero-sized, no state)
pub struct EchoTransportPlugin;

// 2. Implement TransportPlugin trait
#[async_trait]
impl TransportPlugin for EchoTransportPlugin {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        Ok(Box::new(EchoClient {
            endpoint: config.endpoint.clone(),
        }))
    }

    async fn create_server(
        &self,
        config: &ServerConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>> {
        Ok(Box::new(EchoServer {
            bind_addr: config.bind_addr.clone(),
            runner,
        }))
    }

    fn validate_config(&self, _extra_config: &serde_json::Value) -> Result<()> {
        Ok(())  // No required config
    }
}

// 3. Register plugin at startup
#[tokio::main]
async fn main() -> Result<()> {
    use remotemedia_runtime_core::transport::plugin_registry::global_registry;

    let registry = global_registry();
    {
        let mut lock = registry.write().unwrap();
        lock.register(Arc::new(EchoTransportPlugin))?;
    }

    println!("Echo transport registered!");

    // Now "echo" transport is available in manifests
    Ok(())
}
```

Now you can use `"transport": "echo"` in manifests!

---

## Step-by-Step Guide

### Step 1: Create Plugin Struct

Define a zero-sized struct for your transport:

```rust
/// MyCustom transport plugin
///
/// Implements custom protocol for pipeline execution.
pub struct MyCustomTransportPlugin;
```

**Best Practices**:
- Use zero-sized struct (no fields) to minimize memory overhead
- Plugin instances are stateless (state lives in created clients/servers)
- Add doc comments explaining your transport's purpose

---

### Step 2: Implement TransportPlugin Trait

```rust
use async_trait::async_trait;
use remotemedia_runtime_core::transport::TransportPlugin;

#[async_trait]
impl TransportPlugin for MyCustomTransportPlugin {
    fn name(&self) -> &'static str {
        "mycustom"  // Unique identifier (lowercase by convention)
    }

    async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        // Parse endpoint
        let endpoint = config.endpoint.parse::<SocketAddr>()
            .map_err(|e| Error::ConfigError(format!("Invalid endpoint: {}", e)))?;

        // Parse transport-specific config
        let custom_config = if let Some(extra) = &config.extra_config {
            serde_json::from_value::<MyCustomConfig>(extra.clone())
                .map_err(|e| Error::ConfigError(format!("Invalid custom config: {}", e)))?
        } else {
            MyCustomConfig::default()
        };

        // Create and initialize client
        let client = MyCustomClient::new(endpoint, config.auth_token.clone(), custom_config).await?;

        Ok(Box::new(client))
    }

    async fn create_server(
        &self,
        config: &ServerConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>> {
        // Parse bind address
        let bind_addr = config.bind_addr.parse::<SocketAddr>()
            .map_err(|e| Error::ConfigError(format!("Invalid bind address: {}", e)))?;

        // Parse transport-specific config
        let custom_config = if let Some(extra) = &config.extra_config {
            serde_json::from_value::<MyCustomServerConfig>(extra.clone())
                .map_err(|e| Error::ConfigError(format!("Invalid server config: {}", e)))?
        } else {
            MyCustomServerConfig::default()
        };

        // Create and start server
        let server = MyCustomServer::new(bind_addr, runner, custom_config).await?;

        Ok(Box::new(server))
    }

    fn validate_config(&self, extra_config: &serde_json::Value) -> Result<()> {
        // Validate transport-specific config structure
        if let Some(timeout) = extra_config.get("timeout_ms") {
            if !timeout.is_u64() || timeout.as_u64().unwrap() == 0 {
                return Err(Error::ConfigError(
                    "timeout_ms must be a positive integer".to_string()
                ));
            }
        }

        // Validate required fields
        if extra_config.get("required_field").is_none() {
            return Err(Error::ConfigError(
                "required_field is missing in transport_config".to_string()
            ));
        }

        Ok(())
    }
}
```

**Key Points**:
- `name()` must return unique identifier (enforced at registration)
- `create_client()` and `create_server()` should validate config and return ready-to-use instances
- `validate_config()` should check structure and required fields (called before creation)
- All methods should return clear, actionable error messages

---

### Step 3: Implement Client (PipelineClient Trait)

```rust
use remotemedia_runtime_core::transport::client::{PipelineClient, ClientStreamSession};

pub struct MyCustomClient {
    endpoint: SocketAddr,
    auth_token: Option<String>,
    config: MyCustomConfig,
    // Internal state (connection pool, etc.)
}

impl MyCustomClient {
    pub async fn new(
        endpoint: SocketAddr,
        auth_token: Option<String>,
        config: MyCustomConfig,
    ) -> Result<Self> {
        // Initialize connection (if eager) or prepare for lazy initialization
        Ok(Self {
            endpoint,
            auth_token,
            config,
        })
    }
}

#[async_trait]
impl PipelineClient for MyCustomClient {
    async fn execute_unary(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // 1. Connect to remote endpoint
        let mut connection = self.connect().await?;

        // 2. Send manifest and input
        connection.send_manifest(&manifest).await?;
        connection.send_data(&input).await?;

        // 3. Receive output
        let output = connection.receive_data().await?;

        // 4. Close connection
        connection.close().await?;

        Ok(output)
    }

    async fn create_stream_session(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn ClientStreamSession>> {
        // Create long-lived bidirectional connection
        let connection = self.connect().await?;

        let session = MyCustomStreamSession {
            session_id: uuid::Uuid::new_v4().to_string(),
            connection,
            manifest,
        };

        Ok(Box::new(session))
    }

    async fn health_check(&self) -> Result<bool> {
        // Simple connectivity check (ping/pong or similar)
        match self.connect().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

impl MyCustomClient {
    async fn connect(&self) -> Result<Connection> {
        // Transport-specific connection logic
        // Example: TCP connect + custom handshake
        let stream = TcpStream::connect(self.endpoint).await
            .map_err(|e| Error::Transport(format!("Connection failed: {}", e)))?;

        // Perform auth if token provided
        if let Some(token) = &self.auth_token {
            // Send auth token
            // ...
        }

        Ok(Connection { stream })
    }
}
```

**Best Practices**:
- Implement connection pooling if your protocol supports it
- Handle authentication in `connect()` helper
- Provide clear error messages for network failures
- Consider lazy connection (connect on first use)

---

### Step 4: Implement Server (PipelineTransport Trait)

```rust
use remotemedia_runtime_core::transport::{PipelineTransport, StreamSession};

pub struct MyCustomServer {
    bind_addr: SocketAddr,
    runner: Arc<PipelineRunner>,
    config: MyCustomServerConfig,
    // Internal state (listener, active sessions, etc.)
}

impl MyCustomServer {
    pub async fn new(
        bind_addr: SocketAddr,
        runner: Arc<PipelineRunner>,
        config: MyCustomServerConfig,
    ) -> Result<Self> {
        // Bind socket
        let listener = TcpListener::bind(bind_addr).await
            .map_err(|e| Error::Transport(format!("Failed to bind: {}", e)))?;

        let server = Self {
            bind_addr,
            runner,
            config,
        };

        // Spawn accept loop
        server.start_accept_loop(listener);

        Ok(server)
    }

    fn start_accept_loop(&self, listener: TcpListener) {
        let runner = Arc::clone(&self.runner);

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let runner = Arc::clone(&runner);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, runner).await {
                                eprintln!("Connection error from {}: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Accept error: {}", e);
                    }
                }
            }
        });
    }
}

#[async_trait]
impl PipelineTransport for MyCustomServer {
    async fn execute(
        &self,
        manifest: Arc<Manifest>,
        input: TransportData,
    ) -> Result<TransportData> {
        // Execute pipeline via runner
        self.runner.execute_unary(manifest, input).await
    }

    async fn stream(
        &self,
        manifest: Arc<Manifest>,
    ) -> Result<Box<dyn StreamSession>> {
        // Create streaming session via runner
        self.runner.create_stream_session(manifest).await
    }
}

async fn handle_connection(stream: TcpStream, runner: Arc<PipelineRunner>) -> Result<()> {
    // Read manifest from connection
    let manifest = read_manifest_from_stream(&stream).await?;

    // Read input data
    let input = read_data_from_stream(&stream).await?;

    // Execute pipeline
    let output = runner.execute_unary(Arc::new(manifest), input).await?;

    // Write output back to connection
    write_data_to_stream(&stream, &output).await?;

    Ok(())
}
```

**Best Practices**:
- Bind socket during server creation (fail fast if port in use)
- Spawn accept loop in background task
- Handle each connection in separate task
- Implement graceful shutdown (stop accepting, drain active connections)
- Validate authentication before executing pipelines

---

### Step 5: Register Plugin

#### Option A: Automatic Registration (if plugin is in same crate)

```rust
// runtime-core/src/transport/plugin_registry.rs

fn register_default_plugins(registry: &mut TransportPluginRegistry) -> Result<()> {
    // ... existing plugins

    #[cfg(feature = "mycustom-transport")]
    {
        registry.register(Arc::new(MyCustomTransportPlugin))?;
    }

    Ok(())
}
```

#### Option B: Manual Registration (custom plugin in separate crate)

```rust
// my-app/src/main.rs

use remotemedia_runtime_core::transport::plugin_registry::init_global_registry_with_plugins;
use my_custom_transport::MyCustomTransportPlugin;

#[tokio::main]
async fn main() -> Result<()> {
    // Register custom plugin BEFORE any runtime-core usage
    init_global_registry_with_plugins(vec![
        Arc::new(MyCustomTransportPlugin),
    ])?;

    // Now "mycustom" transport is available
    let runner = PipelineRunner::new(registry);
    // ...

    Ok(())
}
```

**Best Practices**:
- Register plugins at program startup (before first pipeline execution)
- Use `init_global_registry_with_plugins()` for custom plugins
- Add feature flag for optional plugins (reduces binary size)

---

### Step 6: Test Your Plugin

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_name() {
        let plugin = MyCustomTransportPlugin;
        assert_eq!(plugin.name(), "mycustom");
    }

    #[tokio::test]
    async fn test_create_client() {
        let plugin = MyCustomTransportPlugin;
        let config = ClientConfig {
            endpoint: "localhost:50051".to_string(),
            auth_token: None,
            extra_config: None,
        };

        let client = plugin.create_client(&config).await;
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_validate_config_missing_required_field() {
        let plugin = MyCustomTransportPlugin;
        let invalid_config = serde_json::json!({});

        let result = plugin.validate_config(&invalid_config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_roundtrip() {
        // Start server
        let runner = Arc::new(PipelineRunner::new(registry));
        let server_config = ServerConfig {
            bind_addr: "127.0.0.1:0".to_string(),  // Random port
            auth_config: None,
            extra_config: None,
        };
        let server = MyCustomTransportPlugin.create_server(&server_config, runner).await.unwrap();

        // Create client
        let client_config = ClientConfig {
            endpoint: format!("127.0.0.1:{}", server.actual_port()),
            auth_token: None,
            extra_config: None,
        };
        let client = MyCustomTransportPlugin.create_client(&client_config).await.unwrap();

        // Execute pipeline
        let manifest = Arc::new(test_manifest());
        let input = TransportData::text("hello".to_string());
        let output = client.execute_unary(manifest, input).await.unwrap();

        assert_eq!(output.as_text().unwrap(), "hello");  // Echo
    }
}
```

---

## Common Patterns

### Pattern 1: Connection Pooling

```rust
use tokio::sync::Mutex;
use std::collections::VecDeque;

pub struct MyCustomClient {
    endpoint: SocketAddr,
    connection_pool: Arc<Mutex<VecDeque<Connection>>>,
    max_connections: usize,
}

impl MyCustomClient {
    async fn get_connection(&self) -> Result<Connection> {
        let mut pool = self.connection_pool.lock().await;

        // Reuse existing connection if available
        if let Some(conn) = pool.pop_front() {
            return Ok(conn);
        }

        // Create new connection if pool not at max
        if pool.len() < self.max_connections {
            return self.create_new_connection().await;
        }

        // Wait for connection to become available
        // (or implement more sophisticated pooling)
        drop(pool);
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.get_connection().await
    }

    async fn return_connection(&self, conn: Connection) {
        let mut pool = self.connection_pool.lock().await;
        pool.push_back(conn);
    }
}
```

### Pattern 2: Authentication Middleware

```rust
async fn authenticate_request(
    auth_config: &Option<AuthConfig>,
    headers: &Headers,
) -> Result<()> {
    match auth_config {
        None => Ok(()),  // No auth required

        Some(AuthConfig::BearerToken { valid_tokens }) => {
            let auth_header = headers.get("Authorization")
                .ok_or_else(|| Error::Unauthorized("Missing Authorization header".to_string()))?;

            let token = auth_header.strip_prefix("Bearer ")
                .ok_or_else(|| Error::Unauthorized("Invalid Authorization format".to_string()))?;

            if !valid_tokens.contains(&token.to_string()) {
                return Err(Error::Unauthorized("Invalid token".to_string()));
            }

            Ok(())
        }

        Some(AuthConfig::ApiKey { header_name, valid_keys }) => {
            let key = headers.get(header_name)
                .ok_or_else(|| Error::Unauthorized(format!("Missing {} header", header_name)))?;

            if !valid_keys.contains(&key.to_string()) {
                return Err(Error::Unauthorized("Invalid API key".to_string()));
            }

            Ok(())
        }

        Some(AuthConfig::Custom { config }) => {
            // Transport-specific auth logic
            custom_authenticate(config, headers).await
        }
    }
}
```

### Pattern 3: Error Mapping

```rust
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::ConnectionRefused => {
                Error::Transport(format!("Connection refused: {}", e))
            }
            std::io::ErrorKind::TimedOut => {
                Error::Timeout("Connection timed out".to_string())
            }
            std::io::ErrorKind::PermissionDenied => {
                Error::Transport(format!("Permission denied: {}", e))
            }
            _ => Error::Transport(format!("IO error: {}", e)),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::ConfigError(format!("JSON parsing error: {}", e))
    }
}
```

### Pattern 4: Health Checking

```rust
impl MyCustomClient {
    async fn health_check(&self) -> Result<bool> {
        // Simple ping/pong health check
        match self.ping().await {
            Ok(_) => Ok(true),
            Err(Error::Transport(_)) => Ok(false),  // Network error = unhealthy
            Err(e) => Err(e),  // Other errors propagate
        }
    }

    async fn ping(&self) -> Result<()> {
        let conn = self.get_connection().await?;

        // Send ping
        conn.write_frame(&Frame::Ping).await?;

        // Wait for pong (with timeout)
        tokio::time::timeout(
            Duration::from_secs(5),
            conn.read_frame()
        ).await
            .map_err(|_| Error::Timeout("Ping timeout".to_string()))?
            .map_err(|e| Error::Transport(format!("Ping failed: {}", e)))?;

        self.return_connection(conn).await;

        Ok(())
    }
}
```

---

## Troubleshooting

### Plugin Not Found Error

```
Error: Transport 'mycustom' not available. Available: ["grpc", "http", "webrtc"]
```

**Solution**: Register plugin before first use:
```rust
init_global_registry_with_plugins(vec![Arc::new(MyCustomTransportPlugin)])?;
```

### Duplicate Registration Error

```
Error: Transport plugin 'mycustom' already registered
```

**Solution**: Check that plugin isn't registered multiple times or conflicts with default plugin name.

### Lock Poisoning

```
Error: Registry lock poisoned
```

**Solution**: Plugin code panicked while holding registry lock. Fix the panic in your plugin implementation.

### Connection Timeout

```
Error: Connection timed out
```

**Solution**: Increase timeout in `extra_config` or check network connectivity.

---

## Complete Example: MockTransport

See full working example in `tests/mock_transport_plugin.rs`:

```rust
// tests/mock_transport_plugin.rs

use remotemedia_runtime_core::transport::*;
use async_trait::async_trait;

/// Mock transport that echoes data back (for testing)
pub struct MockTransportPlugin;

#[async_trait]
impl TransportPlugin for MockTransportPlugin {
    fn name(&self) -> &'static str { "mock" }

    async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        Ok(Box::new(MockClient { endpoint: config.endpoint.clone() }))
    }

    async fn create_server(
        &self,
        config: &ServerConfig,
        runner: Arc<PipelineRunner>,
    ) -> Result<Box<dyn PipelineTransport>> {
        Ok(Box::new(MockServer { runner }))
    }
}

struct MockClient { endpoint: String }

#[async_trait]
impl PipelineClient for MockClient {
    async fn execute_unary(&self, _manifest: Arc<Manifest>, input: TransportData) -> Result<TransportData> {
        Ok(input)  // Echo
    }

    async fn create_stream_session(&self, _manifest: Arc<Manifest>) -> Result<Box<dyn ClientStreamSession>> {
        Ok(Box::new(MockStreamSession))
    }

    async fn health_check(&self) -> Result<bool> { Ok(true) }
}

struct MockServer { runner: Arc<PipelineRunner> }

#[async_trait]
impl PipelineTransport for MockServer {
    async fn execute(&self, manifest: Arc<Manifest>, input: TransportData) -> Result<TransportData> {
        self.runner.execute_unary(manifest, input).await
    }

    async fn stream(&self, manifest: Arc<Manifest>) -> Result<Box<dyn StreamSession>> {
        self.runner.create_stream_session(manifest).await
    }
}

// Usage in tests
#[tokio::test]
async fn test_mock_transport() {
    let registry = global_registry();
    {
        let mut lock = registry.write().unwrap();
        lock.register(Arc::new(MockTransportPlugin)).unwrap();
    }

    // Now "mock" transport is available
    let lock = registry.read().unwrap();
    assert!(lock.get("mock").is_some());
}
```

---

## Next Steps

1. **Read Contracts**: Review detailed API specifications in [contracts/](./contracts/) directory
2. **Study Existing Transports**: Look at `remotemedia-grpc` and `remotemedia-webrtc` for real-world examples
3. **Implement Your Plugin**: Follow this guide to build your custom transport
4. **Write Tests**: Add unit and integration tests for your plugin
5. **Share**: Consider open-sourcing your transport plugin!

---

## References

- **Data Model**: [data-model.md](./data-model.md) - Entity definitions
- **Implementation Plan**: [plan.md](./plan.md) - Phased implementation strategy
- **Contracts**:
  - [transport-plugin.md](./contracts/transport-plugin.md) - TransportPlugin trait specification
  - [plugin-registry.md](./contracts/plugin-registry.md) - Registry API
  - [client-config.md](./contracts/client-config.md) - Config structures
- **Examples**:
  - `remotemedia-grpc/src/lib.rs` - gRPC transport plugin
  - `remotemedia-webrtc/src/lib.rs` - WebRTC transport plugin
  - `tests/mock_transport_plugin.rs` - Mock transport for testing
