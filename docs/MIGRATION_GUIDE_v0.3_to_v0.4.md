# Migration Guide: v0.3.x → v0.4.x

**Target Audience**: Users upgrading from RemoteMedia SDK v0.3.x to v0.4.x

**Breaking Changes**: Minimal - Most code continues to work with dependency updates only

## Overview

Version 0.4 introduces **transport layer decoupling**, separating transport implementations into independent crates for faster builds, cleaner architecture, and independent versioning.

## Quick Start

### For gRPC Users

```toml
# Update Cargo.toml
[dependencies]
remotemedia-grpc = "0.4"
remotemedia-runtime-core = "0.4"
```

```rust
// Update imports
use remotemedia_grpc::GrpcServer;
use remotemedia_runtime_core::transport::PipelineRunner;

// Use PipelineRunner (nodes auto-registered)
let runner = Arc::new(PipelineRunner::new()?);
let server = GrpcServer::new(config, runner)?;
```

### For Python Users

```bash
pip install remotemedia-sdk --upgrade
# API unchanged - just faster installation!
```

## Benefits

- **53% faster builds** for gRPC transport (14s vs 30s)
- **30% faster** Python package installation
- **Independent versioning** of transports
- **Cleaner testing** with mock transports

See full guide: [docs/MIGRATION_GUIDE_v0.3_to_v0.4.md](MIGRATION_GUIDE_v0.3_to_v0.4.md)
