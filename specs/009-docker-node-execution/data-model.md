# Data Model: Docker-Based Node Execution

**Date**: 2025-11-11
**Feature**: Docker-Based Node Execution with iceoryx2 IPC
**Status**: Draft

## Overview

This document defines the core entities, their relationships, and validation rules for the Docker-based node execution feature. All entities are derived from [spec.md](spec.md) Key Entities section and functional requirements.

---

## Entity Diagrams

### Core Relationships

```
┌─────────────────────────────────────────────────────────────────────┐
│  Pipeline Session                                                    │
│  - session_id: String (UUID)                                        │
│  - created_at: Timestamp                                            │
│  - status: Active | Terminated                                      │
└────────┬────────────────────────────────────────────────────────────┘
         │ 1:N
         │
         ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Dockerized Node Configuration                                       │
│  - node_id: String                                                  │
│  - executor_type: "docker"                                          │
│  - python_version: String (e.g., "3.10")                           │
│  - system_dependencies: Vec<String>                                │
│  - python_packages: Vec<String>                                    │
│  - resource_limits: ResourceLimits                                 │
│  - base_image: Option<String> (custom)                             │
│  - config_hash: String (SHA256 for cache lookup)                   │
└────────┬────────────────────────────────────────────────────────────┘
         │ 1:1
         │
         ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Container Image Cache Entry                                         │
│  - image_id: String (Docker sha256:...)                            │
│  - image_name: String (e.g., "omniasr-node")                       │
│  - image_tag: String (e.g., "py310-hash123")                       │
│  - config_hash: String (matches NodeConfiguration)                 │
│  - size_bytes: i64                                                 │
│  - created_at: Timestamp                                           │
│  - last_used: Timestamp                                            │
│  - status: Available | Building | Failed                           │
└────────┬────────────────────────────────────────────────────────────┘
         │ 1:N
         │
         ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Container Session Instance                                          │
│  - container_id: String (Docker container ID)                       │
│  - node_id: String                                                 │
│  - session_ids: Vec<String> (sessions using this container)       │
│  - reference_count: AtomicUsize                                    │
│  - health_status: Healthy | Unhealthy | Starting | Stopping       │
│  - started_at: Timestamp                                           │
│  - last_health_check: Timestamp                                    │
└────────┬────────────────────────────────────────────────────────────┘
         │ 1:N
         │
         ▼
┌─────────────────────────────────────────────────────────────────────┐
│  iceoryx2 IPC Channel                                                │
│  - channel_name: String ("{session_id}_{node_id}_{input|output}")  │
│  - session_id: String                                              │
│  - node_id: String                                                 │
│  - direction: Input | Output                                       │
│  - publisher: Option<Publisher> (!Send, on IPC thread)            │
│  - subscriber: Option<Subscriber> (!Send, on IPC thread)          │
└─────────────────────────────────────────────────────────────────────┘
```

### Lifecycle States

```
Container Session Instance Lifecycle:

   [Starting] ──┐
        │       │ health check fail
        │       │
        ▼       ▼
   [Healthy] ──> [Unhealthy]
        │             │
        │ ref_count=0 │ timeout/retry fail
        ▼             ▼
   [Stopping] ──> [Terminated]
```

```
Image Cache Entry Lifecycle:

   [Building] ──┐
        │       │ build error
        │       │
        ▼       ▼
   [Available]  [Failed]
        │
        │ eviction (LRU)
        ▼
   [Deleted]
```

---

## Entity Definitions

### 1. Dockerized Node Configuration

**Source**: spec.md Key Entities, FR-004

**Purpose**: Represents a node's environment specification from the pipeline manifest. Defines what Python environment, dependencies, and resource limits are required for a node.

**Fields**:

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `node_id` | String | ✅ | Non-empty, unique within pipeline | Identifier from manifest |
| `executor_type` | String | ✅ | Must be "docker" | Execution mode |
| `python_version` | String | ✅ | Must match supported versions (3.9, 3.10, 3.11) | Python runtime version |
| `system_dependencies` | Vec<String> | ❌ | Each entry non-empty | System packages (e.g., ["ffmpeg", "libsndfile1"]) |
| `python_packages` | Vec<String> | ❌ | Each entry non-empty | Python packages (e.g., ["numpy==1.24.0", "torch"]) |
| `resource_limits` | ResourceLimits | ✅ | See ResourceLimits validation | CPU and memory limits |
| `base_image` | Option<String> | ❌ | If present, must be valid Docker image ref | Custom base image (overrides standard) |
| `config_hash` | String | ✅ (computed) | SHA256 hex string | Hash of all config fields for cache lookup |

