# Quickstart: Docker-Based Node Execution

**Feature**: Docker-Based Node Execution with iceoryx2 IPC
**Audience**: Pipeline developers using RemoteMedia SDK
**Date**: 2025-11-11

## Overview

This guide shows you how to run pipeline nodes in isolated Docker containers while maintaining zero-copy data transfer performance via iceoryx2 shared memory IPC.

**Use Cases**:
- Python package version conflicts between nodes
- Nodes requiring different system libraries (ffmpeg, libsndfile, etc.)
- Resource isolation and limits per node
- Reproducible environments across deployments

**Prerequisites**:
- Docker daemon installed and running (`docker --version`)
- Linux x86_64 host (initial implementation)
- RemoteMedia SDK runtime-core 0.4.0+

---

## Quick Example

### 1. Create a Pipeline Manifest with Docker Node

```yaml
# pipeline-with-docker.yaml
version: "v1"
metadata:
  name: "transcription-pipeline"
  description: "Audio transcription with Docker-based OmniASR node"

nodes:
  # Docker-based transcription node
  - id: transcribe
    node_type: OmniASRNode
    is_streaming: true
    docker:
      python_version: "3.10"
      system_dependencies:
        - "ffmpeg"
        - "libsndfile1"
      python_packages:
        - "numpy==1.24.0"
        - "omnilingual_asr==0.1.0"
        - "iceoryx2"  # Required for IPC
      resource_limits:
        memory_mb: 4096  # 4GB
        cpu_cores: 2.0

  # Native Rust VAD node (for comparison)
  - id: vad
    node_type: SileroVAD
    is_streaming: true
    params:
      threshold: 0.5

connections:
  - from: "audio_source"
    to: "vad"
  - from: "vad"
    to: "transcribe"
```

### 2. Run the Pipeline

```bash
# From your application code
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::executor::PipelineExecutor;

#[tokio::main]
async fn main() -> Result<()> {
    // Load manifest
    let manifest = Manifest::from_file("pipeline-with-docker.yaml")?;

    // Create executor (automatically detects Docker nodes)
    let executor = PipelineExecutor::new(manifest)?;

    // Run pipeline
    executor.execute_streaming(audio_stream).await?;

    Ok(())
}
```

The runtime will:
1. **Detect** the `docker` field in the `transcribe` node
2. **Build** a Docker image (or reuse cached image)
3. **Start** a container with iceoryx2 mounts configured
4. **Route** audio data via zero-copy shared memory
5. **Clean up** container when session terminates

---

## Step-by-Step Guide

### Step 1: Verify Docker Setup

```bash
# Check Docker is running
docker --version
# Expected: Docker version 20.10.0+

# Verify Docker daemon accessible
docker ps
# Expected: List of running containers (may be empty)

# Check shared memory size
df -h /dev/shm
# Expected: At least 1-2GB available

# Create iceoryx2 directory (if needed)
sudo mkdir -p /tmp/iceoryx2
sudo chmod 777 /tmp/iceoryx2
```

### Step 2: Create Your Python Node

```python
# my_custom_node.py
from remotemedia.nodes import MultiprocessNode, RuntimeData
import asyncio

class MyCustomNode(MultiprocessNode):
    """
    Custom Python node that runs in Docker container.
    Uses iceoryx2 IPC for zero-copy data transfer with host runtime.
    """

    async def initialize(self):
        """Called once when container starts"""
        print(f"Initializing MyCustomNode in container")
        # Load models, initialize resources, etc.
        self.model = load_my_model()

    async def process(self, data: RuntimeData):
        """
        Process incoming data and yield results.
        This is called for each audio chunk from the host runtime.
        """
        if data.data_type == "audio":
            # Process audio
            result = await self.model.process_audio(data.audio.samples)

            # Yield results (can yield multiple outputs)
            yield RuntimeData(
                data_type="text",
                text={"transcript": result},
                timestamp=data.timestamp
            )

    async def cleanup(self):
        """Called when container stops"""
        print("Cleaning up MyCustomNode")
```

### Step 3: Create Node Manifest Entry

```yaml
nodes:
  - id: my_custom_node
    node_type: MyCustomNode  # Must match class name
    is_streaming: true
    docker:
      # Python version (3.9, 3.10, or 3.11)
      python_version: "3.10"

      # System packages needed (installed via apt-get)
      system_dependencies:
        - "ffmpeg"
        - "libsndfile1"
        # Add any system libs your model needs

      # Python packages (installed via pip)
      python_packages:
        - "numpy==1.24.0"
        - "torch>=2.0.0"
        - "transformers"
        - "iceoryx2"  # REQUIRED for IPC

      # Resource limits (strictly enforced)
      resource_limits:
        memory_mb: 2048  # 2GB - adjust based on your model
        cpu_cores: 1.0   # 1 core

      # Optional: Environment variables
      env:
        PYTHONUNBUFFERED: "1"
        MODEL_PATH: "/models/my-model"

      # Optional: Mount model files from host
      volumes:
        - host_path: "/data/models/my-model"
          container_path: "/models/my-model"
          read_only: true
```

