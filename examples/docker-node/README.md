# Docker Multiprocess Pipeline Examples

This directory contains example pipeline configurations demonstrating Docker execution through the integrated multiprocess system.

## Overview

Docker support is now fully integrated into the multiprocess executor. Python nodes can run either in regular processes or Docker containers, both using the same iceoryx2 IPC for zero-copy data transfer.

**Use Cases**:
- Python package version conflicts between nodes
- Nodes requiring different system libraries
- Resource isolation and limits per node
- Reproducible environments across deployments

## Prerequisites

1. **Docker daemon** installed and running:
   ```bash
   docker --version
   # Expected: Docker version 20.10.0+
   ```

2. **Build base images** (one-time setup):
   ```bash
   cd ../../docker/scripts
   ./build-base-images.sh
   ```

   This creates:
   - `remotemedia/python-node:3.9`
   - `remotemedia/python-node:3.10`
   - `remotemedia/python-node:3.11`

3. **Linux x86_64 host** (Windows/macOS not supported in MVP)

4. **iceoryx2 setup**:
   ```bash
   sudo mkdir -p /tmp/iceoryx2
   sudo chmod 777 /tmp/iceoryx2
   ```

## Files in This Directory

- **[custom_node.py](custom_node.py)**: Example Python nodes (EchoNode, AudioAmplifierNode, TextUppercaseNode)
- **[simple_docker_test.json](simple_docker_test.json)**: Minimal test pipeline for Docker execution
- **[docker_multiprocess_pipeline.json](docker_multiprocess_pipeline.json)**: Basic Docker pipeline with echo processing
- **[mixed_execution_pipeline.json](mixed_execution_pipeline.json)**: Mixed Docker and native multiprocess nodes
- **[advanced_docker_pipeline.json](advanced_docker_pipeline.json)**: Advanced features with GPU support and custom images
- **[manifest.yaml](manifest.yaml)**: Full pipeline with mixed executors
- **[README.md](README.md)**: This file

## Example Manifests

### Simple Docker Test ([simple_docker_test.json](simple_docker_test.json))

Minimal example with a single Docker-based echo node:

```json
{
  "nodes": [
    {
      "id": "echo_container",
      "node_type": "test_echo.EchoNode",
      "executor": "multiprocess",
      "metadata": {
        "use_docker": true,
        "docker_config": {
          "python_version": "3.10",
          "python_packages": ["iceoryx2"],
          "memory_mb": 128,
          "cpu_cores": 0.5,
          "shm_size_mb": 512
        }
      }
    }
  ]
}
```

**Key Points**:
- `executor: "multiprocess"` with `use_docker: true` enables Docker execution
- Configuration in `metadata.docker_config`
- `python_version` must be 3.9, 3.10, or 3.11
- Resource limits are strictly enforced
- `iceoryx2` package required for IPC

### Full Pipeline ([manifest.yaml](manifest.yaml))

Complete audio transcription pipeline mixing native Rust nodes, multiprocess Python nodes, and Docker-based Python nodes:

**Pipeline Flow**:
```
Audio Input (48kHz)
    ↓
Native Rust Resampler (48kHz → 16kHz)
    ↓
Native Rust VAD (Silero)
    ↓
Docker Python Transcription (OmniASR) ← Isolated environment
    ↓
Multiprocess Python Postprocessing
```

**Docker Node Configuration (New Integrated Format)**:
```json
{
  "id": "transcribe_docker",
  "node_type": "OmniASRNode",
  "executor": "multiprocess",
  "metadata": {
    "use_docker": true,
    "docker_config": {
      "python_version": "3.10",
      "system_packages": ["ffmpeg", "libsndfile1"],
      "python_packages": [
        "numpy==1.24.0",
        "torch>=2.0.0",
        "omnilingual_asr==0.1.0",
        "iceoryx2"
      ],
      "memory_mb": 4096,
      "cpu_cores": 2.0,
      "shm_size_mb": 2048,
      "env_vars": {
        "PYTHONUNBUFFERED": "1",
        "MODEL_PATH": "/models/omnilingual"
      },
      "gpu_devices": ["0"],
      "volumes": [
        {
          "host_path": "/models",
          "container_path": "/models",
          "read_only": true
        }
      ]
    }
  }
}
```

## Running the Examples

### Option 1: Via gRPC Transport (Recommended)

```bash
# Terminal 1: Start gRPC server
cd transports/grpc
cargo run --bin grpc_server --release

# Terminal 2: Run client with Docker manifest
# (Client implementation needed - see below)
```

### Option 2: Direct Executor Usage (For Testing)

```rust
use remotemedia_runtime_core::python::docker::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load manifest
    let manifest = std::fs::read_to_string("examples/docker-node/manifest.yaml")?;
    let parsed: serde_json::Value = serde_yaml::from_str(&manifest)?;

    // 2. Extract docker node config
    let docker_config = DockerExecutorConfig {
        python_version: "3.10".to_string(),
        python_packages: vec!["iceoryx2".to_string()],
        resource_limits: ResourceLimits {
            memory_mb: 512,
            cpu_cores: 0.5,
            gpu_devices: vec![],
        },
        ..Default::default()
    };

    let node_config = DockerizedNodeConfiguration::new(
        "echo_docker".to_string(),
        docker_config,
    );

    // 3. Create and initialize executor
    let mut executor = DockerExecutor::new(node_config, None)?;
    executor.initialize("test_session_001".to_string()).await?;

    // 4. Execute (send test data)
    // ... (requires RuntimeData and IPC setup)

    // 5. Cleanup
    executor.cleanup().await?;

    Ok(())
}
```

