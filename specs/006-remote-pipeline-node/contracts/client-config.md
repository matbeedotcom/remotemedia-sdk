# Contract: ClientConfig and ServerConfig

**Feature**: 006-remote-pipeline-node | **Date**: 2025-01-10
**Related**: [data-model.md](../data-model.md) | [transport-plugin.md](./transport-plugin.md)

## Overview

This contract defines the configuration structures used when creating transport clients and servers. Both structures follow a common pattern: common fields + transport-specific `extra_config` JSON.

`ClientConfig` and `ServerConfig` are transport-agnostic, allowing the same configuration pattern across all transports (gRPC, WebRTC, HTTP, custom).

---

## ClientConfig

### Definition

**Location**: `runtime-core/src/transport/client/mod.rs`

```rust
use serde::{Deserialize, Serialize};

/// Transport-agnostic client configuration
///
/// Passed to `TransportPlugin::create_client()` when initializing a client
/// for remote pipeline execution. Contains common fields (endpoint, auth) plus
/// transport-specific configuration in JSON format.
///
/// # Field Semantics
///
/// - **endpoint**: Connection string, format varies by transport
/// - **auth_token**: Optional authentication token (format varies by transport)
/// - **extra_config**: Transport-specific JSON configuration (validated by plugin)
///
/// # Thread Safety
///
/// This struct is `Clone` and `Send + Sync`, allowing safe sharing across threads.
///
/// # Example
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Endpoint URL/address
    ///
    /// Format varies by transport:
    /// - **gRPC**: "localhost:50051" or "grpc://example.com:50051"
    /// - **WebRTC**: "wss://signaling.example.com" (signaling server URL)
    /// - **HTTP**: "https://api.example.com/pipeline"
    ///
    /// # Validation
    ///
    /// - Must be non-empty
    /// - Format validated by transport plugin in create_client()
    /// - May include protocol prefix (e.g., "grpc://", "wss://", "https://")
    ///
    /// # Examples
    ///
    /// ```rust
    /// // gRPC
    /// endpoint: "localhost:50051"
    /// endpoint: "grpc://production.example.com:50051"
    ///
    /// // WebRTC
    /// endpoint: "wss://signaling.example.com/ws"
    ///
    /// // HTTP
    /// endpoint: "https://api.example.com/pipeline/execute"
    /// ```
    pub endpoint: String,

    /// Optional authentication token
    ///
    /// Format and usage varies by transport:
    /// - **gRPC**: Bearer token added to metadata (Authorization: Bearer <token>)
    /// - **WebRTC**: Token for signaling server authentication
    /// - **HTTP**: Bearer token in Authorization header
    ///
    /// # Security
    ///
    /// - Should be treated as sensitive data (not logged)
    /// - Transmitted securely (TLS/WSS recommended)
    /// - Consider using environment variables or secret management
    ///
    /// # Validation
    ///
    /// - If present, must be non-empty
    /// - Transport may reject if required but missing
    /// - Transport may accept but ignore if not supported
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Bearer token
    /// auth_token: Some("eyJhbGciOiJIUzI1NiIs...".to_string())
    ///
    /// // API key
    /// auth_token: Some("sk-1234567890abcdef".to_string())
    ///
    /// // No authentication
    /// auth_token: None
    /// ```
    pub auth_token: Option<String>,

    /// Transport-specific configuration (JSON)
    ///
    /// Each transport defines its own schema. Common patterns:
    /// - **gRPC**: Usually unused (all config in endpoint)
    /// - **WebRTC**: ICE servers, signaling config, timeouts
    /// - **HTTP**: Timeouts, retries, headers
    ///
    /// # Validation
    ///
    /// - Validated by `TransportPlugin::validate_config()`
    /// - Should fail fast if invalid (before client creation)
    /// - If None, transport uses default configuration
    ///
    /// # Type Safety
    ///
    /// Transports typically deserialize into a typed struct:
    ///
    /// ```rust
    /// #[derive(Deserialize)]
    /// struct WebRtcClientConfig {
    ///     ice_servers: Vec<IceServerConfig>,
    ///     signaling_timeout_ms: Option<u64>,
    /// }
    ///
    /// let typed: WebRtcClientConfig = serde_json::from_value(
    ///     config.extra_config.unwrap_or_default()
    /// )?;
    /// ```
    ///
    /// # Examples
    ///
    /// See "Transport-Specific Examples" section below.
    pub extra_config: Option<serde_json::Value>,
}
```

### Methods

```rust
impl ClientConfig {
    /// Create client config from manifest parameters
    ///
    /// Extracts configuration from RemotePipelineNode manifest params.
    /// This is the typical way ClientConfig is constructed.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Transport endpoint (required in manifest)
    /// * `auth_token` - Optional authentication token
    /// * `manifest_params` - Full manifest params (may contain transport_config)
    ///
    /// # Returns
    ///
    /// ClientConfig with extracted transport-specific configuration
    ///
    /// # Example
    ///
    /// ```rust
    /// // Manifest JSON:
    /// // {
    /// //   "endpoint": "localhost:50051",
    /// //   "auth_token": "token123",
    /// //   "transport_config": { "timeout_ms": 30000 }
    /// // }
    ///
    /// let params = serde_json::json!({
    ///     "endpoint": "localhost:50051",
    ///     "auth_token": "token123",
    ///     "transport_config": { "timeout_ms": 30000 }
    /// });
    ///
    /// let config = ClientConfig::from_manifest_params(
    ///     params["endpoint"].as_str().unwrap().to_string(),
    ///     params["auth_token"].as_str().map(|s| s.to_string()),
    ///     &params,
    /// );
    ///
    /// assert_eq!(config.endpoint, "localhost:50051");
    /// assert_eq!(config.auth_token, Some("token123".to_string()));
    /// assert!(config.extra_config.is_some());
    /// ```
    pub fn from_manifest_params(
        endpoint: String,
        auth_token: Option<String>,
        manifest_params: &serde_json::Value,
    ) -> Self {
        // Extract transport-specific config if present
        let extra_config = manifest_params
            .get("transport_config")
            .cloned();

        Self {
            endpoint,
            auth_token,
            extra_config,
        }
    }

