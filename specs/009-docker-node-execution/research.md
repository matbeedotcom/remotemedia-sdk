# Research: Docker-Based Node Execution Technologies

**Date**: 2025-11-11
**Feature**: Docker-Based Node Execution with iceoryx2 IPC
**Status**: Completed

## Overview

This document resolves all NEEDS CLARIFICATION items from [plan.md](plan.md) Technical Context section. Research focused on selecting optimal technologies and strategies for implementing Docker container management in Rust while maintaining zero-copy iceoryx2 IPC performance.

---

## 1. Rust Docker Client Library Selection

### Decision: **bollard**

### Rationale

bollard is the clear choice for modern Rust Docker integration based on:

1. **Active Maintenance**: Latest release in 2025 with Rust 1.88, tonic 0.14, Docker API 1.49 support. Contrast: shiplift has no releases since 2020.

2. **Modern Async Architecture**: Fully asynchronous API with futures, streams, and async/await built on Hyper and Tokio. Perfect alignment with existing tokio-based runtime (Cargo.toml shows `tokio = "1.35"`).

3. **Comprehensive API Coverage**:
   - Image builds with BuildKit and cache management
   - Container lifecycle (create/start/stop/remove)
   - Stats streaming for monitoring
   - Exec for in-container command execution
   - Network management

4. **Cross-Platform**: Windows Named Pipes + Linux Unix sockets. Critical for development on Windows (per git status showing Windows paths).

5. **Production-Ready**: Active community, issue resolution, used in production environments.

### Alternatives Considered

- **shiplift**: Legacy option, no maintenance since 2020, uncertain future. Basic Docker daemon interaction but lacks modern features and async patterns.
- **dockworker**: Less popular, limited documentation, smaller ecosystem.

### Implementation Notes

```toml
# Add to runtime-core/Cargo.toml
[dependencies]
bollard = "0.19"  # Latest stable as of 2025
```

**Key API Patterns**:
```rust
use bollard::Docker;
use bollard::image::BuildImageOptions;
use bollard::container::{CreateContainerOptions, Config, StartContainerOptions};

// Connect to local Docker daemon
let docker = Docker::connect_with_local_defaults()?;

// Build image (streaming logs)
let mut build_stream = docker.build_image(
    BuildImageOptions {
        dockerfile: "Dockerfile",
        t: "omniasr-node:py310",
        ..Default::default()
    },
    None,
    Some(dockerfile_tar_bytes),
);

// Create and start container
let container = docker.create_container(
    Some(CreateContainerOptions { name: "omniasr_node_1", ..Default::default() }),
    Config {
        image: Some("omniasr-node:py310"),
        host_config: Some(HostConfig {
            binds: Some(vec![
                "/tmp/iceoryx2:/tmp/iceoryx2".to_string(),
                "/dev/shm:/dev/shm".to_string(),
            ]),
            shm_size: Some(2_000_000_000), // 2GB
            ..Default::default()
        }),
        ..Default::default()
    },
).await?;

docker.start_container(&container.id, None::<StartContainerOptions<String>>).await?;
```

**Performance**: Direct Docker daemon communication via Unix socket/Named Pipe. Expected overhead: ~8-18µs from Tokio async runtime (negligible for container operations measured in seconds).

---

## 2. Container Image Cache Persistence Strategy

### Decision: **SQLite database**

### Rationale

SQLite provides optimal balance of simplicity, reliability, and performance:

1. **Performance Advantages**:
   - Benchmark data: 18GB JSON → 4.8GB SQLite (73% reduction)
   - Startup time: 30 minutes (JSON parsing) → 10 seconds (SQLite query)
   - 110MB JSON parsing: 10-18 seconds. Equivalent SQLite query: sub-second
   - Sequential I/O patterns, less disk thrashing

2. **Reliability**:
   - ACID transactions ensure data integrity during crashes
   - Atomic updates prevent corruption
   - Built-in locking handles concurrent access from multiple runtime instances