**Relationships**:
- Belongs to 1 Pipeline Session
- Maps to 1 Container Image Cache Entry (via config_hash)
- Creates N Container Session Instances (via container sharing - FR-012)

**Validation Rules**:

```rust
impl DockerizedNodeConfiguration {
    pub fn validate(&self) -> Result<(), ValidationError> {
        // FR-004: Executor type must be "docker"
        if self.executor_type != "docker" {
            return Err(ValidationError::InvalidExecutorType);
        }

        // FR-013: Python version must be supported
        const SUPPORTED_VERSIONS: &[&str] = &["3.9", "3.10", "3.11"];
        if !SUPPORTED_VERSIONS.contains(&self.python_version.as_str()) {
            return Err(ValidationError::UnsupportedPythonVersion);
        }

        // FR-014: Resource limits must be valid
        self.resource_limits.validate()?;

        // FR-016: If custom base image provided, validate it
        if let Some(ref base_image) = self.base_image {
            validate_custom_base_image(base_image)?;
        }

        // System dependencies and Python packages must be non-empty strings
        for dep in &self.system_dependencies {
            if dep.trim().is_empty() {
                return Err(ValidationError::EmptyDependency);
            }
        }
        for pkg in &self.python_packages {
            if pkg.trim().is_empty() {
                return Err(ValidationError::EmptyPackage);
            }
        }

        Ok(())
    }

    pub fn compute_config_hash(&self) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();

        // Hash all configuration fields deterministically
        hasher.update(self.python_version.as_bytes());
        hasher.update(self.base_image.as_ref().map(|s| s.as_bytes()).unwrap_or(b""));

        for dep in &self.system_dependencies {
            hasher.update(dep.as_bytes());
        }
        for pkg in &self.python_packages {
            hasher.update(pkg.as_bytes());
        }

        hasher.update(&self.resource_limits.memory_mb.to_le_bytes());
        hasher.update(&self.resource_limits.cpu_cores.to_le_bytes());

        hex::encode(hasher.finalize())
    }
}
```

**Example**:
```rust
DockerizedNodeConfiguration {
    node_id: "omniasr_transcribe".to_string(),
    executor_type: "docker".to_string(),
    python_version: "3.10".to_string(),
    system_dependencies: vec!["ffmpeg".to_string(), "libsndfile1".to_string()],
    python_packages: vec!["numpy==1.24.0".to_string(), "iceoryx2".to_string()],
    resource_limits: ResourceLimits {
        memory_mb: 2048,
        cpu_cores: 2.0,
    },
    base_image: None, // Use standard python:3.10-slim base
    config_hash: "a3b2c1...".to_string(), // Computed
}
```

---

### 2. Container Image Cache Entry

**Source**: spec.md Key Entities, research.md Section 2

**Purpose**: Tracks built Docker images with metadata for reuse across pipeline sessions. Stored in SQLite database for persistence (FR-013, P3 user story).

**Fields**:

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `id` | i64 | ✅ (auto) | SQLite AUTOINCREMENT | Primary key |
| `image_id` | String | ✅ | Docker image ID format (sha256:...) | Docker's internal image ID |
| `image_name` | String | ✅ | Non-empty | Human-readable name (e.g., "omniasr-node") |
| `image_tag` | String | ✅ | Non-empty | Tag (e.g., "py310-hash123") |
| `config_hash` | String | ✅ | SHA256 hex (64 chars) | Matches DockerizedNodeConfiguration hash |
| `size_bytes` | i64 | ✅ | Positive | Image size on disk |
| `created_at` | Timestamp | ✅ | ISO8601 | Image build timestamp |
| `last_used` | Timestamp | ✅ | ISO8601 | Last time image started a container |
| `build_config` | String (JSON) | ✅ | Valid JSON | Serialized DockerizedNodeConfiguration |
| `dependencies` | String (JSON) | ✅ | Valid JSON | System deps + Python packages list |
| `python_version` | String | ✅ | Matches supported versions | For quick filtering |
| `base_image` | String | ✅ | Non-empty | Base image used (standard or custom) |
| `status` | String | ✅ | "available" \| "building" \| "failed" | Image build status |
| `error_message` | Option<String> | ❌ | Non-empty if status=failed | Build error details |