    /// Validate basic configuration constraints
    ///
    /// Checks common validation rules (non-empty endpoint, etc.).
    /// Transport-specific validation done by plugin.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration passes basic validation
    /// * `Err(Error::ConfigError)` - Configuration invalid
    ///
    /// # Errors
    ///
    /// * Empty endpoint string
    /// * Empty auth_token (if Some)
    ///
    /// # Example
    ///
    /// ```rust
    /// let config = ClientConfig {
    ///     endpoint: "".to_string(),  // Invalid!
    ///     auth_token: None,
    ///     extra_config: None,
    /// };
    ///
    /// assert!(config.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<()> {
        if self.endpoint.is_empty() {
            return Err(Error::ConfigError("endpoint cannot be empty".to_string()));
        }

        if let Some(ref token) = self.auth_token {
            if token.is_empty() {
                return Err(Error::ConfigError("auth_token cannot be empty".to_string()));
            }
        }

        Ok(())
    }
}
```

### Custom Debug (Redacts Sensitive Data)

```rust
impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("endpoint", &self.endpoint)
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***REDACTED***"))
            .field("extra_config", &self.extra_config.as_ref().map(|_| "***REDACTED***"))
            .finish()
    }
}
```

---

## ServerConfig

### Definition

**Location**: `runtime-core/src/transport/server.rs` (new file)

```rust
use serde::{Deserialize, Serialize};