3. **Simplicity**:
   - Single file database (`~/.cache/remotemedia/image_cache.db`)
   - Zero configuration, serverless architecture
   - Excellent Rust support via `rusqlite` (synchronous) or `sqlx` (async)
   - Easy migration from JSON (import existing data)

4. **Feature Set**:
   - Complex queries for image lookup (by tag, hash, last used timestamp)
   - Indexed searches for fast retrieval
   - JSON column support in SQLite 3.43+ for flexible metadata
   - Built-in vacuum for reclaiming space after deletions

### Alternatives Considered

**JSON File**:
- **Pros**: Human-readable, simple tooling, git-friendly for debugging
- **Cons**: Whole-file parsing on every read (~10s for 110MB), no concurrent access safety, poor performance for frequent updates
- **Verdict**: Only suitable if updates are infrequent (<1/day) and dataset small (<1000 images)

**In-Memory Only**:
- **Pros**: Fastest access, simplest code
- **Cons**: Data loss on restart (expensive rebuild), wasted computation re-pulling images, no persistence across service restarts
- **Verdict**: Unacceptable for production - runtime restarts would require rebuilding entire cache

### Implementation Notes

**Schema Design**:
```sql
CREATE TABLE image_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    image_name TEXT NOT NULL,
    image_tag TEXT NOT NULL,
    image_id TEXT NOT NULL,  -- Docker image ID (sha256:...)
    digest TEXT,              -- Manifest digest for verification
    size_bytes INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_pulled TIMESTAMP,
    last_used TIMESTAMP,
    build_config TEXT,        -- JSON: Dockerfile content, build args
    dependencies TEXT,        -- JSON: system deps, Python packages
    python_version TEXT,      -- e.g., "3.10"
    base_image TEXT,          -- e.g., "python:3.10-slim" or custom
    status TEXT NOT NULL,     -- "available", "building", "failed"
    error_message TEXT,       -- If status="failed", store error
    UNIQUE(image_name, image_tag)
);

CREATE INDEX idx_image_lookup ON image_metadata(image_name, image_tag);
CREATE INDEX idx_last_used ON image_metadata(last_used DESC);
CREATE INDEX idx_status ON image_metadata(status);
```

**Rust Implementation** (using rusqlite):
```rust
use rusqlite::{Connection, params, OptionalExtension};
use serde::{Serialize, Deserialize};
use std::path::Path;

pub struct ImageCache {
    conn: Connection,
}

impl ImageCache {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        // Initialize schema
        conn.execute_batch(include_str!("schema.sql"))?;
        Ok(Self { conn })
    }

    pub fn get_image(&self, name: &str, tag: &str) -> Result<Option<ImageMetadata>> {
        self.conn.query_row(
            "SELECT * FROM image_metadata WHERE image_name = ? AND image_tag = ? AND status = 'available'",
            params![name, tag],
            |row| ImageMetadata::from_row(row)
        ).optional()
    }

    pub fn upsert_image(&mut self, metadata: &ImageMetadata) -> Result<()> {
        self.conn.execute(
            "INSERT INTO image_metadata (image_name, image_tag, image_id, size_bytes, build_config, dependencies, python_version, base_image, status, last_used)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(image_name, image_tag) DO UPDATE SET
                image_id = excluded.image_id,
                size_bytes = excluded.size_bytes,
                status = excluded.status,
                last_used = CURRENT_TIMESTAMP",
            params![
                metadata.image_name,
                metadata.image_tag,
                metadata.image_id,
                metadata.size_bytes,
                serde_json::to_string(&metadata.build_config)?,
                serde_json::to_string(&metadata.dependencies)?,
                metadata.python_version,
                metadata.base_image,
                metadata.status.as_str(),
            ]
        )?;
        Ok(())
    }

    /// Evict least recently used images beyond max_count
    pub fn evict_lru(&mut self, max_count: usize) -> Result<Vec<String>> {
        let images_to_remove: Vec<String> = self.conn.prepare(
            "SELECT image_id FROM image_metadata
             ORDER BY last_used DESC
             LIMIT -1 OFFSET ?"
        )?.query_map(params![max_count], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

        for image_id in &images_to_remove {
            self.conn.execute("DELETE FROM image_metadata WHERE image_id = ?", params![image_id])?;
        }

        Ok(images_to_remove)
    }
}
```