### Step 4: Package Your Node Code

Your node code must be accessible to the container. Options:

**Option A: Include in Docker image** (recommended for production):

```dockerfile
# Custom Dockerfile for your node
FROM python:3.10-slim

# Install iceoryx2 (required)
RUN pip install iceoryx2

# Copy your node code
COPY my_custom_node.py /app/nodes/

# Your manifest would reference this custom image:
docker:
  base_image: "myorg/my-custom-node:latest"
  python_version: "3.10"
  resource_limits:
    memory_mb: 2048
    cpu_cores: 1.0
```

**Option B: Mount from host** (easier for development):

```yaml
docker:
  python_version: "3.10"
  volumes:
    - host_path: "./my_custom_node.py"
      container_path: "/app/nodes/my_custom_node.py"
      read_only: true
  resource_limits:
    memory_mb: 2048
    cpu_cores: 1.0
```

### Step 5: Run and Monitor

```bash
# Run your pipeline
cargo run --bin my_pipeline -- --manifest pipeline.yaml

# Monitor containers (in another terminal)
watch -n 1 'docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Image}}"'

# Check container logs
docker logs <container_name> -f

# Monitor resource usage
docker stats

# Check shared memory usage
df -h /dev/shm
```

---

## Common Scenarios

### Scenario 1: Different PyTorch Versions Per Node

**Problem**: Node A needs PyTorch 1.x, Node B needs PyTorch 2.x

**Solution**: Run each in separate Docker containers

```yaml
nodes:
  - id: node_a
    node_type: LegacyModel
    docker:
      python_version: "3.9"
      python_packages:
        - "torch==1.13.1"
      resource_limits:
        memory_mb: 2048
        cpu_cores: 1.0

  - id: node_b
    node_type: ModernModel
    docker:
      python_version: "3.11"
      python_packages:
        - "torch==2.1.0"
      resource_limits:
        memory_mb: 2048
        cpu_cores: 1.0
```

Both nodes run simultaneously in isolated containers, sharing data via iceoryx2.

### Scenario 2: Mixing Docker and Multiprocess Nodes

**Problem**: Want some nodes in Docker, some as host processes

**Solution**: Omit `docker` field for multiprocess execution

```yaml
nodes:
  # Docker node (isolated environment)
  - id: transcribe
    node_type: HeavyModel
    docker:
      python_version: "3.10"
      python_packages:
        - "torch==2.0.0"
      resource_limits:
        memory_mb: 4096
        cpu_cores: 2.0

  # Multiprocess node (host Python environment)
  - id: postprocess
    node_type: SimpleTextCleanup
    # No docker field = uses multiprocess executor
    runtime_hint: "cpython"
    params:
      rules: ["lowercase", "trim"]

  # Native Rust node (fastest)
  - id: vad
    node_type: SileroVAD
    # Rust nodes run natively, no Python involved
```

### Scenario 3: Custom Base Image with Pre-installed Models

**Problem**: Model files are large (5GB+), don't want to download every time

**Solution**: Bake models into custom Docker image

```dockerfile
# my-model-image.Dockerfile
FROM python:3.10-slim

# Install iceoryx2 (required)
RUN pip install iceoryx2

# Install dependencies
RUN pip install torch transformers

# Download and cache model (happens once at build time)
RUN python -c "from transformers import AutoModel; AutoModel.from_pretrained('facebook/wav2vec2-base')"

# Copy node code
COPY transcribe_node.py /app/nodes/
```

Build and use:

```bash
# Build image once
docker build -t myorg/transcribe-node:v1 -f my-model-image.Dockerfile .

# Reference in manifest
nodes:
  - id: transcribe
    node_type: TranscribeNode
    docker:
      base_image: "myorg/transcribe-node:v1"
      python_version: "3.10"  # Must match base image
      resource_limits:
        memory_mb: 6144  # 6GB for model + overhead
        cpu_cores: 2.0
```

### Scenario 4: Debugging Container Issues

**Problem**: Node fails to start or crashes unexpectedly

**Troubleshooting Steps**:

```bash
# 1. Check container logs
docker logs <container_name>

# 2. Check if container is running
docker ps -a | grep <node_id>

# 3. Inspect container configuration
docker inspect <container_name>

# 4. Check resource limits
docker stats <container_name>
# If "OOMKilled" appears, memory_mb too low

# 5. Verify iceoryx2 mounts
docker inspect <container_name> | grep -A 10 Mounts
# Expected: /tmp/iceoryx2 and /dev/shm mounted

# 6. Check iceoryx2 service files
ls -la /tmp/iceoryx2/services/
# Should see files like: {session_id}_{node_id}_input

# 7. Test container manually
docker run -it \
  -v /tmp/iceoryx2:/tmp/iceoryx2 \
  -v /dev/shm:/dev/shm \
  --shm-size=2g \
  <image_name> \
  /bin/bash
```

---

## Performance Tuning

### Memory Sizing

```bash
# Profile actual memory usage
docker stats <container_name>

# Add 20-30% headroom to observed peak
# Example: Observed peak 1.2GB → Set memory_mb: 1536 (1.5GB)
```

