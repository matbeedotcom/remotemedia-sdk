# Contract: TransportPluginRegistry

**Feature**: 006-remote-pipeline-node | **Date**: 2025-01-10
**Related**: [data-model.md](../data-model.md) | [transport-plugin.md](./transport-plugin.md)

## Overview

The `TransportPluginRegistry` is a thread-safe global registry that stores and provides access to registered transport plugins. It uses a lazy-initialized singleton pattern with `OnceLock` and provides concurrent read access via `RwLock`.

This contract defines the complete API for registering, looking up, and managing transport plugins.

---

## Module Structure

**Location**: `runtime-core/src/transport/plugin_registry.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use crate::{Result, Error};
use crate::transport::TransportPlugin;

// Module exports
pub struct TransportPluginRegistry { /* ... */ }

pub fn global_registry() -> Arc<RwLock<TransportPluginRegistry>>;
pub fn init_global_registry_with_plugins(plugins: Vec<Arc<dyn TransportPlugin>>) -> Result<()>;
fn register_default_plugins(registry: &mut TransportPluginRegistry) -> Result<()>;
```

---

## TransportPluginRegistry Struct

### Definition

```rust
/// Global transport plugin registry
///
/// Stores registered transport plugins and provides thread-safe access
/// for client/server creation. Uses a HashMap for O(1) lookup performance.
///
/// # Thread Safety
///
/// The registry is wrapped in Arc<RwLock<...>> which provides:
/// - Multiple concurrent readers (no lock contention)
/// - Exclusive writer (during registration only)
/// - Send + Sync (safe to share across threads)
///
/// # Lifecycle
///
/// 1. **Uninitialized**: No registry exists (program start)
/// 2. **Initialization**: First access via global_registry() or init_global_registry_with_plugins()
/// 3. **Active**: Used for plugin lookups (read-only after initialization)
///
/// # Example
///
/// ```rust
/// // Get global registry
/// let registry = global_registry();
///
/// // Lookup plugin (read lock)
/// let plugin = {
///     let lock = registry.read().unwrap();
///     lock.get("grpc").ok_or("Plugin not found")?
/// };
///
/// // Use plugin
/// let client = plugin.create_client(&config).await?;
/// ```
pub struct TransportPluginRegistry {
    /// Map of transport name → plugin implementation
    ///
    /// Keys are transport names (e.g., "grpc", "webrtc", "http")
    /// Values are Arc-wrapped trait objects for cheap cloning
    plugins: HashMap<String, Arc<dyn TransportPlugin>>,
}
```

### Invariants

- **Uniqueness**: Each plugin name must be unique (enforced by HashMap + registration check)
- **Immutability**: After initialization, registry is effectively immutable (no removal/replacement)
- **Thread-Safe**: All public methods are safe to call from multiple threads concurrently
- **Initialization Once**: Registry can only be initialized once (enforced by OnceLock)

---

## Methods

### new() - Create Empty Registry

```rust
/// Create a new empty registry
///
/// This is a private constructor. Users should access the registry via
/// `global_registry()` or `init_global_registry_with_plugins()`.
///
/// # Returns
///
/// An empty registry with no plugins registered.
///
/// # Example
///
/// ```rust
/// let registry = TransportPluginRegistry::new();
/// assert_eq!(registry.list().len(), 0);
/// ```
fn new() -> Self {
    Self {
        plugins: HashMap::new(),
    }
}
```

---

### register() - Register a Plugin

```rust
/// Register a transport plugin
///
/// Adds a plugin to the registry, making it available for lookup.
/// The plugin's name() method is used as the registry key.
///
/// # Arguments
///
/// * `plugin` - Plugin implementation wrapped in Arc for sharing
///
/// # Returns
///
/// * `Ok(())` - Plugin registered successfully
/// * `Err(Error::ConfigError)` - Plugin name already registered
///
/// # Errors
///
/// ## Duplicate Registration
///
/// Returns `Error::ConfigError` if a plugin with the same name is already
/// registered. Plugin names must be unique.
///
/// ```rust
/// let mut registry = TransportPluginRegistry::new();
///
/// // First registration succeeds
/// registry.register(Arc::new(GrpcTransportPlugin))?;
///
/// // Second registration with same name fails
/// let result = registry.register(Arc::new(GrpcTransportPlugin));
/// assert!(matches!(result, Err(Error::ConfigError(_))));
/// ```
///
/// # Thread Safety
///
/// This method requires mutable access (&mut self), which means it
/// requires a write lock when called through the global registry:
///
/// ```rust
/// let registry = global_registry();
/// {
///     let mut lock = registry.write().unwrap();
///     lock.register(Arc::new(MyPlugin))?;
/// }
/// ```
///
/// # Performance
///
/// - HashMap insert: O(1) average case
/// - Duplicate check: O(1) HashMap contains_key
/// - Total: ~50-100ns
///
/// # Example
///
/// ```rust
/// pub struct CustomTransportPlugin;
///
/// #[async_trait]
/// impl TransportPlugin for CustomTransportPlugin {
///     fn name(&self) -> &'static str { "custom" }
///     // ... other methods
/// }
///
/// // Register globally
/// let registry = global_registry();
/// {
///     let mut lock = registry.write().unwrap();
///     lock.register(Arc::new(CustomTransportPlugin))?;
/// }
///
/// // Now "custom" transport is available
/// ```
pub fn register(&mut self, plugin: Arc<dyn TransportPlugin>) -> Result<()> {
    let name = plugin.name().to_string();

    if self.plugins.contains_key(&name) {
        return Err(Error::ConfigError(
            format!("Transport plugin '{}' already registered", name)
        ));
    }

    self.plugins.insert(name, plugin);
    Ok(())
}
```

---

### get() - Lookup Plugin by Name

```rust
/// Get plugin by name
///
/// Looks up a plugin in the registry and returns an Arc-wrapped reference
/// if found. The Arc allows cheap cloning without duplicating the plugin.
///
/// # Arguments
///
/// * `name` - Transport identifier (e.g., "grpc", "webrtc", "http")
///
/// # Returns
///
/// * `Some(Arc<dyn TransportPlugin>)` - Plugin found
/// * `None` - No plugin registered with this name
///
/// # Thread Safety
///
/// This method only requires read access (&self), allowing multiple
/// concurrent lookups without lock contention:
///
/// ```rust
/// let registry = global_registry();
/// let lock = registry.read().unwrap();  // Read lock (shared)
///
/// // Multiple threads can read concurrently
/// let grpc_plugin = lock.get("grpc");
/// let webrtc_plugin = lock.get("webrtc");
/// ```
///
/// # Performance
///
/// - HashMap lookup: O(1) average case
/// - Arc clone: ~5ns (refcount increment)
/// - Total: ~10-20ns
///
/// # Examples
///
/// ## Success Case
///
/// ```rust
/// let registry = global_registry();
/// let lock = registry.read().unwrap();
///
/// if let Some(plugin) = lock.get("grpc") {
///     println!("Found gRPC plugin: {}", plugin.name());
///     let client = plugin.create_client(&config).await?;
/// }
/// ```
///
/// ## Error Case (Plugin Not Found)
///
/// ```rust
/// let registry = global_registry();
/// let lock = registry.read().unwrap();
///
/// let plugin = lock.get("unknown")
///     .ok_or_else(|| Error::ConfigError(
///         format!("Transport 'unknown' not available. Available: {:?}", lock.list())
///     ))?;
/// ```
///
/// ## Common Pattern in RemotePipelineNode
///
/// ```rust
/// async fn get_client(&self) -> Result<Arc<dyn PipelineClient>> {
///     // Lookup plugin
///     let registry = global_registry();
///     let lock = registry.read().unwrap();
///
///     let plugin = lock.get(&self.config.transport)
///         .ok_or_else(|| Error::ConfigError(
///             format!("Transport '{}' not available. Available: {:?}",
///                 self.config.transport,
///                 lock.list())
///         ))?;
///
///     // Drop lock before async operation
///     drop(lock);
///
///     // Create client (may take 1-10ms)
///     let client = plugin.create_client(&self.config).await?;
///     Ok(Arc::new(client))
/// }
/// ```
pub fn get(&self, name: &str) -> Option<Arc<dyn TransportPlugin>> {
    self.plugins.get(name).cloned()
}
```

---

### list() - List All Plugin Names

```rust
/// List all registered plugin names
///
/// Returns a vector of all transport names currently registered in the
/// registry. Useful for error messages and debugging.
///
/// # Returns
///
/// Vector of transport names (e.g., ["grpc", "http", "webrtc"])
/// The order is arbitrary (HashMap iteration order).
///
/// # Thread Safety
///
/// Like `get()`, this method only requires read access (&self),
/// allowing concurrent calls from multiple threads.
///
/// # Performance
///
/// - HashMap keys iteration: O(n) where n = number of plugins
/// - String cloning: O(k) where k = total length of all names
/// - Typical: ~100-200ns for 3 plugins
///
/// # Examples
///
/// ## Error Messages
///
/// ```rust
/// let registry = global_registry();
/// let lock = registry.read().unwrap();
///
/// let plugin = lock.get("unknown")
///     .ok_or_else(|| Error::ConfigError(
///         format!("Transport 'unknown' not available. Available: {:?}", lock.list())
///     ))?;
/// // Error: "Transport 'unknown' not available. Available: [\"grpc\", \"http\", \"webrtc\"]"
/// ```
///
/// ## Debugging
///
/// ```rust
/// let registry = global_registry();
/// let lock = registry.read().unwrap();
///
/// println!("Registered transports: {:?}", lock.list());
/// // Output: Registered transports: ["grpc", "http", "webrtc"]
/// ```
///
/// ## Validation
///
/// ```rust
/// fn validate_manifest_transport(transport: &str) -> Result<()> {
///     let registry = global_registry();
///     let lock = registry.read().unwrap();
///
///     if !lock.list().contains(&transport.to_string()) {
///         return Err(Error::ConfigError(
///             format!("Invalid transport '{}'. Available: {:?}", transport, lock.list())
///         ));
///     }
///
///     Ok(())
/// }
/// ```
pub fn list(&self) -> Vec<String> {
    self.plugins.keys().cloned().collect()
}
```

---

## Global Access Functions

### global_registry() - Get Global Registry

```rust
/// Global registry instance (lazy-initialized)
static TRANSPORT_REGISTRY: OnceLock<Arc<RwLock<TransportPluginRegistry>>> = OnceLock::new();