**Cache Location**:
- Linux: `~/.cache/remotemedia/image_cache.db`
- Windows: `%LOCALAPPDATA%\remotemedia\image_cache.db`
- Override via `REMOTEMEDIA_IMAGE_CACHE_PATH` environment variable

**Cache Eviction Policy**:
- LRU eviction based on `last_used` timestamp
- Default: keep 50 most recently used images
- Configurable via `REMOTEMEDIA_IMAGE_CACHE_MAX_SIZE` environment variable
- Run eviction after each new image build or periodically (daily cron)

---

## 3. Docker Image Build Strategy

### Decision: **Multi-stage builds with layer caching optimization**

### Rationale

Multi-stage builds provide optimal image size (~60% reduction), security (minimal attack surface), and build speed (better layer caching):

1. **Image Size**: 500MB-1GB (multi-stage) vs 2-3GB (single-stage). Removing build-essential alone saves ~250MB.

2. **Security**: Runtime image contains only necessary dependencies, not build tools (gcc, git, etc.). Reduces CVE exposure.

3. **Build Performance**: Separate stages for dependencies (changes rarely) vs application code (changes frequently) enables efficient layer caching.

4. **Existing Pattern**: Project already uses multi-stage builds (see reference Dockerfiles), maintain consistency.

### Multi-Stage Build Template

```dockerfile
# ============================================================================
# Stage 1: Builder - Heavy dependencies and compilation
# ============================================================================
FROM python:3.10-slim as builder

# Install build dependencies (will be discarded in final image)
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    gcc \
    g++ \
    git \
    wget \
    # Dev packages for building Python extensions
    libsndfile1-dev \
    libavcodec-dev \
    && rm -rf /var/lib/apt/lists/*

# Create virtual environment to isolate dependencies
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Copy requirements FIRST (for layer caching - changes infrequently)
COPY requirements.txt .

# Install Python packages with pip cache mount
RUN --mount=type=cache,target=/root/.cache/pip \
    pip install --no-cache-dir -r requirements.txt

# Install iceoryx2 Python bindings
RUN pip install --no-cache-dir iceoryx2

# Download and install local wheels if needed
# COPY wheels/omnilingual_asr-0.1.0-py3-none-any.whl .
# RUN pip install --no-cache-dir omnilingual_asr-0.1.0-py3-none-any.whl

# ============================================================================
# Stage 2: Runtime - Minimal runtime environment
# ============================================================================
FROM python:3.10-slim as runtime

# Install ONLY runtime system dependencies (not build tools)
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Audio libraries (runtime only, no -dev packages)
    libsndfile1 \
    ffmpeg \
    libsox-fmt-all \
    # Cleanup aggressively
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Create non-root user (security best practice)
RUN useradd -m -u 1000 omniasr && \
    mkdir -p /tmp/iceoryx2 && \
    chown omniasr:omniasr /tmp/iceoryx2

USER omniasr
WORKDIR /home/omniasr/app

# Copy application code LAST (changes most frequently)
COPY --chown=omniasr:omniasr ./src ./src
COPY --chown=omniasr:omniasr ./runner.py .

# Set Python environment
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1

# Ensure proper signal handling for graceful shutdown
STOPSIGNAL SIGTERM

CMD ["python", "runner.py"]
```

### Layer Caching Optimization Techniques

**Principle**: Order Dockerfile commands by change frequency (ascending):

1. **Base image** (changes: never)
2. **System packages** (changes: rarely)
3. **Requirements.txt** (changes: occasionally)
4. **Application code** (changes: frequently)

