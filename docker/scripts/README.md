# Docker Build Scripts

Scripts for building RemoteMedia Docker node images with full Python package support.

## Quick Start

### Linux/macOS

```bash
# Build Python 3.10 image (default)
./docker/scripts/build-remotemedia-node.sh

# Build Python 3.11 image
./docker/scripts/build-remotemedia-node.sh 3.11
```

### Windows

```powershell
# Build Python 3.10 image (default)
.\docker\scripts\build-remotemedia-node.ps1

# Build Python 3.11 image
.\docker\scripts\build-remotemedia-node.ps1 -PythonVersion "3.11"
```

## Manual Build (Alternative)

From workspace root:

```bash
# Build using standalone Dockerfile
docker build -f docker/Dockerfile.remotemedia-node -t remotemedia/python-node:py3.10 .

# Build with specific Python version
docker build -f docker/Dockerfile.remotemedia-node \
  --build-arg PYTHON_VERSION=3.11 \
  -t remotemedia/python-node:py3.11 \
  .
```

## Test the Image

```bash
# Run diagnostic script (shows import status)
docker run --rm remotemedia/python-node:py3.10

# Expected output:
# === RemoteMedia Docker Container Diagnostics ===
# Python: 3.10.x
# Testing imports...
# ✓ remotemedia imported
# ✓ iceoryx2 imported
# ✓ remotemedia.core.multiprocessing.runner available
# ...
# ✓ Container ready for node execution!

# Interactive shell for debugging
docker run --rm -it remotemedia/python-node:py3.10 /bin/bash

# Inside container, verify:
python -c "import remotemedia; print('remotemedia OK')"
python -c "import iceoryx2; print('iceoryx2 OK')"
python -c "from remotemedia.core.multiprocessing.runner import main; print('runner OK')"
pip list | grep remotemedia
```

## What Gets Installed

The Docker image includes:

**System packages**:
- Python 3.10 (or specified version)
- libsndfile1 (audio I/O)
- ffmpeg (audio/video processing)

**Python packages**:
- `remotemedia` (shared protobuf definitions) - from root setup.py
- `remotemedia-client` (SDK with nodes) - from python-client/setup.py
- `iceoryx2` (zero-copy IPC)
- All dependencies from requirements.txt

**Configuration**:
- Non-root user (`remotemedia`, UID 1000)
- `/tmp/iceoryx2` directory (writable)
- Virtual environment at `/opt/venv`
- Multi-stage build (smaller final image)

## Troubleshooting

### Build fails with "remotemedia not found"

Ensure you're running from the workspace root:
```bash
cd /path/to/remotemedia-sdk-webrtc
./docker/scripts/build-remotemedia-node.sh
```

### Container runs but imports fail

Check the build logs for pip install errors:
```bash
docker build -f docker/Dockerfile.remotemedia-node -t test . 2>&1 | grep -i error
```

### Path issues on Windows/WSL

Use the standalone Dockerfile instead of the script:
```bash
docker build -f docker/Dockerfile.remotemedia-node -t remotemedia/python-node:py3.10 .
```

## Integration with Runtime

The runtime automatically builds images when needed. To use pre-built images:

```yaml
nodes:
  - id: my_node
    docker:
      base_image: "remotemedia/python-node:py3.10"  # Use pre-built image
      python_packages: ["torch"]  # Add extra packages
      resource_limits:
        memory_mb: 2048
        cpu_cores: 1.0
```

## Build Performance

**First build**: ~2-5 minutes (downloads base image, installs packages)
**Subsequent builds**: ~30 seconds (uses layer cache)
**Image size**: ~500MB-1GB (multi-stage build reduces size 60%)

## Next Steps

After building the image:

1. **Test with runtime**:
   ```bash
   cd runtime-core
   cargo test test_e2e_simple_docker_pipeline -- --nocapture
   ```

2. **Run example pipeline**:
   ```bash
   cargo run --bin webrtc_server --features grpc-signaling -- \
     --manifest examples/docker-node/simple_docker_node.json
   ```

3. **Verify node execution**:
   - Container should receive data via iceoryx2
   - Python node processes and yields outputs
   - Outputs flow back to host runtime