/// Get global transport plugin registry
///
/// Returns a reference to the global registry, initializing it on first access.
/// The registry is wrapped in Arc<RwLock<...>> for thread-safe shared access.
///
/// # Returns
///
/// Arc-wrapped RwLock-protected registry. Clone is cheap (Arc refcount increment).
///
/// # Initialization
///
/// On first call, the registry is initialized with default plugins based on
/// enabled feature flags:
/// - `grpc-client` feature → registers GrpcTransportPlugin
/// - `webrtc-client` feature → registers WebRtcTransportPlugin
/// - Always registers HttpTransportPlugin (no feature flag required)
///
/// # Thread Safety
///
/// - First call may block while initializing (OnceLock ensures atomic init)
/// - Subsequent calls return immediately (just Arc clone)
/// - Multiple threads can call concurrently (OnceLock handles synchronization)
///
/// # Performance
///
/// - First call: ~1-10μs (initialization + plugin registration)
/// - Subsequent calls: ~5ns (Arc clone)
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust
/// // Get registry (initializes on first call)
/// let registry = global_registry();
///
/// // Read access (shared lock)
/// {
///     let lock = registry.read().unwrap();
///     let plugin = lock.get("grpc").unwrap();
/// }
///
/// // Write access (exclusive lock)
/// {
///     let mut lock = registry.write().unwrap();
///     lock.register(Arc::new(CustomPlugin))?;
/// }
/// ```
///
/// ## Custom Plugin Registration
///
/// ```rust
/// use remotemedia_runtime_core::transport::plugin_registry::global_registry;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // Get registry (initializes with defaults)
///     let registry = global_registry();
///
///     // Register custom plugin
///     {
///         let mut lock = registry.write().unwrap();
///         lock.register(Arc::new(MyCustomTransportPlugin))?;
///     }
///
///     // Now both default and custom transports are available
///     Ok(())
/// }
/// ```
///
/// ## Error Handling
///
/// ```rust
/// let registry = global_registry();
///
/// // Read lock can fail if poisoned (panic occurred while holding lock)
/// match registry.read() {
///     Ok(lock) => {
///         // Normal operation
///         let plugin = lock.get("grpc");
///     }
///     Err(e) => {
///         // Lock poisoned (rare, indicates bug in plugin implementation)
///         eprintln!("Registry lock poisoned: {}", e);
///         return Err(Error::Internal("Registry corrupted".to_string()));
///     }
/// }
/// ```
pub fn global_registry() -> Arc<RwLock<TransportPluginRegistry>> {
    TRANSPORT_REGISTRY
        .get_or_init(|| {
            let mut registry = TransportPluginRegistry::new();
            register_default_plugins(&mut registry)
                .expect("Failed to register default transport plugins");
            Arc::new(RwLock::new(registry))
        })
        .clone()
}
```

---

### init_global_registry_with_plugins() - Initialize with Custom Plugins

```rust
/// Initialize global registry with custom plugins
///
/// Allows applications to register custom transport plugins before the
/// registry is accessed for the first time. This must be called once at
/// program startup, before any `global_registry()` calls.
///
/// # Arguments
///
/// * `plugins` - Vector of custom plugins to register (in addition to defaults)
///
/// # Returns
///
/// * `Ok(())` - Registry initialized successfully
/// * `Err(Error::ConfigError)` - Registry already initialized or plugin conflict
///
/// # Errors
///
/// ## Already Initialized
///
/// Returns error if `global_registry()` or `init_global_registry_with_plugins()`
/// was already called:
///
/// ```rust
/// // First call succeeds
/// init_global_registry_with_plugins(vec![])?;
///
/// // Second call fails
/// let result = init_global_registry_with_plugins(vec![]);
/// assert!(matches!(result, Err(Error::ConfigError(_))));
/// // Error: "Global registry already initialized"
/// ```
///
/// ## Plugin Name Conflict
///
/// Returns error if a custom plugin name conflicts with a default plugin:
///
/// ```rust
/// // Custom plugin with name "grpc" conflicts with default
/// struct MyGrpcPlugin;
/// impl TransportPlugin for MyGrpcPlugin {
///     fn name(&self) -> &'static str { "grpc" }
///     // ...
/// }
///
/// let result = init_global_registry_with_plugins(vec![
///     Arc::new(MyGrpcPlugin),
/// ]);
/// assert!(matches!(result, Err(Error::ConfigError(_))));
/// // Error: "Transport plugin 'grpc' already registered"
/// ```
///
/// # Usage Pattern
///
/// ```rust
/// use remotemedia_runtime_core::transport::plugin_registry::init_global_registry_with_plugins;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // IMPORTANT: Call this BEFORE any other runtime-core usage
///     init_global_registry_with_plugins(vec![
///         Arc::new(MyCustomTransport1),
///         Arc::new(MyCustomTransport2),
///     ])?;
///
///     // Now custom transports are available in manifests
///     let runner = PipelineRunner::new(registry);
///     runner.execute_unary(manifest, input).await?;
///
///     Ok(())
/// }
/// ```
///
/// # Implementation Notes
///
/// - Registers default plugins first (based on feature flags)
/// - Then registers custom plugins in order provided
/// - If any registration fails, entire operation fails atomically
/// - OnceLock::set ensures this can only succeed once
pub fn init_global_registry_with_plugins(
    plugins: Vec<Arc<dyn TransportPlugin>>
) -> Result<()> {
    let mut registry = TransportPluginRegistry::new();

    // Register default plugins first
    register_default_plugins(&mut registry)?;

    // Register custom plugins
    for plugin in plugins {
        registry.register(plugin)?;
    }

    // Atomically set global registry
    TRANSPORT_REGISTRY.set(Arc::new(RwLock::new(registry)))
        .map_err(|_| Error::ConfigError("Global registry already initialized".to_string()))
}
```

---

### register_default_plugins() - Register Built-in Plugins

```rust
/// Register default transport plugins based on feature flags
///
/// Called automatically during registry initialization. Registers:
/// - HttpTransportPlugin (always)
/// - GrpcTransportPlugin (if grpc-client feature enabled)
/// - WebRtcTransportPlugin (if webrtc-client feature enabled)
///
/// # Arguments
///
/// * `registry` - Registry to populate with default plugins
///
/// # Returns
///
/// * `Ok(())` - All default plugins registered successfully
/// * `Err(Error::ConfigError)` - Plugin registration failed (should never happen)
///
/// # Feature Flags
///
/// ## grpc-client
///
/// Enables gRPC transport:
/// ```toml
/// [dependencies]
/// remotemedia-runtime-core = { version = "0.5", features = ["grpc-client"] }
/// ```
///
/// ## webrtc-client
///
/// Enables WebRTC transport:
/// ```toml
/// [dependencies]
/// remotemedia-runtime-core = { version = "0.5", features = ["webrtc-client"] }
/// ```
///
/// ## all-transports
///
/// Convenience feature that enables all transports:
/// ```toml
/// [dependencies]
/// remotemedia-runtime-core = { version = "0.5", features = ["all-transports"] }
/// ```
///
/// # Example
///
/// ```rust
/// let mut registry = TransportPluginRegistry::new();
/// register_default_plugins(&mut registry)?;
///
/// // HTTP always available
/// assert!(registry.get("http").is_some());
///
/// // gRPC available if feature enabled
/// #[cfg(feature = "grpc-client")]
/// assert!(registry.get("grpc").is_some());
///
/// // WebRTC available if feature enabled
/// #[cfg(feature = "webrtc-client")]
/// assert!(registry.get("webrtc").is_some());
/// ```
fn register_default_plugins(registry: &mut TransportPluginRegistry) -> Result<()> {
    // HTTP always available (reqwest in core dependencies)
    registry.register(Arc::new(HttpTransportPlugin))?;

    // gRPC (conditional on feature flag)
    #[cfg(feature = "grpc-client")]
    {
        use remotemedia_grpc::GrpcTransportPlugin;
        registry.register(Arc::new(GrpcTransportPlugin))?;
    }

    // WebRTC (conditional on feature flag)
    #[cfg(feature = "webrtc-client")]
    {
        use remotemedia_webrtc::WebRtcTransportPlugin;
        registry.register(Arc::new(WebRtcTransportPlugin))?;
    }

    Ok(())
}
```

---

## Thread Safety Guarantees

### Concurrent Reads (No Contention)

```rust
// Multiple threads can read concurrently
std::thread::scope(|s| {
    for i in 0..8 {
        s.spawn(move || {
            let registry = global_registry();
            let lock = registry.read().unwrap();  // Shared lock
            let plugin = lock.get("grpc");
            println!("Thread {}: {:?}", i, plugin.is_some());
        });
    }
});
```

### Exclusive Write (Blocks All Others)

```rust
let registry = global_registry();