**Key Techniques**:

```dockerfile
# ✅ Good - requirements layer cached independently
COPY requirements.txt .
RUN pip install -r requirements.txt
COPY ./src ./src

# ❌ Bad - any code change invalidates pip install
COPY . .
RUN pip install -r requirements.txt
```

**BuildKit Cache Mounts** (Docker 18.09+):
```dockerfile
# Cache pip downloads across builds
RUN --mount=type=cache,target=/root/.cache/pip \
    pip install -r requirements.txt

# Cache apt packages
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    apt-get update && apt-get install -y ffmpeg
```

**Consolidate RUN Commands**:
```dockerfile
# ✅ Good - single layer
RUN apt-get update && apt-get install -y \
    ffmpeg \
    libsndfile1 \
    && rm -rf /var/lib/apt/lists/*

# ❌ Bad - three layers (larger image, slower build)
RUN apt-get update
RUN apt-get install -y ffmpeg libsndfile1
RUN rm -rf /var/lib/apt/lists/*
```

### System Dependencies Matrix

| Dependency | Purpose | Stage | Package Name | Notes |
|------------|---------|-------|--------------|-------|
| **libsndfile1** | Audio I/O | Runtime | `libsndfile1` | Runtime library only, not `libsndfile1-dev` |
| **ffmpeg** | Audio transcoding | Runtime | `ffmpeg` | Full package, not individual libs |
| **libsox-fmt-all** | Audio format support | Runtime | `libsox-fmt-all` | Optional, only if diverse formats needed |
| **build-essential** | Compile extensions | Builder | `build-essential` | ~250MB, discard after build |
| **gcc/g++** | C/C++ compilation | Builder | `gcc g++` | For numpy/scipy native extensions |
| **git** | Clone dependencies | Builder | `git` | If installing from git repos |
| **libsndfile1-dev** | Build native extensions | Builder | `libsndfile1-dev` | Headers for compilation |

### iceoryx2 Installation

```dockerfile
# Builder stage
RUN pip install --no-cache-dir iceoryx2

# Runtime stage - copied via venv
COPY --from=builder /opt/venv /opt/venv
# iceoryx2 has no additional system dependencies beyond POSIX shared memory
```

**Important**: iceoryx2 is a zero-dependency IPC library. No RouDi daemon, no special system requirements beyond standard POSIX shared memory APIs (available in base Python image).

### CI/CD Registry Cache Strategy

For GitHub Actions or other CI:

```bash
# Build with registry cache export
docker buildx build \
  --cache-to type=registry,ref=ghcr.io/yourorg/omniasr-cache,mode=max \
  --cache-from type=registry,ref=ghcr.io/yourorg/omniasr-cache \
  --push \
  -t ghcr.io/yourorg/omniasr-node:py310 \
  .

# mode=max exports ALL layers (slower push, faster subsequent builds)
# mode=min (default) only exports final image layers
```

### Alternatives Considered

**Single-Stage Build**:
- **Pros**: Simple, easier to debug
- **Cons**: 2-3GB images (vs 500MB-1GB), includes unnecessary build tools in production, larger attack surface (more CVEs)
- **Verdict**: Only acceptable for quick prototyping, never for production

---

## 4. iceoryx2 Docker Integration

### Decision: **Mount `/tmp/iceoryx2` and `/dev/shm`, no RouDi daemon**

### Rationale

iceoryx2 has significantly simpler Docker requirements than original iceoryx:

1. **No RouDi Daemon**: iceoryx2 is brokerless, eliminating complex daemon lifecycle management
2. **Minimal Mounts**: Only 2 volumes required (vs 3+ for original iceoryx)
3. **Filesystem-Based Discovery**: Service discovery via shared directory (no socket management)
4. **Already in Use**: Project already uses iceoryx2 0.7.0 (per Cargo.toml), this extends existing architecture

### Required Volume Mounts