## Configuration Reference

### Docker Field Schema

```yaml
docker:
  # Required: Python version (3.9, 3.10, or 3.11)
  python_version: "3.10"

  # Optional: System packages (installed via apt-get)
  system_dependencies:
    - "ffmpeg"
    - "libsndfile1"
    - "libsox-fmt-all"

  # Optional: Python packages (installed via pip)
  python_packages:
    - "numpy==1.24.0"      # Pin versions for reproducibility
    - "torch>=2.0.0"
    - "iceoryx2"           # Required for IPC

  # Required: Resource limits (strictly enforced)
  resource_limits:
    memory_mb: 2048        # Minimum: 128MB
    cpu_cores: 1.0         # Minimum: 0.1 cores

  # Optional: Custom base image
  base_image: "myorg/custom-python:3.10"

  # Optional: Environment variables
  env:
    PYTHONUNBUFFERED: "1"
    MODEL_PATH: "/models/my-model"

  # Optional: Volume mounts (beyond default iceoryx2 mounts)
  volumes:
    - host_path: "/data/models"
      container_path: "/models"
      read_only: true
```

### Automatic Configuration

**Docker executor automatically provides**:
- `/tmp/iceoryx2:/tmp/iceoryx2` mount (FR-005)
- `/dev/shm:/dev/shm` mount (FR-005)
- 2GB shared memory size
- Non-root user (UID 1000)
- Signal handling (SIGTERM → SIGKILL)

## Monitoring

### Check Running Containers

```bash
# List RemoteMedia containers
docker ps --filter "name=remotemedia_"

# Expected output:
# CONTAINER ID   IMAGE                          STATUS
# abc123...      remotemedia/python-node:3.10   Up 2 minutes
```

### View Container Logs

```bash
# Get container name from docker ps
docker logs remotemedia_session123_transcribe_docker -f
```

### Monitor Resource Usage

```bash
# Real-time stats
docker stats --filter "name=remotemedia_"

# Expected:
# NAME              CPU %   MEM USAGE / LIMIT   MEM %
# remotemedia_...   45%     1.2GB / 4GB         30%
```

## Troubleshooting

### Error: "Docker daemon not accessible"

**Solution**:
```bash
# Start Docker daemon
sudo systemctl start docker

# Verify connection
docker ps
```

### Error: "Image not found"

**Solution**: Build base images first
```bash
cd docker/scripts
./build-base-images.sh
```

### Error: "iceoryx2 service not found"

**Solution**: Check volume mounts
```bash
docker inspect <container_id> | grep iceoryx
# Should show /tmp/iceoryx2 mount

# Verify permissions
ls -ld /tmp/iceoryx2
# Should be rwxrwxrwx
```

### Container Exits Immediately

**Debug**:
```bash
# Check exit code
docker ps -a | grep remotemedia_

# View logs
docker logs <container_id>

# Exit code 137 = OOMKilled (memory too low)
# Exit code 1 = Python error
```

## Performance Characteristics

Based on design specifications (spec.md):

- **Latency**: Within 5ms of multiprocess nodes (SC-001)
- **Startup**: <5s for cached images (SC-005)
- **Memory**: Zero-copy via iceoryx2 shared memory (SC-003)
- **Isolation**: Full process-level isolation
- **Resource Limits**: Strictly enforced via Docker

## Comparison: Docker vs Multiprocess

| Feature | Multiprocess | Docker |
|---------|-------------|--------|
| **Environment Isolation** | Python version only | Full OS-level |
| **System Dependencies** | Must install on host | Isolated per container |
| **Resource Limits** | OS-level (cgroups) | Docker enforced (strict) |
| **Startup Time** | ~100ms | ~3-5s (cached image) |
| **IPC Performance** | iceoryx2 (zero-copy) | iceoryx2 (zero-copy) |
| **Use Case** | Same Python version OK | Different Python/deps needed |

## Next Steps

1. **Create Python Node** for testing:
   ```python
   # custom_node.py
   from remotemedia.nodes import MultiprocessNode, RuntimeData

   class EchoNode(MultiprocessNode):
       async def process(self, data: RuntimeData):
           # Echo data back
           yield data
   ```

2. **Run Integration Test**:
   - Create test with manifest.yaml
   - Send audio data
   - Verify Docker container created
   - Verify iceoryx2 IPC working
   - Measure latency

3. **Production Deployment**:
   - Build base images in production environment
   - Configure resource limits appropriately
   - Monitor container health
   - Set up log aggregation

## References

- **Specification**: [../../specs/009-docker-node-execution/spec.md](../../specs/009-docker-node-execution/spec.md)
- **Quickstart Guide**: [../../specs/009-docker-node-execution/quickstart.md](../../specs/009-docker-node-execution/quickstart.md)
- **Implementation Status**: [../../specs/009-docker-node-execution/MVP_COMPLETE.md](../../specs/009-docker-node-execution/MVP_COMPLETE.md)
- **Architecture**: [../../specs/009-docker-node-execution/plan.md](../../specs/009-docker-node-execution/plan.md)

---

**💡 Tip**: Start with `simple_docker_node.json` to test basic Docker executor functionality before moving to the full transcription pipeline.