### CPU Allocation

```yaml
resource_limits:
  # Fractional CPUs allowed (uses Docker CPU shares)
  cpu_cores: 0.5  # Half a core
  cpu_cores: 1.5  # 1.5 cores
  cpu_cores: 4.0  # 4 full cores
```

### Shared Memory Size

Default `/dev/shm` is 64MB - often too small for audio streaming:

```bash
# Increase shared memory when running containers manually
docker run --shm-size=2g ...

# Or in docker-compose
services:
  node:
    shm_size: '2gb'
```

Runtime automatically sets `--shm-size=2g` for Docker nodes.

### Image Build Speed

**Use layer caching**:

```dockerfile
# Good: Dependencies cached separately from code
COPY requirements.txt .
RUN pip install -r requirements.txt
COPY ./src ./src

# Bad: Code changes invalidate pip install
COPY . .
RUN pip install -r requirements.txt
```

**Use BuildKit cache mounts**:

```dockerfile
RUN --mount=type=cache,target=/root/.cache/pip \
    pip install -r requirements.txt
```

---

## Troubleshooting

### Error: "Docker daemon not accessible"

**Symptoms**: Pipeline fails with "Cannot connect to Docker daemon"

**Solutions**:
```bash
# Start Docker daemon
sudo systemctl start docker

# Add user to docker group (Linux)
sudo usermod -aG docker $USER
# Log out and back in

# Verify connection
docker ps
```

### Error: "Container exits immediately"

**Symptoms**: Container starts but exits within seconds

**Debugging**:
```bash
# Check exit code
docker ps -a | grep <node_id>
# Exit code 137 = OOMKilled (memory too low)
# Exit code 1 = Python error

# Read full logs
docker logs <container_name>

# Try running manually
docker run -it <image_name> python /app/runner.py
```

### Error: "iceoryx2 service not found"

**Symptoms**: "Failed to connect to iceoryx2 channel"

**Solutions**:
```bash
# Verify mounts
docker inspect <container_name> | grep iceoryx

# Check permissions
ls -ld /tmp/iceoryx2
# Should be readable/writable by all

# Verify service files exist
ls -la /tmp/iceoryx2/services/ | grep <session_id>

# Check session ID matches
# Container and host must use same session_id in channel names
```

### Warning: "Container killed (resource limit exceeded)"

**Symptoms**: Container disappears, logs show OOMKilled

**Solution**: Increase `memory_mb` in manifest:

```yaml
resource_limits:
  memory_mb: 4096  # Increase from 2048
  cpu_cores: 2.0
```

---

## Best Practices

### 1. **Pin Package Versions**

```yaml
# Good: Explicit versions ensure reproducibility
python_packages:
  - "numpy==1.24.0"
  - "torch==2.0.1"

# Bad: Floating versions can break between builds
python_packages:
  - "numpy"
  - "torch"
```

### 2. **Set Realistic Resource Limits**

```bash
# Profile first, then set limits with 20-30% headroom
docker stats <container>
# Observed: 1.2GB memory, 0.8 CPU → Set 1.5GB, 1.0 CPU
```

### 3. **Use Standard Base Images When Possible**

```yaml
# Preferred: Use standard python:3.10-slim base
docker:
  python_version: "3.10"  # No base_image specified

# Only use custom base_image if absolutely necessary
```

### 4. **Keep Node Code Small**

- Large model files: Mount as volumes or bake into custom image
- Node code: Keep minimal, only business logic
- Dependencies: Specify in manifest, not in code

### 5. **Monitor Resource Usage**

```bash
# Add to your deployment monitoring
docker stats --format "table {{.Name}}\t{{.MemUsage}}\t{{.CPUPerc}}"
```

---

## Migration from Multiprocess

**Before** (multiprocess executor):
```yaml
nodes:
  - id: my_node
    node_type: MyNode
    runtime_hint: "cpython"
    params:
      model_path: "/models/my-model"
```

**After** (Docker executor):
```yaml
nodes:
  - id: my_node
    node_type: MyNode
    docker:
      python_version: "3.10"
      python_packages:
        - "iceoryx2"
        - "numpy==1.24.0"
      resource_limits:
        memory_mb: 2048
        cpu_cores: 1.0
      volumes:
        - host_path: "/models/my-model"
          container_path: "/models/my-model"
          read_only: true
```

**Key Changes**:
1. Add `docker` field with configuration
2. Move system dependencies to `system_dependencies`
3. Specify `python_packages` explicitly
4. Set `resource_limits`
5. Mount any host resources as `volumes`

---

## Next Steps

- **Read**: [data-model.md](data-model.md) - Understand internal data structures
- **Read**: [research.md](research.md) - Technology decisions and rationale
- **Read**: [contracts/manifest-docker-extension.yaml](contracts/manifest-docker-extension.yaml) - Full manifest schema
- **Example**: `examples/docker-node/` - Complete working example

## Support

For issues or questions:
- GitHub Issues: https://github.com/yourorg/remotemedia-sdk/issues
- Documentation: https://docs.remotemedia.dev/docker-nodes
- Slack: #remotemedia-support