**Unique Constraint**: `(image_name, image_tag)`

**Indexes**:
- `idx_image_lookup` on `(image_name, image_tag)` - fast lookup
- `idx_last_used` on `last_used DESC` - LRU eviction
- `idx_status` on `status` - filter available images

**Relationships**:
- Created from 1 Dockerized Node Configuration
- Used by N Container Session Instances

**Validation Rules**:

```rust
impl ImageCacheEntry {
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Image ID must be Docker format
        if !self.image_id.starts_with("sha256:") {
            return Err(ValidationError::InvalidImageId);
        }

        // Config hash must be SHA256 (64 hex chars)
        if self.config_hash.len() != 64 || !self.config_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ValidationError::InvalidConfigHash);
        }

        // Size must be positive
        if self.size_bytes <= 0 {
            return Err(ValidationError::InvalidImageSize);
        }

        // Status must be valid enum value
        match self.status.as_str() {
            "available" | "building" | "failed" => {},
            _ => return Err(ValidationError::InvalidStatus),
        }

        // If failed, must have error message
        if self.status == "failed" && self.error_message.is_none() {
            return Err(ValidationError::MissingErrorMessage);
        }

        // Build config must be valid JSON
        serde_json::from_str::<DockerizedNodeConfiguration>(&self.build_config)?;

        Ok(())
    }
}
```

**State Transitions**:

```rust
impl ImageCacheEntry {
    pub fn mark_building(&mut self) {
        self.status = "building".to_string();
        self.error_message = None;
    }

    pub fn mark_available(&mut self, image_id: String, size_bytes: i64) {
        self.status = "available".to_string();
        self.image_id = image_id;
        self.size_bytes = size_bytes;
        self.error_message = None;
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = "failed".to_string();
        self.error_message = Some(error);
    }

    pub fn update_last_used(&mut self) {
        self.last_used = Utc::now();
    }
}
```

---

### 3. Container Session Instance

**Source**: spec.md Key Entities, FR-012, FR-015

**Purpose**: Represents a running Docker container that executes a node. Tracks which sessions are using the container (for shared containers - FR-012) and manages reference counting (FR-015).

**Fields**:

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `container_id` | String | ✅ | Docker container ID format (64 hex chars) | Docker's internal container ID |
| `container_name` | String | ✅ | Non-empty, unique | Human-readable name |
| `node_id` | String | ✅ | Non-empty | Identifies which node type this container runs |
| `image_id` | String | ✅ | Matches ImageCacheEntry.image_id | Image this container was created from |
| `session_ids` | Vec<String> | ✅ | Non-empty | Sessions currently using this container |
| `reference_count` | AtomicUsize | ✅ | Positive | Number of active sessions |
| `health_status` | HealthStatus | ✅ | Enum variant | Container health state |
| `started_at` | Timestamp | ✅ | ISO8601 | When container started |
| `last_health_check` | Timestamp | ✅ | ISO8601 | Last health check timestamp |
| `resource_limits` | ResourceLimits | ✅ | See ResourceLimits validation | Applied CPU/memory limits |

**Relationships**:
- Created from 1 Container Image Cache Entry
- Used by N Pipeline Sessions (via session_ids)
- Owns N iceoryx2 IPC Channels (one per session)

**Validation Rules**:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Starting,
    Healthy,
    Unhealthy,
    Stopping,
}