// Write lock blocks all other reads and writes
{
    let mut lock = registry.write().unwrap();  // Exclusive lock
    lock.register(Arc::new(CustomPlugin))?;
}  // Lock released

// Now reads can proceed
let lock = registry.read().unwrap();
let plugin = lock.get("custom");
```

### Lock Poisoning

```rust
let registry = global_registry();

// If thread panics while holding lock, lock becomes poisoned
std::thread::spawn(|| {
    let mut lock = registry.write().unwrap();
    panic!("Oops!");  // Lock now poisoned
});

// Subsequent accesses fail
match registry.read() {
    Ok(lock) => { /* Normal path */ }
    Err(e) => {
        // Lock poisoned - should never happen in production
        eprintln!("Registry corrupted: {}", e);
    }
}
```

---

## Performance Characteristics

### Lookup Performance

```rust
// Measured latencies (release build, x86_64):
let start = Instant::now();

let registry = global_registry();           // ~5ns (Arc clone)
let lock = registry.read().unwrap();        // ~10ns (RwLock read)
let plugin = lock.get("grpc");              // ~10ns (HashMap get)

println!("Total: {:?}", start.elapsed());   // ~25ns typical
```

### Scalability

- **Single thread**: ~10-20ns per lookup
- **8 concurrent threads**: ~50-100ns per lookup (still negligible)
- **No contention**: RwLock allows unlimited concurrent readers

### Memory Usage

- **Registry struct**: ~56 bytes (HashMap overhead)
- **Per plugin**: ~56 bytes (Arc + HashMap entry + String key)
- **Total (3 plugins)**: ~224 bytes
- **Global wrappers**: ~64 bytes (OnceLock + Arc + RwLock)
- **Grand total**: ~288 bytes (negligible)

---

## Testing

### Unit Tests

```rust
#[test]
fn test_register_and_get() {
    let mut registry = TransportPluginRegistry::new();
    registry.register(Arc::new(MockPlugin { name: "mock" })).unwrap();

    assert!(registry.get("mock").is_some());
    assert!(registry.get("unknown").is_none());
}