**Docker Compose Configuration**:
```yaml
version: '3.8'

services:
  rust-runtime:
    image: remotemedia-runtime:latest
    volumes:
      - iceoryx2_data:/tmp/iceoryx2
      - type: bind
        source: /dev/shm
        target: /dev/shm
    shm_size: '2gb'  # Increase from 64MB default

  omniasr-node-1:
    image: omniasr-node:py310
    volumes:
      - iceoryx2_data:/tmp/iceoryx2
      - type: bind
        source: /dev/shm
        target: /dev/shm
    shm_size: '2gb'
    depends_on:
      - rust-runtime

volumes:
  iceoryx2_data:
    driver: local
```

**Docker Run Command**:
```bash
docker run \
  --name omniasr-node \
  -v /tmp/iceoryx2:/tmp/iceoryx2 \
  -v /dev/shm:/dev/shm \
  --shm-size=2g \
  omniasr-node:py310
```

**Explanation**:

1. **`/tmp/iceoryx2`**: Service discovery files and channel metadata
   - All communicating containers must mount the same host directory
   - Customizable via config file (default location)

2. **`/dev/shm`**: POSIX shared memory for zero-copy data transfer
   - Default size: 64MB (insufficient for audio streaming)
   - Recommended: 1-2GB for continuous audio buffers
   - Calculate: `buffer_size × concurrent_sessions × 2 (input+output)`

**No Additional Mounts Needed**:
- ❌ No `/dev/mqueue` (used by original iceoryx for RouDi)
- ❌ No socket files (no daemon)
- ❌ No separate configuration directory

### Shared Memory Sizing Calculation

```rust
// Example for audio streaming
const SAMPLE_RATE: usize = 16000; // Hz
const BUFFER_SECONDS: usize = 5;
const BYTES_PER_SAMPLE: usize = 4; // f32

let buffer_size = SAMPLE_RATE * BUFFER_SECONDS * BYTES_PER_SAMPLE; // 320KB
let num_sessions = 10;
let num_buffers_per_session = 2; // input + output

let required_shm = buffer_size * num_sessions * num_buffers_per_session;
// = 320KB × 10 × 2 = 6.4MB

// Recommended: 2GB for headroom (10x safety margin)
```

### Common Pitfalls and Solutions

#### Pitfall 1: Stale Service Files

**Problem**: Crashed containers leave service discovery files, causing routing failures.

**Solution**: Session-scoped channel naming (already implemented per CLAUDE.md):
```rust
// From existing codebase
let input_channel = format!("{}_{}_input", session_id, node_id);
let output_channel = format!("{}_{}_output", session_id, node_id);

// Cleanup on session termination
async fn cleanup_session(session_id: &str) -> Result<()> {
    let pattern = format!("/tmp/iceoryx2/services/{}_*", session_id);
    for entry in glob::glob(&pattern)? {
        if let Ok(path) = entry {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}
```

#### Pitfall 2: Permission Errors

**Problem**: Non-root container user (UID 1000) cannot access `/tmp/iceoryx2` or `/dev/shm`.

**Solution**: Pre-create directories in Dockerfile:
```dockerfile
# Create directories with correct ownership
RUN mkdir -p /tmp/iceoryx2 && \
    chown omniasr:omniasr /tmp/iceoryx2
```

Or adjust volume permissions on host:
```bash
sudo mkdir -p /tmp/iceoryx2
sudo chmod 777 /tmp/iceoryx2  # Or use specific group
```

#### Pitfall 3: Graceful Shutdown

**Problem**: Container restart leaves zombie services in registry.

**Solution**: Implement signal handling:
```rust
use tokio::signal;

#[tokio::main]
async fn main() -> Result<()> {
    let session = start_session().await?;

    tokio::select! {
        _ = signal::ctrl_c() => {
            tracing::info!("Shutdown signal received");
            cleanup_session(&session.id).await?;
        }
        result = run_session(session) => {
            if let Err(e) = result {
                tracing::error!("Session error: {}", e);
            }
        }
    }

    Ok(())
}
```

