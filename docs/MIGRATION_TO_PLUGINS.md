# Migration Guide: Transport Plugin System

## Overview

RemoteMedia SDK v0.5.0 introduces a plugin-based transport system that replaces the hardcoded transport factory pattern. This guide helps you migrate existing code.

## Why Change?

**Before** (v0.4.x):
- Transport clients hardcoded in runtime-core
- Adding custom transports required modifying core
- Tight coupling between core and transport implementations

**After** (v0.5.0+):
- Transport plugins are self-contained in separate crates
- Custom transports can be added without modifying core
- Clean separation of concerns

## Breaking Changes

### Deprecated APIs

- `create_transport_client()` - Use plugin registry instead
- Hardcoded client modules in `runtime-core/src/transport/client/{grpc,webrtc}.rs` - Moved to transport crates

### Migration Steps

#### 1. Register Transport Plugins at Startup

**Old code** (implicit, transports always available):
```rust
// No registration needed - transports were hardcoded
```

**New code** (explicit registration):
```rust
use remotemedia_runtime_core::transport::plugin_registry::global_registry;
use std::sync::Arc;

// Register at application startup
fn init_transports() -> Result<()> {
    let registry = global_registry();

    #[cfg(feature = "grpc-client")]
    {
        use remotemedia_grpc::GrpcTransportPlugin;
        registry.register(Arc::new(GrpcTransportPlugin))?;
    }

    #[cfg(feature = "webrtc")]
    {
        use remotemedia_webrtc::WebRtcTransportPlugin;
        registry.register(Arc::new(WebRtcTransportPlugin))?;
    }

    #[cfg(feature = "http-client")]
    {
        use remotemedia_http::HttpTransportPlugin;
        registry.register(Arc::new(HttpTransportPlugin))?;
    }

    Ok(())
}
```

#### 2. Update Client Creation

**Old code**:
```rust
use remotemedia_runtime_core::transport::client::{create_transport_client, TransportConfig};

let config = TransportConfig {
    transport_type: TransportType::Grpc,
    endpoint: "localhost:50051".to_string(),
    auth_token: None,
    extra_config: None,
};
let client = create_transport_client(config).await?;
```

**New code**:
```rust
use remotemedia_runtime_core::transport::{plugin_registry::global_registry, ClientConfig};

let config = ClientConfig {
    address: "localhost:50051".to_string(),
    auth_token: None,
    timeout_ms: None,
    extra_config: None,
};
let plugin = global_registry().get("grpc")
    .expect("grpc transport not registered");
let client = plugin.create_client(&config).await?;
```

#### 3. Update Manifest Configuration

Manifests remain **backward compatible** - no changes needed!

```yaml
# Still works in v0.5.0+
nodes:
  - id: remote_node
    node_type: RemotePipelineNode
    params:
      transport: "grpc"
      endpoint: "localhost:50051"
```

## Custom Transport Implementation

See `specs/006-remote-pipeline-node/quickstart.md` for complete guide.

### Quick Example

```rust
use remotemedia_runtime_core::transport::{TransportPlugin, ClientConfig};
use async_trait::async_trait;

pub struct MyCustomTransportPlugin;

#[async_trait]
impl TransportPlugin for MyCustomTransportPlugin {
    fn name(&self) -> &str {
        "my-custom-transport"
    }

    async fn create_client(&self, config: &ClientConfig) -> Result<Box<dyn PipelineClient>> {
        // Your implementation here
        Ok(Box::new(MyCustomClient::new(config)))
    }
}

// Register at startup:
use remotemedia_runtime_core::transport::plugin_registry::global_registry;
global_registry().register(Arc::new(MyCustomTransportPlugin))?;
```

## FAQ

**Q: Do I need to change my manifest files?**
A: No, manifests are backward compatible.

**Q: What if I don't register a transport plugin?**
A: You'll get a helpful error: "Transport 'X' not found. Available: Y, Z. Make sure to register..."

**Q: Can I still use the old create_transport_client()?**
A: Yes, but it's deprecated and will return an error directing you to use the plugin registry.

**Q: When will the deprecated functions be removed?**
A: Planned for v1.0.0 (6+ months).

**Q: How do I migrate tests?**
A: Register test transport plugins in test setup:
```rust
#[tokio::test]
async fn my_test() {
    // Setup
    use remotemedia_grpc::GrpcTransportPlugin;
    global_registry().register(Arc::new(GrpcTransportPlugin)).unwrap();

    // Your test code...
}
```

## Migration Checklist

- [ ] Identify all uses of `create_transport_client()`
- [ ] Add plugin registration at application startup
- [ ] Update client creation code to use plugin registry
- [ ] Test with actual transport connections
- [ ] Update tests to register plugins in setup
- [ ] Remove any direct imports of old client modules

## Need Help?

- See `specs/006-remote-pipeline-node/` for detailed design documentation
- Check `runtime-core/tests/test_custom_transport.rs` for plugin examples
- Open an issue on GitHub if you encounter migration problems