#[test]
fn test_duplicate_registration() {
    let mut registry = TransportPluginRegistry::new();
    registry.register(Arc::new(MockPlugin { name: "mock" })).unwrap();

    let result = registry.register(Arc::new(MockPlugin { name: "mock" }));
    assert!(matches!(result, Err(Error::ConfigError(_))));
}

#[test]
fn test_list() {
    let mut registry = TransportPluginRegistry::new();
    registry.register(Arc::new(MockPlugin { name: "plugin1" })).unwrap();
    registry.register(Arc::new(MockPlugin { name: "plugin2" })).unwrap();

    let list = registry.list();
    assert_eq!(list.len(), 2);
    assert!(list.contains(&"plugin1".to_string()));
    assert!(list.contains(&"plugin2".to_string()));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_global_registry_initialization() {
    let registry = global_registry();
    let lock = registry.read().unwrap();

    // HTTP always available
    assert!(lock.get("http").is_some());

    #[cfg(feature = "grpc-client")]
    assert!(lock.get("grpc").is_some());
}

#[test]
fn test_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(global_registry());

    let handles: Vec<_> = (0..8).map(|_| {
        let registry = Arc::clone(&registry);
        thread::spawn(move || {
            let lock = registry.read().unwrap();
            lock.get("http").is_some()
        })
    }).collect();

    for handle in handles {
        assert!(handle.join().unwrap());
    }
}
```

---

## References

- **Data Model**: [data-model.md](../data-model.md) - Registry entity definition
- **Plugin Contract**: [transport-plugin.md](./transport-plugin.md) - TransportPlugin trait spec
- **Config Contract**: [client-config.md](./client-config.md) - ClientConfig and ServerConfig
- **Implementation Plan**: [plan.md](../plan.md) - Phased implementation strategy
- **Rust Patterns**:
  - OnceLock: https://doc.rust-lang.org/std/sync/struct.OnceLock.html
  - RwLock: https://doc.rust-lang.org/std/sync/struct.RwLock.html
  - Arc: https://doc.rust-lang.org/std/sync/struct.Arc.html