/// Transport-agnostic server configuration
///
/// Passed to `TransportPlugin::create_server()` when initializing a server
/// for handling remote pipeline execution requests.
///
/// # Field Semantics
///
/// - **bind_addr**: Socket address to bind (IP:port)
/// - **auth_config**: Optional authentication/authorization settings
/// - **extra_config**: Transport-specific JSON configuration
///
/// # Example
///
/// ```rust
/// let config = ServerConfig {
///     bind_addr: "0.0.0.0:50051".to_string(),
///     auth_config: Some(AuthConfig::BearerToken {
///         valid_tokens: vec!["token1".to_string()],
///     }),
///     extra_config: None,
/// };
///
/// let runner = Arc::new(PipelineRunner::new(registry));
/// let plugin = GrpcTransportPlugin;
/// let server = plugin.create_server(&config, runner).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind address
    ///
    /// Socket address to bind the server listener.
    ///
    /// # Format
    ///
    /// - **IPv4**: "0.0.0.0:50051", "127.0.0.1:8080"
    /// - **IPv6**: "[::]:50051", "[::1]:8080"
    ///
    /// # Special Addresses
    ///
    /// - "0.0.0.0" - Bind to all IPv4 interfaces
    /// - "[::]" - Bind to all IPv6 interfaces
    /// - "127.0.0.1" / "[::1]" - Localhost only
    ///
    /// # Validation
    ///
    /// - Must be valid IP:port format
    /// - Port must be available (not in use)
    /// - May require elevated privileges for ports < 1024 (Unix)
    ///
    /// # Examples
    ///
    /// ```rust
    /// // All interfaces, port 50051
    /// bind_addr: "0.0.0.0:50051"
    ///
    /// // Localhost only, port 8080
    /// bind_addr: "127.0.0.1:8080"
    ///
    /// // IPv6, all interfaces
    /// bind_addr: "[::]:50051"
    /// ```
    pub bind_addr: String,

    /// Optional authentication configuration
    ///
    /// Defines how the server validates incoming requests.
    /// If None, server accepts unauthenticated requests (not recommended for production).
    ///
    /// # Validation
    ///
    /// - Checked during server creation
    /// - Invalid config causes create_server() to fail
    ///
    /// # Example
    ///
    /// ```rust
    /// // Bearer token authentication
    /// auth_config: Some(AuthConfig::BearerToken {
    ///     valid_tokens: vec!["token1".to_string(), "token2".to_string()],
    /// })
    ///
    /// // API key authentication
    /// auth_config: Some(AuthConfig::ApiKey {
    ///     header_name: "X-API-Key".to_string(),
    ///     valid_keys: vec!["key1".to_string()],
    /// })
    ///
    /// // No authentication (insecure)
    /// auth_config: None
    /// ```
    pub auth_config: Option<AuthConfig>,

    /// Transport-specific configuration (JSON)
    ///
    /// Each transport defines its own schema. Common patterns:
    /// - **gRPC**: Max concurrent streams, keepalive settings, message size limits
    /// - **WebRTC**: ICE servers, max peers, connection timeouts
    /// - **HTTP**: Max body size, request timeout, CORS settings
    ///
    /// # Validation
    ///
    /// - Validated by `TransportPlugin::validate_config()`
    /// - Should fail fast if invalid (before server creation)
    /// - If None, transport uses default configuration
    ///
    /// # Examples
    ///
    /// See "Transport-Specific Examples" section below.
    pub extra_config: Option<serde_json::Value>,
}

/// Authentication configuration for server
///
/// Defines how the server validates incoming requests.
/// Different transports may support different auth types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthConfig {
    /// No authentication (accept all requests)
    ///
    /// **Warning**: Only use for development/testing. Not secure for production.
    None,

    /// Bearer token authentication
    ///
    /// Validates "Authorization: Bearer <token>" header (HTTP/gRPC) or
    /// equivalent token in signaling messages (WebRTC).
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "BearerToken",
    ///   "valid_tokens": ["secret-token-1", "secret-token-2"]
    /// }
    /// ```
    BearerToken {
        /// List of valid bearer tokens
        ///
        /// Requests are accepted if Authorization header matches any token.
        /// Tokens are compared exactly (case-sensitive, no trimming).
        valid_tokens: Vec<String>,
    },

    /// API key authentication (header-based)
    ///
    /// Validates a custom header (e.g., "X-API-Key") against a list of valid keys.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "ApiKey",
    ///   "header_name": "X-API-Key",
    ///   "valid_keys": ["key-abc123", "key-def456"]
    /// }
    /// ```
    ApiKey {
        /// Header name to check (e.g., "X-API-Key")
        header_name: String,

        /// List of valid API keys
        valid_keys: Vec<String>,
    },

    /// Custom authentication (transport-specific)
    ///
    /// Allows transports to implement custom auth logic.
    /// The config JSON is interpreted by the transport plugin.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "type": "Custom",
    ///   "config": {
    ///     "jwt_secret": "my-secret-key",
    ///     "jwt_algorithm": "HS256"
    ///   }
    /// }
    /// ```
    Custom {
        /// Transport-specific authentication config
        config: serde_json::Value,
    },
}
```

---

## Transport-Specific Examples

### gRPC

#### Client Config (Minimal)

```json
{
  "endpoint": "localhost:50051",
  "auth_token": "grpc-bearer-token"
}
```

#### Server Config

```json
{
  "bind_addr": "0.0.0.0:50051",
  "auth_config": {
    "type": "BearerToken",
    "valid_tokens": ["token1", "token2"]
  },
  "extra_config": {
    "max_concurrent_streams": 100,
    "keepalive_interval_ms": 60000,
    "keepalive_timeout_ms": 20000,
    "max_message_size": 10485760
  }
}
```

### WebRTC

#### Client Config

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
    "signaling_timeout_ms": 5000,
    "ice_gathering_timeout_ms": 3000
  }
}
```

#### Server Config

```json
{
  "bind_addr": "0.0.0.0:8080",
  "auth_config": null,
  "extra_config": {
    "ice_servers": [
      { "urls": "stun:stun.l.google.com:19302" }
    ],
    "max_peers": 50,
    "peer_connection_timeout_ms": 30000,
    "signaling_path": "/ws"
  }
}
```