impl ContainerSessionInstance {
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Container ID must be Docker format (64 hex chars)
        if self.container_id.len() != 64 || !self.container_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ValidationError::InvalidContainerId);
        }

        // Session IDs must match reference count (FR-015)
        if self.session_ids.len() != self.reference_count.load(Ordering::SeqCst) {
            return Err(ValidationError::ReferenceCountMismatch);
        }

        // Reference count must be positive if container is running
        if matches!(self.health_status, HealthStatus::Healthy | HealthStatus::Unhealthy) {
            if self.reference_count.load(Ordering::SeqCst) == 0 {
                return Err(ValidationError::ZeroReferencesForRunningContainer);
            }
        }

        Ok(())
    }

    // FR-015: Reference counting management
    pub async fn add_session(&mut self, session_id: String) -> Result<()> {
        if !self.session_ids.contains(&session_id) {
            self.session_ids.push(session_id);
            self.reference_count.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    pub async fn remove_session(&mut self, session_id: &str) -> Result<bool> {
        if let Some(pos) = self.session_ids.iter().position(|s| s == session_id) {
            self.session_ids.remove(pos);
            let prev_count = self.reference_count.fetch_sub(1, Ordering::SeqCst);

            // FR-015: Stop container when no sessions remain
            if prev_count == 1 {
                self.health_status = HealthStatus::Stopping;
                return Ok(true); // Signal: should stop container
            }
        }
        Ok(false) // Container still has other sessions
    }

    pub fn should_health_check(&self) -> bool {
        const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);
        Utc::now().signed_duration_since(self.last_health_check) > HEALTH_CHECK_INTERVAL
    }
}
```

**State Transitions**:

```rust
impl ContainerSessionInstance {
    pub fn transition_to(&mut self, new_status: HealthStatus) -> Result<()> {
        use HealthStatus::*;

        // Valid transitions
        let valid = match (&self.health_status, &new_status) {
            (Starting, Healthy) => true,
            (Starting, Unhealthy) => true,
            (Healthy, Unhealthy) => true,
            (Unhealthy, Healthy) => true,  // Recovered after retry
            (Healthy, Stopping) => true,
            (Unhealthy, Stopping) => true,
            _ => false,
        };

        if !valid {
            return Err(Error::InvalidStateTransition {
                from: self.health_status.clone(),
                to: new_status.clone(),
            });
        }

        self.health_status = new_status;
        Ok(())
    }
}
```

---

### 4. iceoryx2 IPC Channel

**Source**: spec.md Key Entities, FR-010, existing multiprocess implementation

**Purpose**: Represents a zero-copy shared memory channel for data transfer between host runtime and Docker container. Uses session-scoped naming to prevent conflicts (FR-010).

**Fields**:

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `channel_name` | String | ✅ | Format: `{session_id}_{node_id}_{direction}` | Full channel identifier |
| `session_id` | String | ✅ | UUID format | Session this channel belongs to |
| `node_id` | String | ✅ | Non-empty | Node this channel communicates with |
| `direction` | Direction | ✅ | Enum variant | Input or Output |
| `created_at` | Timestamp | ✅ | ISO8601 | Channel creation time |

**Enum Types**:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Input,  // Host → Container
    Output, // Container → Host
}
```

**Relationships**:
- Belongs to 1 Container Session Instance
- Belongs to 1 Pipeline Session

**Validation Rules**:

```rust
impl IceoryxChannel {
    pub fn validate(&self) -> Result<(), ValidationError> {
        // FR-010: Channel name must follow session-scoped format
        let expected_suffix = match self.direction {
            Direction::Input => "input",
            Direction::Output => "output",
        };
        let expected_name = format!("{}_{}_{}",   self.session_id, self.node_id, expected_suffix);

        if self.channel_name != expected_name {
            return Err(ValidationError::InvalidChannelName {
                expected: expected_name,
                actual: self.channel_name.clone(),
            });
        }

        // Session ID should be UUID format
        Uuid::parse_str(&self.session_id)?;

        Ok(())
    }

    pub fn new(session_id: String, node_id: String, direction: Direction) -> Self {
        let suffix = match direction {
            Direction::Input => "input",
            Direction::Output => "output",
        };
        let channel_name = format!("{}_{}_{}",session_id, node_id, suffix);

        Self {
            channel_name,
            session_id,
            node_id,
            direction,
            created_at: Utc::now(),
        }
    }
}
```

**Important Notes** (from CLAUDE.md):

- `Publisher` and `Subscriber` are `!Send` types and must live on dedicated IPC threads
- Cannot be stored in async contexts or moved across threads
- Communication from async code via channels: `mpsc::Sender<IpcCommand>` → IPC thread

---

### 5. Resource Limits

**Source**: FR-014, FR-017

**Purpose**: Defines strict CPU and memory limits for Docker containers. Enforced via Docker's hard limit mechanism (FR-014).

**Fields**:

| Field | Type | Required | Validation | Description |
|-------|------|----------|------------|-------------|
| `memory_mb` | u64 | ✅ | Min: 128, Max: host available | Memory limit in megabytes |
| `cpu_cores` | f32 | ✅ | Min: 0.1, Max: host CPU count | CPU cores (fractional allowed) |

**Validation Rules**:

```rust
impl ResourceLimits {
    pub fn validate(&self) -> Result<(), ValidationError> {
        // FR-014: Memory must be at least 128MB (minimum for Python runtime)
        if self.memory_mb < 128 {
            return Err(ValidationError::MemoryTooLow {
                requested: self.memory_mb,
                minimum: 128,
            });
        }

        // FR-014: CPU must be at least 0.1 cores
        if self.cpu_cores < 0.1 {
            return Err(ValidationError::CpuTooLow {
                requested: self.cpu_cores,
                minimum: 0.1,
            });
        }

        // Validate against host limits
        let host_cpu_count = num_cpus::get() as f32;
        if self.cpu_cores > host_cpu_count {
            return Err(ValidationError::CpuExceedsHost {
                requested: self.cpu_cores,
                available: host_cpu_count,
            });
        }

        // Memory limit should not exceed host available memory
        // (Implementation would check system available memory here)

        Ok(())
    }

    pub fn to_docker_host_config(&self) -> bollard::models::HostConfig {
        bollard::models::HostConfig {
            memory: Some(self.memory_mb as i64 * 1_048_576), // Convert MB to bytes
            nano_cpus: Some((self.cpu_cores * 1_000_000_000.0) as i64), // Convert cores to nano CPUs
            ..Default::default()
        }
    }
}
```

**Example**:
```rust
ResourceLimits {
    memory_mb: 2048,  // 2GB
    cpu_cores: 2.0,   // 2 CPU cores
}
```

---

## Shared Container Registry

**Source**: FR-012, FR-015

**Purpose**: Global registry tracking which containers are shared across sessions. Maps `node_id` → `ContainerSessionInstance`.

**Structure**:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

pub type ContainerRegistry = Arc<RwLock<HashMap<String, ContainerSessionInstance>>>;

lazy_static! {
    pub static ref GLOBAL_CONTAINER_REGISTRY: ContainerRegistry =
        Arc::new(RwLock::new(HashMap::new()));
}

impl ContainerRegistry {
    pub async fn get_or_create(
        &self,
        node_id: &str,
        config: &DockerizedNodeConfiguration,
    ) -> Result<ContainerSessionInstance> {
        let mut registry = self.write().await;

        if let Some(container) = registry.get(node_id) {
            // FR-012: Reuse existing container
            tracing::info!("Reusing existing container for node {}", node_id);
            return Ok(container.clone());
        }

        // Create new container
        let container = create_new_container(config).await?;
        registry.insert(node_id.to_string(), container.clone());

        Ok(container)
    }

    pub async fn remove_session_from_container(
        &self,
        node_id: &str,
        session_id: &str,
    ) -> Result<bool> {
        let mut registry = self.write().await;

        if let Some(container) = registry.get_mut(node_id) {
            let should_stop = container.remove_session(session_id).await?;

            if should_stop {
                // FR-015: No more references, remove from registry
                registry.remove(node_id);
                tracing::info!("Removed container {} from registry (no more sessions)", node_id);
                return Ok(true);
            }
        }

        Ok(false)
    }
}
```

---

## Summary

### Entity Count

- **5 core entities**: Dockerized Node Configuration, Container Image Cache Entry, Container Session Instance, iceoryx2 IPC Channel, Resource Limits
- **1 global registry**: Shared Container Registry

### Key Relationships

1. **Pipeline Session** → N **Dockerized Node Configurations**
2. **Dockerized Node Configuration** → 1 **Container Image Cache Entry** (via config_hash)
3. **Container Image Cache Entry** → N **Container Session Instances**
4. **Container Session Instance** → N **iceoryx2 IPC Channels**
5. **Container Session Instance** ↔ N **Pipeline Sessions** (many-to-many via session_ids)

### Validation Summary

All entities implement validation according to functional requirements:

- **FR-004**: Node configuration validated (executor type, Python version, dependencies)
- **FR-010**: IPC channel names follow session-scoped format
- **FR-012**: Container sharing via registry with session tracking
- **FR-013**: Python version and base image validation
- **FR-014**: Resource limits validated and enforced
- **FR-015**: Reference counting with cleanup on zero references
- **FR-016**: Custom base image validation (separate function)

### Persistence

- **SQLite**: Container Image Cache Entry
- **In-Memory**: All other entities (tied to runtime process lifecycle)
- **Cleanup**: Session termination removes channels, decrements container references

This data model provides a complete foundation for implementing the Docker-based node execution feature while maintaining consistency with existing multiprocess executor patterns.
