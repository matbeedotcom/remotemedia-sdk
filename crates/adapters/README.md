# RemoteMedia Adapters

This directory contains protocol-specific ingestion adapters for the RemoteMedia SDK.

## Overview

Adapters are optional crates that extend the ingestion framework with support for additional protocols. Each adapter implements the `IngestPlugin` trait and can be registered with the global `IngestRegistry`.

## Available Adapters

| Crate | Schemes | Description |
|-------|---------|-------------|
| [`ingest-rtmp`](./ingest-rtmp/) | `rtmp://`, `rtmps://`, `rtsp://`, `rtsps://`, `udp://`, `rtp://`, `srt://` | Streaming protocol ingestion via FFmpeg |

## Usage

### Adding an Adapter

Add the adapter as a dependency in your `Cargo.toml`:

```toml
[dependencies]
remotemedia-runtime-core = { version = "0.1" }
remotemedia-ingest-rtmp = { version = "0.1" }  # Optional
```

### Registering Plugins

Register the adapter plugin with the global registry:

```rust
use std::sync::Arc;
use remotemedia_runtime_core::ingestion::global_ingest_registry;
use remotemedia_ingest_rtmp::RtmpIngestPlugin;

// Register the streaming plugin
global_ingest_registry().register(Arc::new(RtmpIngestPlugin))?;

// Now RTMP/RTSP/UDP URLs will work
let config = IngestConfig::from_url("rtmp://localhost:1935/live/stream");
let source = global_ingest_registry().create_from_uri(&config)?;
```

### Feature Flags

For optional adapter support, use feature flags:

```toml
[features]
default = []
rtmp = ["dep:remotemedia-ingest-rtmp"]

[dependencies]
remotemedia-ingest-rtmp = { version = "0.1", optional = true }
```

Then conditionally register:

```rust
#[cfg(feature = "rtmp")]
{
    use remotemedia_ingest_rtmp::RtmpIngestPlugin;
    global_ingest_registry().register(Arc::new(RtmpIngestPlugin)).ok();
}
```

## Creating a New Adapter

1. Create a new crate in this directory:
   ```sh
   cargo new ingest-myproto --lib
   ```

2. Add dependencies to `Cargo.toml`:
   ```toml
   [package]
   name = "remotemedia-ingest-myproto"
   version.workspace = true
   edition.workspace = true

   [dependencies]
   remotemedia-runtime-core = { path = "../../runtime-core" }
   tokio = { workspace = true }
   async-trait = { workspace = true }
   ```

3. Implement the `IngestPlugin` trait:
   ```rust
   use remotemedia_runtime_core::ingestion::{
       IngestPlugin, IngestSource, IngestConfig, Error
   };

   pub struct MyProtoPlugin;

   impl IngestPlugin for MyProtoPlugin {
       fn name(&self) -> &'static str { "myproto" }
       
       fn schemes(&self) -> &'static [&'static str] { &["myproto", "myprotos"] }
       
       fn create(&self, config: &IngestConfig) -> Result<Box<dyn IngestSource>, Error> {
           Ok(Box::new(MyProtoSource::new(config)?))
       }
       
       fn validate(&self, config: &IngestConfig) -> Result<(), Error> {
           // Validate URL format
           if !config.url.starts_with("myproto://") && !config.url.starts_with("myprotos://") {
               return Err(Error::InvalidScheme("Expected myproto:// or myprotos://".into()));
           }
           Ok(())
       }
   }
   ```

4. Implement the `IngestSource` trait for your source type.

5. Add the crate to workspace members in root `Cargo.toml`:
   ```toml
   [workspace]
   members = [
       # ...
       "adapters/ingest-myproto",
   ]
   ```

## Architecture

```
adapters/
├── README.md               # This file
├── ingest-rtmp/           # RTMP/RTSP/UDP adapter
│   ├── Cargo.toml
│   ├── build.rs           # FFmpeg linking
│   └── src/
│       ├── lib.rs         # RtmpIngestPlugin, RtmpIngestSource
│       └── demuxer.rs     # FFmpeg demuxing/decoding logic
│
├── ingest-srt/            # SRT adapter (future)
│   └── ...
│
└── ingest-webrtc/         # WebRTC SFU adapter (future)
    └── ...
```

## Key Concepts

### Multi-Track Support

Adapters should tag `RuntimeData` with `stream_id` to support multi-track sources:

- `"audio:0"` - First audio track
- `"audio:1"` - Second audio track  
- `"video:0"` - First video track
- `"subtitle:0"` - First subtitle track

### Reconnection

Adapters handling network streams should implement reconnection using `IngestConfig::reconnect`:

```rust
if config.reconnect.enabled {
    for attempt in 0..config.reconnect.max_attempts {
        // Try to connect...
        if connected {
            break;
        }
        let delay = config.reconnect.delay_ms * 
            (config.reconnect.backoff_multiplier.powi(attempt as i32) as u64);
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}
```

### Status Reporting

Update `IngestStatus` as the connection state changes:

- `Idle` → Created, not yet started
- `Connecting` → Attempting to connect
- `Connected` → Successfully connected, producing data
- `Reconnecting { attempt, max_attempts }` → Temporarily disconnected
- `Disconnected` → Cleanly stopped
- `Error(String)` → Fatal error

## Testing

Each adapter should include:

1. **Unit tests** for URL validation and plugin configuration
2. **Integration tests** (marked `#[ignore]`) for actual protocol testing

Run integration tests with:

```sh
# Requires running RTMP/RTSP server
cargo test -p remotemedia-ingest-rtmp --test integration_tests -- --ignored
```