### HTTP

#### Client Config

```json
{
  "endpoint": "https://api.example.com/pipeline",
  "auth_token": "Bearer xyz123",
  "extra_config": {
    "timeout_ms": 30000,
    "max_retries": 3,
    "retry_backoff_ms": 1000,
    "headers": {
      "X-Custom-Header": "value"
    }
  }
}
```

#### Server Config

```json
{
  "bind_addr": "0.0.0.0:8000",
  "auth_config": {
    "type": "ApiKey",
    "header_name": "X-API-Key",
    "valid_keys": ["key-abc123"]
  },
  "extra_config": {
    "max_body_size": 10485760,
    "request_timeout_ms": 30000,
    "cors": {
      "allowed_origins": ["https://example.com"],
      "allowed_methods": ["POST"]
    }
  }
}
```

---

## Validation Rules

### Common Validation (All Transports)

| Field | Rule | Error |
|-------|------|-------|
| `endpoint` | Non-empty | "endpoint cannot be empty" |
| `auth_token` | Non-empty if Some | "auth_token cannot be empty" |
| `bind_addr` | Valid IP:port | "Invalid bind address: ..." |
| `extra_config` | Valid JSON | "Invalid JSON in extra_config" |

### Transport-Specific Validation

Performed by `TransportPlugin::validate_config()`:

**WebRTC**:
- `ice_servers` must be present and non-empty
- Each ICE server must have valid `urls` field
- TURN servers must have `username` and `credential`

**HTTP**:
- `timeout_ms` must be positive integer
- `max_retries` must be non-negative integer
- `max_body_size` must be positive integer

**gRPC**:
- No required extra_config fields (all optional)

---

## Security Considerations

### Sensitive Data Handling

```rust
// Custom Debug impl redacts sensitive fields
impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("endpoint", &self.endpoint)
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***REDACTED***"))
            .field("extra_config", &"***REDACTED***")
            .finish()
    }
}

// Output:
// ClientConfig { endpoint: "localhost:50051", auth_token: Some("***REDACTED***"), extra_config: "***REDACTED***" }
```

### Best Practices

1. **Never log auth tokens or credentials**
2. **Use TLS/WSS for all production deployments**
3. **Store tokens in environment variables or secret management**
4. **Rotate tokens regularly**
5. **Use least-privilege authentication (don't use root tokens)**
6. **Consider implementing token expiration**
7. **Audit authentication failures**

---

## Testing

### Unit Tests

```rust
#[test]
fn test_client_config_validation() {
    // Valid config
    let valid = ClientConfig {
        endpoint: "localhost:50051".to_string(),
        auth_token: Some("token".to_string()),
        extra_config: None,
    };
    assert!(valid.validate().is_ok());

    // Empty endpoint
    let empty_endpoint = ClientConfig {
        endpoint: "".to_string(),
        auth_token: None,
        extra_config: None,
    };
    assert!(empty_endpoint.validate().is_err());

    // Empty auth token
    let empty_token = ClientConfig {
        endpoint: "localhost:50051".to_string(),
        auth_token: Some("".to_string()),
        extra_config: None,
    };
    assert!(empty_token.validate().is_err());
}

#[test]
fn test_from_manifest_params() {
    let params = serde_json::json!({
        "endpoint": "localhost:50051",
        "auth_token": "token123",
        "transport_config": { "timeout_ms": 30000 }
    });

    let config = ClientConfig::from_manifest_params(
        "localhost:50051".to_string(),
        Some("token123".to_string()),
        &params,
    );

    assert_eq!(config.endpoint, "localhost:50051");
    assert_eq!(config.auth_token, Some("token123".to_string()));
    assert!(config.extra_config.is_some());
}

#[test]
fn test_debug_redacts_sensitive_data() {
    let config = ClientConfig {
        endpoint: "localhost:50051".to_string(),
        auth_token: Some("secret-token".to_string()),
        extra_config: Some(serde_json::json!({"key": "value"})),
    };

    let debug_str = format!("{:?}", config);
    assert!(!debug_str.contains("secret-token"));
    assert!(debug_str.contains("***REDACTED***"));
}
```

---

## References

- **Data Model**: [data-model.md](../data-model.md) - ClientConfig and ServerConfig entities
- **Plugin Contract**: [transport-plugin.md](./transport-plugin.md) - TransportPlugin trait
- **Registry Contract**: [plugin-registry.md](./plugin-registry.md) - Plugin registration
- **Implementation Plan**: [plan.md](../plan.md) - Phased implementation strategy