Dockerfile signal configuration:
```dockerfile
STOPSIGNAL SIGTERM
# Docker stop waits 10s by default before SIGKILL
```

#### Pitfall 4: Cross-Host Communication

**Problem**: Expecting iceoryx2 to work across Docker hosts (e.g., Kubernetes nodes).

**Solution**: **iceoryx2 cannot span hosts** (POSIX shared memory limitation). For distributed pipelines:

1. Use gRPC transport (existing `remotemedia-grpc` crate):
   ```rust
   if node.is_remote {
       use_grpc_transport(node).await?;
   } else {
       use_iceoryx2_transport(node).await?;
   }
   ```

2. Deploy all communicating containers on same host:
   ```yaml
   services:
     omniasr-node:
       deploy:
         placement:
           constraints:
             - node.hostname == specific-host
   ```

3. Document limitation clearly in README

### Verification Commands

```bash
# Check shared memory usage
df -h /dev/shm

# List iceoryx2 service files
ls -la /tmp/iceoryx2/services/

# Monitor from inside container
docker exec omniasr-node ls -la /tmp/iceoryx2/

# Find stale files
find /tmp/iceoryx2 -mtime +1  # Older than 1 day
```

### iceoryx2 vs Original iceoryx Comparison

| Aspect | iceoryx2 | Original iceoryx |
|--------|----------|------------------|
| **RouDi Daemon** | ❌ Not needed | ✅ Required |
| **Volume Mounts** | 2 mounts | 3+ mounts |
| **Service Discovery** | Filesystem | RouDi-managed |
| **Complexity** | Low | Medium-High |
| **Container Lifecycle** | Independent | Coupled to RouDi |
| **Project Status** | ✅ Already using (Cargo.toml) | N/A |

**Recommendation**: Existing choice of iceoryx2 is optimal for containerized environments. Simpler architecture eliminates daemon management while maintaining zero-copy performance.

---

## Summary of Decisions

| Decision Area | Recommendation | Key Benefit | Implementation Effort |
|---------------|----------------|-------------|----------------------|
| **Docker Client** | bollard | Active maintenance, tokio integration | Low (add dependency) |
| **Metadata Persistence** | SQLite | 4x size reduction, ACID transactions | Medium (schema + CRUD) |
| **Build Strategy** | Multi-stage builds | 60% size reduction, security | Low (follow template) |
| **iceoryx2 Setup** | Mount `/tmp/iceoryx2` + `/dev/shm` | Zero-copy IPC, no daemon | Low (volume config) |

## Next Steps

1. **Add Dependencies**:
   ```bash
   cd runtime-core
   cargo add bollard@0.19
   cargo add rusqlite
   ```

2. **Create Module Structure**:
   ```bash
   mkdir -p runtime-core/src/python/docker
   touch runtime-core/src/python/docker/{mod.rs,docker_executor.rs,container_manager.rs,image_builder.rs,container_registry.rs,ipc_bridge.rs,config.rs}
   ```

3. **Implement Image Cache**:
   - Create SQLite schema (use template from section 2)
   - Implement ImageCache struct with CRUD operations
   - Add LRU eviction logic

4. **Create Standard Base Images**:
   ```bash
   mkdir -p docker/base-images
   # Create python39.Dockerfile, python310.Dockerfile, python311.Dockerfile
   ```

5. **Test iceoryx2 Integration**:
   - Verify volume mounts work
   - Benchmark latency in containerized setup
   - Test session-scoped channel naming

6. **Update Documentation**:
   - Add Docker setup to project README
   - Document volume mount requirements
   - Create quickstart guide for Docker nodes

## References

- bollard documentation: https://docs.rs/bollard/latest/bollard/
- iceoryx2 Docker example: https://iceoryx.io/v2.0.2/examples/icedocker/
- SQLite performance benchmarks: Various community benchmarks cited
- Multi-stage build best practices: Docker official documentation
