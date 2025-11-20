# Docker Troubleshooting Guide

Comprehensive troubleshooting guide for Docker-based multiprocess Python node execution in RemoteMedia SDK.

## Table of Contents

1. [Common Docker Issues](#common-docker-issues)
2. [Container Startup Failures](#container-startup-failures)
3. [IPC/iceoryx2 Connection Issues](#ipciceoryx2-connection-issues)
4. [Resource Limit Problems](#resource-limit-problems)
5. [Image Building Failures](#image-building-failures)
6. [Permission Issues](#permission-issues)
7. [Network Connectivity Problems](#network-connectivity-problems)
8. [Container Cleanup Procedures](#container-cleanup-procedures)
9. [Log Analysis Techniques](#log-analysis-techniques)
10. [Performance Debugging](#performance-debugging)

---

## Common Docker Issues

### Docker Daemon Not Running

**Problem:** Cannot connect to Docker daemon.

**Symptoms:**
```
Error: Docker daemon is not responding to ping
Cannot connect to the Docker daemon at unix:///var/run/docker.sock
```

**Diagnosis:**
```bash
# Check if Docker daemon is running
docker info

# Check Docker service status (Linux)
sudo systemctl status docker

# Check Docker daemon logs (Linux)
sudo journalctl -u docker.service -n 50

# macOS - Check Docker Desktop status
open -a Docker
```

**Solutions:**

**Linux:**
```bash
# Start Docker service
sudo systemctl start docker

# Enable Docker to start on boot
sudo systemctl enable docker

# Restart Docker service if it's stuck
sudo systemctl restart docker
```

**macOS/Windows:**
- Open Docker Desktop application
- Wait for Docker to fully initialize (check system tray icon)
- If stuck, quit Docker Desktop and restart it

**Prevention:**
- Set Docker to auto-start on system boot
- Monitor Docker daemon health with health checks
- Use `DockerSupport::validate_docker_availability()` before operations

---

### Permission Denied (Docker Socket)

**Problem:** User lacks permission to access Docker daemon socket.

**Symptoms:**
```
Error: permission denied while trying to connect to the Docker daemon socket
Got permission denied while trying to connect to unix:///var/run/docker.sock
```

**Diagnosis:**
```bash
# Check Docker socket permissions
ls -la /var/run/docker.sock

# Check if user is in docker group
groups $USER
```

**Solutions:**

**Linux:**
```bash
# Add user to docker group
sudo usermod -aG docker $USER

# Apply group changes (requires logout/login)
newgrp docker

# Verify permission
docker ps

# Alternative: Run with sudo (not recommended for production)
sudo docker ps
```

**macOS:**
- Docker Desktop handles permissions automatically
- If issues persist, reinstall Docker Desktop

**Prevention:**
- Add all developers to `docker` group during onboarding
- Document permission requirements in setup guide
- Use rootless Docker for enhanced security

---

### Docker API Version Mismatch

**Problem:** Client API version incompatible with Docker daemon.

**Symptoms:**
```
Error: client version X.XX is too new. Maximum supported API version is Y.YY
```

**Diagnosis:**
```bash
# Check client and server versions
docker version

# Check API versions
docker version --format '{{.Client.APIVersion}} / {{.Server.APIVersion}}'
```

**Solutions:**
```bash
# Set explicit API version
export DOCKER_API_VERSION=1.40

# Upgrade Docker daemon (Linux)
sudo apt-get update
sudo apt-get install docker-ce docker-ce-cli

# Downgrade client if necessary
# (or upgrade daemon to match client)
```

**Code Fix:**
The `DockerSupport` module validates API version 1.40+ (Docker 19.03+). If you encounter version issues:

```rust
// In docker_support.rs, the validation logs warnings but continues
// Minimum required: Docker API 1.40 (Docker 19.03+)
// Upgrade your Docker daemon to meet this requirement
```

---

## Container Startup Failures

### Container Fails to Start

**Problem:** Container created but fails to transition to running state.

**Symptoms:**
```
Container created: abc123def456
Error: Container failed to start within timeout
Container status: Exited (1)
```

**Diagnosis:**
```bash
# Check container status
docker ps -a --filter "label=remotemedia.session_id=<session_id>"

# Inspect container exit code
docker inspect <container_id> | jq '.[0].State'

# View container logs
docker logs <container_id>

# Check last few log lines
docker logs --tail 50 <container_id>
```

**Common Causes & Solutions:**

### 1. Missing Python Dependencies

**Logs show:**
```
ModuleNotFoundError: No module named 'iceoryx2'
ImportError: cannot import name 'MultiprocessNode'
```

**Solution:**
Update `DockerNodeConfig` to include required packages:

```rust
DockerNodeConfig {
    python_packages: vec![
        "iceoryx2".to_string(),
        "remotemedia".to_string(),
        // Add your custom packages
        "numpy".to_string(),
        "torch".to_string(),
    ],
    // ...
}
```

### 2. Image Not Found

**Logs show:**
```
Error: No such image: remotemedia/node:abc123def456
Unable to find image 'remotemedia/node:latest' locally
```

**Solution:**
```bash
# Build image manually
docker build -f docker/Dockerfile.remotemedia-node -t remotemedia/node:latest .

# Or force rebuild in code
let image = builder.build_image(&config, force_rebuild: true).await?;
```

### 3. Entrypoint/CMD Failure

**Logs show:**
```
python: can't open file '/app/runner.py': [Errno 2] No such file or directory
```

**Solution:**
Verify Dockerfile CMD:
```dockerfile
# Correct: Keep container alive
CMD ["tail", "-f", "/dev/null"]

# Or: Use infinite sleep
CMD ["python", "-c", "import asyncio; asyncio.run(__import__('asyncio').sleep(float('inf')))"]
```

For custom nodes, override CMD or use `docker exec` to run Python runner.

---

### Container Stuck in "Created" State

**Problem:** Container never transitions from "Created" to "Running".

**Diagnosis:**
```bash
# Check container state
docker inspect <container_id> | jq '.[0].State.Status'

# Check if waiting for resources
docker inspect <container_id> | jq '.[0].State.Error'
```

**Solutions:**

### 1. Resource Exhaustion
```bash
# Check Docker daemon resources
docker info | grep -E "CPUs|Memory"

# Check system resources
free -h
df -h
```

**Fix:** Reduce container resource limits or free up system resources.

```rust
DockerNodeConfig {
    memory_mb: 512,  // Reduce from 2048
    cpu_cores: 0.5,  // Reduce from 2.0
    // ...
}
```

### 2. Volume Mount Errors
```bash
# Check volume mount issues
docker inspect <container_id> | jq '.[0].Mounts'

# Verify host paths exist
ls -la /tmp
ls -la /dev
```

**Fix:** Ensure IPC volume mount directories exist and are accessible:
```bash
# Create iceoryx2 directory with proper permissions
sudo mkdir -p /tmp/iceoryx2
sudo chmod 777 /tmp/iceoryx2
```

---

### Health Check Failures

**Problem:** Container runs but health check reports unhealthy.

**Diagnosis:**
```bash
# Check health status
docker inspect <container_id> | jq '.[0].State.Health'

# View health check logs
docker inspect <container_id> | jq '.[0].State.Health.Log'
```

**Solution:**
Modify health check in Dockerfile:
```dockerfile
# Less aggressive health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD python -c "import sys; sys.exit(0)" || exit 1
```

Or disable health checks for debugging:
```dockerfile
HEALTHCHECK NONE
```

---

## IPC/iceoryx2 Connection Issues

### iceoryx2 Channel Not Found

**Problem:** Python process cannot connect to iceoryx2 channels.

**Symptoms:**
```python
Error: Service not found: <session_id>_<node_id>_input
iceoryx2.ServiceDoesNotExist: Failed to open service
```

**Diagnosis:**
```bash
# Check if iceoryx2 services exist
ls -la /tmp/iceoryx2/

# Check container volume mounts
docker inspect <container_id> | jq '.[0].Mounts[] | select(.Destination=="/tmp")'

# Verify IPC files on host
find /tmp -name "*iceoryx2*" -ls
```

**Root Causes & Solutions:**

### 1. Volume Mount Missing

**Problem:** Container doesn't have `/tmp` mounted from host.

**Verification:**
```bash
docker inspect <container_id> | grep -A5 "Mounts"
```

**Fix in code (`docker_support.rs`):**
```rust
// Ensure IPC volume mounts are configured
let mut binds = vec![
    "/tmp:/tmp".to_string(),      // For iceoryx2 service files
    "/dev:/dev".to_string(),        // For shared memory
];
host_config.binds = Some(binds);
```

### 2. Service Created After Subscriber

**Problem:** Python tries to subscribe before Rust creates the publisher.

**Logs show:**
```
Failed to open input service on attempt 1
Failed to open input service on attempt 2
...
```

**Solution implemented in `runner.py`:**
```python
# Retry opening with exponential backoff
max_retries = 50
retry_delay = 0.1  # 100ms

for attempt in range(max_retries):
    try:
        input_service = node.service_builder(input_channel_name) \
            .publish_subscribe(iox2.Slice[ctypes.c_uint8]) \
            .open()  # Open existing service
        break
    except Exception as e:
        if attempt < max_retries - 1:
            time.sleep(retry_delay)
```

### 3. Session ID Mismatch

**Problem:** Channel names don't match between Rust and Python.

**Verification:**
```bash
# Check Rust side channel names (in logs)
grep "Creating IPC channel" /var/log/remotemedia.log

# Check Python side channel names
docker logs <container_id> | grep "Opening input channel"
```

**Fix:** Ensure consistent naming:
```rust
// Rust side
let input_channel = format!("{}_{}_input", session_id, node_id);
let output_channel = format!("{}_{}_output", session_id, node_id);
```

```python
# Python side
input_channel_name = f"{session_id}_{node_id}_input"
output_channel_name = f"{session_id}_{node_id}_output"
```

---

### READY Signal Not Received

**Problem:** Rust doesn't receive READY signal from Python process.

**Symptoms:**
```
Waiting for READY signal from node...
Timeout waiting for node initialization
```

**Diagnosis:**
```bash
# Check if Python process started
docker exec <container_id> ps aux | grep python

# Check Python logs for READY signal
docker logs <container_id> | grep "READY"

# Check control channel
ls -la /tmp/iceoryx2/ | grep control
```

**Solutions:**

### 1. Control Channel Name Mismatch
Ensure session-scoped control channel naming:
```python
# Python: Include session_id in control channel
control_service_name = iox2.ServiceName.new(
    f"control/{session_id}_{node_id}"
)
```

```rust
// Rust: Match Python naming
let control_channel = format!("control/{}_{}", session_id, node_id);
```

### 2. Python Process Crashed
```bash
# Check exit status
docker inspect <container_id> | jq '.[0].State.ExitCode'

# Check stderr
docker logs <container_id> 2>&1 | grep -i error
```

**Fix:** Debug Python crash first before troubleshooting IPC.

---

### Data Transfer Stalls

**Problem:** Data sent but not received, or vice versa.

**Symptoms:**
```
Sent data to node, waiting for output...
No samples received from iceoryx2 subscriber
```

**Diagnosis:**
```bash
# Check if both processes are alive
docker exec <container_id> ps aux | grep python
ps aux | grep remotemedia

# Monitor IPC activity
watch -n 1 'ls -la /tmp/iceoryx2/'

# Check buffer sizes
docker logs <container_id> | grep "buffer_size"
```

**Solutions:**

### 1. Buffer Overflow
Increase subscriber buffer size:
```python
# Increase from default 10 to 100
input_subscriber = input_service.subscriber_builder() \
    .buffer_size(100) \
    .create()
```

### 2. History Configuration Mismatch
Enable history on both sides:
```rust
// Rust side: Enable history
let service = node_builder
    .publish_subscribe::<[u8]>()
    .history_size(100)
    .subscriber_max_buffer_size(100)
    .create()?;
```

```python
# Python side: Match history config
input_service = node.service_builder(input_channel_name) \
    .publish_subscribe(iox2.Slice[ctypes.c_uint8]) \
    .history_size(100) \
    .subscriber_max_buffer_size(100) \
    .open_or_create()
```

### 3. Serialization Errors
Check data format:
```bash
# Enable debug logging
export RUST_LOG=debug
docker logs <container_id> | grep "Serialization"
```

Verify serialization format matches between Rust and Python (see `data_transfer.rs`).

---

## Resource Limit Problems

### Out of Memory (OOM) Kills

**Problem:** Container killed due to memory limit.

**Symptoms:**
```
Error: Container exited with code 137
docker inspect shows: "OOMKilled": true
```

**Diagnosis:**
```bash
# Check OOM status
docker inspect <container_id> | jq '.[0].State | {Status, OOMKilled, ExitCode}'

# Check memory usage before crash
docker stats <container_id> --no-stream

# View dmesg for OOM events
dmesg | grep -i oom | tail -20
```

**Solutions:**

### 1. Increase Memory Limit
```rust
DockerNodeConfig {
    memory_mb: 4096,  // Increase from 2048
    // ...
}
```

### 2. Monitor Memory Usage in Real-Time
```rust
// Use DockerSupport to monitor
let stats = docker_support.monitor_resource_usage(&container_id).await?;
info!("Memory: {} MB / {} MB", stats.memory_mb, stats.memory_limit_mb.unwrap_or(0));

// Set up alerting if approaching limit
if stats.memory_mb > (stats.memory_limit_mb.unwrap_or(u64::MAX) * 80 / 100) {
    warn!("Memory usage exceeds 80% of limit!");
}
```

### 3. Increase Shared Memory (`/dev/shm`)
For ML models that use shared memory:
```rust
DockerNodeConfig {
    shm_size_mb: 4096,  // Increase from 2048
    // ...
}
```

Verify in container:
```bash
docker exec <container_id> df -h | grep shm
```

### 4. Optimize Python Memory Usage
```python
# Force garbage collection after processing
import gc
gc.collect()

# Use memory profiling
from memory_profiler import profile

@profile
def process_data(self, data):
    # Your processing code
    pass
```

---

### CPU Throttling

**Problem:** Container experiencing CPU throttling, causing slow processing.

**Symptoms:**
```
Processing time: 5000ms (expected: 100ms)
CPU percent: 100.0 (maxed out)
```

**Diagnosis:**
```bash
# Monitor CPU usage
docker stats <container_id> --no-stream

# Check CPU quota
docker inspect <container_id> | jq '.[0].HostConfig | {NanoCpus, CpuShares}'

# Check system CPU load
top
htop
```

**Solutions:**

### 1. Increase CPU Allocation
```rust
DockerNodeConfig {
    cpu_cores: 4.0,  // Increase from 1.0
    // ...
}
```

### 2. Monitor CPU Usage
```rust
let stats = docker_support.monitor_resource_usage(&container_id).await?;
info!("CPU: {:.2}%", stats.cpu_percent);

// CPU percent can exceed 100% for multi-core systems
// e.g., 200% means using 2 full cores
```

### 3. Profile Python Code
```bash
# Use cProfile
docker exec <container_id> python -m cProfile -o output.prof runner.py

# Analyze profile
docker exec <container_id> python -c "import pstats; pstats.Stats('output.prof').sort_stats('cumtime').print_stats(20)"
```

---

### GPU Allocation Issues

**Problem:** Container cannot access GPU devices.

**Symptoms:**
```
RuntimeError: CUDA not available
No CUDA-capable device is detected
```

**Diagnosis:**
```bash
# Check GPU availability on host
nvidia-smi

# Check if NVIDIA Container Toolkit is installed
dpkg -l | grep nvidia-container-toolkit

# Check container GPU access
docker exec <container_id> nvidia-smi

# Inspect device requests
docker inspect <container_id> | jq '.[0].HostConfig.DeviceRequests'
```

**Solutions:**

### 1. Install NVIDIA Container Toolkit
```bash
# Ubuntu/Debian
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -s -L https://nvidia.github.io/nvidia-docker/gpgkey | sudo apt-key add -
curl -s -L https://nvidia.github.io/nvidia-docker/$distribution/nvidia-docker.list | \
    sudo tee /etc/apt/sources.list.d/nvidia-docker.list

sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit
sudo systemctl restart docker
```

### 2. Configure GPU in DockerNodeConfig
```rust
DockerNodeConfig {
    gpu_devices: vec!["0".to_string(), "1".to_string()],  // Specific GPUs
    // OR
    gpu_devices: vec!["all".to_string()],  // All GPUs
    // ...
}
```

### 3. Use NVIDIA Base Image
```rust
DockerNodeConfig {
    base_image: Some("nvidia/cuda:12.4.0-runtime-ubuntu22.04".to_string()),
    // ...
}
```

### 4. Verify GPU Environment Variables
```bash
docker exec <container_id> env | grep -i nvidia
```

Should show:
```
NVIDIA_VISIBLE_DEVICES=0,1
NVIDIA_DRIVER_CAPABILITIES=compute,utility
```

---

## Image Building Failures

### Build Context Too Large

**Problem:** Docker build times out or fails due to large context.

**Symptoms:**
```
Sending build context to Docker daemon: 5.234GB
Error: context canceled
```

**Diagnosis:**
```bash
# Check context size
du -sh .

# Check what's being sent
docker build --no-cache --progress=plain -f Dockerfile . 2>&1 | head -20
```

**Solutions:**

### 1. Optimize .dockerignore
```bash
# Edit .dockerignore
cat >> .dockerignore << EOF
# Rust build artifacts
target/
debug/
release/

# Python caches
__pycache__/
*.pyc
.pytest_cache/

# Large binaries
*.tar.gz
*.zip
archive/

# Git and specs
.git/
specs/
.specify/
EOF
```

### 2. Use Multi-Stage Builds
Already implemented in `container_builder.rs`:
```dockerfile
# Builder stage: Heavy dependencies
FROM python:3.10-slim AS builder
# ... install deps ...

# Runtime stage: Minimal image
FROM python:3.10-slim
COPY --from=builder /usr/local/lib/python3.10/site-packages /usr/local/lib/python3.10/site-packages
```

---

### Package Installation Failures

**Problem:** pip or apt-get fails during image build.

**Symptoms:**
```
ERROR: Could not find a version that satisfies the requirement torch
E: Unable to locate package libsndfile1-dev
```

**Diagnosis:**
```bash
# Check package availability
docker run --rm python:3.10-slim apt-cache search libsndfile

# Test pip install
docker run --rm python:3.10-slim pip install torch --dry-run
```

**Solutions:**

### 1. Add System Dependencies
```rust
DockerNodeConfig {
    system_packages: vec![
        "build-essential".to_string(),
        "libsndfile1-dev".to_string(),
        "ffmpeg".to_string(),
    ],
    // ...
}
```

### 2. Pin Package Versions
```rust
DockerNodeConfig {
    python_packages: vec![
        "torch==2.5.1".to_string(),  // Pin version
        "numpy>=1.24.0,<2.0.0".to_string(),  // Version range
    ],
    // ...
}
```

### 3. Use Private PyPI Index
```rust
DockerNodeConfig {
    env_vars: hashmap!{
        "PIP_INDEX_URL".to_string() => "https://pypi.org/simple".to_string(),
        "PIP_EXTRA_INDEX_URL".to_string() => "https://download.pytorch.org/whl/cu124".to_string(),
    },
    // ...
}
```

---

### Build Hangs or Times Out

**Problem:** Docker build hangs indefinitely.

**Diagnosis:**
```bash
# Build with verbose output
docker build --progress=plain --no-cache -f Dockerfile .

# Check Docker daemon logs
sudo journalctl -u docker.service -f

# Check if network is accessible
docker run --rm alpine ping -c 3 pypi.org
```

**Solutions:**

### 1. Increase Build Timeout
```bash
# Set longer timeout
export DOCKER_CLIENT_TIMEOUT=300
export COMPOSE_HTTP_TIMEOUT=300
```

### 2. Use BuildKit
```bash
# Enable BuildKit for better caching
export DOCKER_BUILDKIT=1
docker build -f Dockerfile .
```

### 3. Check Network Proxy
```bash
# If behind corporate proxy
docker build --build-arg HTTP_PROXY=http://proxy:port \
    --build-arg HTTPS_PROXY=http://proxy:port \
    -f Dockerfile .
```

---

### Image Cache Issues

**Problem:** Cached layers contain stale dependencies.

**Symptoms:**
```
Using cached layer from previous build
ModuleNotFoundError: No module named 'newly_added_package'
```

**Solutions:**

### 1. Force Rebuild Without Cache
```rust
// In code
let image = builder.build_image(&config, force_rebuild: true).await?;
```

```bash
# CLI
docker build --no-cache -f Dockerfile .
```

### 2. Clear Image Cache
```rust
// Clear all cached images
builder.clear_cache().await;
```

```bash
# Remove all RemoteMedia images
docker images | grep remotemedia | awk '{print $3}' | xargs docker rmi -f

# Prune unused images
docker image prune -a -f
```

### 3. Verify Cache Statistics
```rust
let (count, size, max_size) = builder.cache_stats().await;
info!("Cache: {} images, {} bytes / {} bytes", count, size, max_size);

// Manually evict if needed
if size > max_size * 90 / 100 {
    builder.clear_cache().await;
}
```

---

## Permission Issues

### Volume Mount Permission Errors

**Problem:** Container cannot read/write to mounted volumes.

**Symptoms:**
```
PermissionError: [Errno 13] Permission denied: '/tmp/iceoryx2/service_xyz'
OSError: [Errno 13] Permission denied: '/dev/shm'
```

**Diagnosis:**
```bash
# Check volume permissions
docker exec <container_id> ls -la /tmp/iceoryx2/
docker exec <container_id> ls -la /dev/shm/

# Check container user
docker exec <container_id> whoami
docker exec <container_id> id
```

**Solutions:**

### 1. Fix Host Directory Permissions
```bash
# Create and set permissions on host
sudo mkdir -p /tmp/iceoryx2
sudo chmod 777 /tmp/iceoryx2

# For shared memory
sudo chmod 777 /dev/shm
```

### 2. Run Container as Root
```dockerfile
# In Dockerfile
USER root
```

Or in container config:
```bash
docker run --user root <image>
```

### 3. Use --privileged (Last Resort)
```bash
# Only for debugging, NOT for production
docker run --privileged <image>
```

**Better Solution:** Fix permissions properly instead of using privileged mode.

---

### Docker Socket Permission Errors

**Problem:** Container trying to access host Docker socket.

**Symptoms:**
```
Got permission denied while trying to connect to the Docker daemon socket
```

**Solution:**
This typically means you're trying to run Docker-in-Docker. For RemoteMedia nodes, this shouldn't be necessary. If you need it:

```bash
# Mount Docker socket
docker run -v /var/run/docker.sock:/var/run/docker.sock <image>

# Add user to docker group in container
docker exec <container_id> adduser <user> docker
```

---

## Network Connectivity Problems

### DNS Resolution Failures

**Problem:** Container cannot resolve hostnames.

**Symptoms:**
```
Could not resolve host: pypi.org
NameResolutionError: [Errno -2] Name or service not known
```

**Diagnosis:**
```bash
# Test DNS inside container
docker exec <container_id> nslookup pypi.org
docker exec <container_id> ping -c 3 8.8.8.8

# Check container DNS config
docker inspect <container_id> | jq '.[0].HostConfig.Dns'
```

**Solutions:**

### 1. Configure DNS Servers
```bash
# Add DNS to Docker daemon config
cat >> /etc/docker/daemon.json << EOF
{
  "dns": ["8.8.8.8", "8.8.4.4"]
}
EOF

sudo systemctl restart docker
```

### 2. Use Host Network (Temporary)
```bash
# Run container with host networking
docker run --network host <image>
```

**Note:** Breaks isolation, use only for debugging.

---

### Proxy Configuration

**Problem:** Container behind corporate proxy cannot access external resources.

**Diagnosis:**
```bash
# Check if proxy needed
echo $HTTP_PROXY
echo $HTTPS_PROXY

# Test from container
docker exec <container_id> curl -v https://pypi.org
```

**Solutions:**

### 1. Configure Docker Daemon Proxy
```bash
# Create systemd drop-in directory
sudo mkdir -p /etc/systemd/system/docker.service.d

# Add proxy configuration
cat > /etc/systemd/system/docker.service.d/http-proxy.conf << EOF
[Service]
Environment="HTTP_PROXY=http://proxy.example.com:8080"
Environment="HTTPS_PROXY=http://proxy.example.com:8080"
Environment="NO_PROXY=localhost,127.0.0.1"
EOF

# Reload and restart
sudo systemctl daemon-reload
sudo systemctl restart docker
```

### 2. Pass Proxy to Container
```rust
DockerNodeConfig {
    env_vars: hashmap!{
        "HTTP_PROXY".to_string() => "http://proxy:8080".to_string(),
        "HTTPS_PROXY".to_string() => "http://proxy:8080".to_string(),
        "NO_PROXY".to_string() => "localhost,127.0.0.1".to_string(),
    },
    // ...
}
```

---

## Container Cleanup Procedures

### Manual Container Cleanup

**Problem:** Orphaned containers consuming resources.

**List all RemoteMedia containers:**
```bash
# List by label
docker ps -a --filter "label=remotemedia.session_id"

# List by name pattern
docker ps -a | grep remotemedia
```

**Stop and remove:**
```bash
# Stop all RemoteMedia containers
docker ps -a --filter "label=remotemedia.session_id" -q | xargs docker stop

# Remove all stopped RemoteMedia containers
docker ps -a --filter "label=remotemedia.session_id" -q | xargs docker rm -f
```

---

### Automated Cleanup in Code

**Use DockerSupport cleanup:**
```rust
// Cleanup specific session
let removed = docker_support.cleanup_session_containers(&session_id).await?;
info!("Cleaned up {} containers for session {}", removed.len(), session_id);

// Cleanup all stopped containers
docker system prune -f

// Cleanup with filter
docker container prune --filter "label=remotemedia.session_id" -f
```

---

### Force Remove Stuck Containers

**Problem:** Container won't stop or remove.

```bash
# Kill container process
docker kill <container_id>

# Force remove
docker rm -f <container_id>

# If still stuck, restart Docker daemon
sudo systemctl restart docker
```

---

### Clean Up Docker System Resources

**Problem:** Docker consuming too much disk space.

**Diagnosis:**
```bash
# Check Docker disk usage
docker system df

# Detailed breakdown
docker system df -v
```

**Cleanup:**
```bash
# Remove unused containers
docker container prune -f

# Remove unused images
docker image prune -a -f

# Remove unused volumes
docker volume prune -f

# Remove unused networks
docker network prune -f

# Nuclear option: Remove everything
docker system prune -a --volumes -f
```

**Selective cleanup:**
```bash
# Remove RemoteMedia images only
docker images | grep remotemedia | awk '{print $3}' | xargs docker rmi -f

# Remove old images (older than 7 days)
docker image prune -a --filter "until=168h" -f
```

---

## Log Analysis Techniques

### Accessing Container Logs

**View logs:**
```bash
# Follow logs in real-time
docker logs -f <container_id>

# Last 100 lines
docker logs --tail 100 <container_id>

# Logs since timestamp
docker logs --since 2024-01-01T10:00:00 <container_id>

# Both stdout and stderr
docker logs <container_id> 2>&1

# Only stderr
docker logs <container_id> 2>&1 >/dev/null
```

**Using DockerSupport in code:**
```rust
// Get logs programmatically
let logs = docker_support.get_container_logs(&container_id, Some(100)).await?;
info!("Container logs:\n{}", logs);
```

---

### Log Patterns to Look For

**Successful initialization:**
```
Initialized node: <node_id> (<node_type>)
✅ Sent READY signal via iceoryx2 control channel
✅ Input subscriber created successfully with history enabled
```

**IPC connection issues:**
```
Failed to open input service on attempt X
Service not found: <channel_name>
Timeout waiting for READY signal
```

**Memory issues:**
```
MemoryError: Unable to allocate array
killed by signal 9 (SIGKILL)
OOMKilled: true
```

**Python errors:**
```
ModuleNotFoundError: No module named 'X'
ImportError: cannot import name 'Y'
Traceback (most recent call last):
```

---

### Structured Log Analysis

**Filter by log level:**
```bash
# Errors only
docker logs <container_id> 2>&1 | grep -i error

# Warnings and errors
docker logs <container_id> 2>&1 | grep -iE "error|warning"

# Debug info
docker logs <container_id> 2>&1 | grep -i debug
```

**Extract specific information:**
```bash
# Find session ID
docker logs <container_id> | grep -oP 'session_id: \K[^\s]+'

# Find processing times
docker logs <container_id> | grep -oP 'processing time: \K[0-9]+ ms'

# Find memory usage
docker logs <container_id> | grep -oP 'memory: \K[0-9]+ MB'
```

---

### Save Logs for Analysis

```bash
# Save to file
docker logs <container_id> > container.log 2>&1

# Compress old logs
docker logs <container_id> 2>&1 | gzip > container-$(date +%Y%m%d).log.gz

# Stream to logging system
docker logs -f <container_id> 2>&1 | logger -t remotemedia-docker
```

---

### Docker Daemon Logs

**Check Docker daemon logs:**
```bash
# Linux systemd
sudo journalctl -u docker.service -f

# Last 100 lines
sudo journalctl -u docker.service -n 100

# Since specific time
sudo journalctl -u docker.service --since "2024-01-01 10:00:00"

# Docker log file (if not using systemd)
tail -f /var/log/docker.log
```

---

## Performance Debugging

### Monitor Resource Usage

**Real-time monitoring:**
```bash
# Basic stats
docker stats <container_id>

# Continuous monitoring
watch -n 1 'docker stats <container_id> --no-stream'

# All RemoteMedia containers
docker stats $(docker ps --filter "label=remotemedia.session_id" -q)
```

**In code:**
```rust
// Monitor resource usage
let stats = docker_support.monitor_resource_usage(&container_id).await?;
info!("CPU: {:.2}%, Memory: {} MB / {} MB",
    stats.cpu_percent,
    stats.memory_mb,
    stats.memory_limit_mb.unwrap_or(0)
);

// Set up periodic monitoring
let mut interval = tokio::time::interval(Duration::from_secs(5));
loop {
    interval.tick().await;
    let stats = docker_support.monitor_resource_usage(&container_id).await?;

    // Alert if high usage
    if stats.cpu_percent > 90.0 {
        warn!("High CPU usage: {:.2}%", stats.cpu_percent);
    }
    if stats.memory_mb > (stats.memory_limit_mb.unwrap_or(u64::MAX) * 90 / 100) {
        warn!("High memory usage: {} MB", stats.memory_mb);
    }
}
```

---

### Profile Container Performance

**CPU profiling:**
```bash
# Install profiling tools in container
docker exec <container_id> pip install py-spy

# Profile running Python process
docker exec <container_id> py-spy top --pid 1

# Generate flamegraph
docker exec <container_id> py-spy record -o profile.svg --pid 1 --duration 30
docker cp <container_id>:/app/profile.svg .
```

**Memory profiling:**
```bash
# Install memory profiler
docker exec <container_id> pip install memory-profiler

# Profile specific function
docker exec <container_id> python -m memory_profiler runner.py
```

**I/O profiling:**
```bash
# Monitor I/O stats
docker stats <container_id> --format "table {{.Container}}\t{{.BlockIO}}\t{{.NetIO}}"

# Inside container
docker exec <container_id> iotop
```

---

### Benchmark IPC Throughput

**Measure data transfer performance:**

```python
# In Python node
import time
import asyncio

async def benchmark_ipc(self, num_iterations=1000):
    start = time.time()

    for i in range(num_iterations):
        # Receive data
        data = await self.receive_data()

        # Echo back
        await self.send_data(data)

    elapsed = time.time() - start
    throughput = num_iterations / elapsed

    print(f"IPC Throughput: {throughput:.2f} messages/sec")
    print(f"Latency: {elapsed/num_iterations*1000:.2f} ms/message")
```

**Rust side:**
```rust
// Measure round-trip time
let start = Instant::now();

for _ in 0..1000 {
    // Send data
    send_data_to_node(&node_id, &session_id, &data).await?;

    // Receive response
    let response = receive_output(&node_id, &session_id).await?;
}

let elapsed = start.elapsed();
info!("Round-trip time: {:?} per message", elapsed / 1000);
```

---

### Optimize Build Times

**Use BuildKit cache mounts:**
```dockerfile
# In Dockerfile
RUN --mount=type=cache,target=/root/.cache/pip \
    pip install --no-cache-dir -r requirements.txt
```

**Parallel builds:**
```bash
# Build multiple images concurrently
docker build --build-arg PYTHON_VERSION=3.9 -t remotemedia:py39 . &
docker build --build-arg PYTHON_VERSION=3.10 -t remotemedia:py310 . &
docker build --build-arg PYTHON_VERSION=3.11 -t remotemedia:py311 . &
wait
```

**Layer ordering optimization:**
```dockerfile
# Copy requirements first (changes less frequently)
COPY requirements.txt .
RUN pip install -r requirements.txt

# Copy source code last (changes frequently)
COPY . .
```

---

### Debug Slow Container Startup

**Measure initialization stages:**
```bash
# Enable timing in logs
export RUST_LOG=debug,remotemedia_runtime_core::python::multiprocess=trace

# Track initialization phases
docker logs <container_id> 2>&1 | grep -E "Initialization|READY|Created"
```

**Breakdown:**
1. Image pull: `docker pull` time
2. Container creation: `DockerSupport::create_container`
3. Container start: `DockerSupport::start_container`
4. Python startup: Time until READY signal
5. IPC setup: Channel creation time
6. Model loading: Node initialization time

**Optimization tips:**
- Pre-pull images: `docker pull remotemedia/node:latest`
- Use image cache: Avoid `force_rebuild`
- Optimize Dockerfile: Use multi-stage builds
- Reduce model download time: Mount model cache volume

---

## Prevention and Best Practices

### Proactive Monitoring

**Health checks:**
```rust
// Periodic health checks
async fn monitor_container_health(docker_support: &DockerSupport, container_id: &str) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        // Check if running
        match docker_support.is_container_running(container_id).await {
            Ok(true) => {
                // Monitor resources
                if let Ok(stats) = docker_support.monitor_resource_usage(container_id).await {
                    info!("Container health: CPU {:.2}%, Memory {} MB",
                        stats.cpu_percent, stats.memory_mb);
                }
            }
            Ok(false) => {
                error!("Container {} stopped unexpectedly", container_id);
                break;
            }
            Err(e) => {
                error!("Health check failed: {}", e);
            }
        }
    }
}
```

---

### Graceful Degradation

**Handle Docker unavailability:**
```rust
// Check Docker before using it
match docker_support.check_daemon_ready().await {
    Ok(_) => {
        // Use Docker execution
        let executor = DockerExecutor::new(config).await?;
    }
    Err(e) => {
        warn!("Docker unavailable: {}. Falling back to native execution.", e);
        // Use native Python executor instead
        let executor = MultiprocessExecutor::new(config).await?;
    }
}
```

---

### Documentation

Always document:
1. Required Docker version
2. System dependencies (NVIDIA Container Toolkit for GPU)
3. Volume mount requirements
4. Resource recommendations
5. Known issues and workarounds

---

## Additional Resources

### Official Documentation

- [Docker Documentation](https://docs.docker.com/)
- [iceoryx2 Documentation](https://iceoryx.io/)
- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/)

### RemoteMedia SDK Documentation

- [CLAUDE.md](/home/acidhax/dev/personal/remotemedia-sdk/CLAUDE.md) - Architecture overview
- [NATIVE_ACCELERATION.md](/home/acidhax/dev/personal/remotemedia-sdk/docs/NATIVE_ACCELERATION.md) - Rust acceleration
- [PERFORMANCE_TUNING.md](/home/acidhax/dev/personal/remotemedia-sdk/docs/PERFORMANCE_TUNING.md) - Optimization guide

### Source Code References

- `runtime-core/src/python/multiprocess/docker_support.rs` - Docker integration
- `runtime-core/src/python/multiprocess/container_builder.rs` - Image building
- `python-client/remotemedia/core/multiprocessing/runner.py` - Python node runner
- `docker/Dockerfile.pytorch-node` - PyTorch base image

### Testing

Run integration tests:
```bash
cd runtime-core

# Test Docker integration (requires Docker daemon)
cargo test test_docker --features docker,multiprocess -- --nocapture

# Test specific scenarios
cargo test test_docker_multiprocess_e2e --features docker,multiprocess -- --nocapture

# Skip Docker tests
SKIP_DOCKER_TESTS=1 cargo test
```

---

## Quick Reference: Common Commands

```bash
# Check Docker status
docker info
docker version

# List containers
docker ps -a --filter "label=remotemedia.session_id"

# View logs
docker logs -f <container_id>
docker logs --tail 100 <container_id>

# Monitor resources
docker stats <container_id>

# Inspect container
docker inspect <container_id> | jq '.[0].State'

# Cleanup
docker stop <container_id>
docker rm -f <container_id>
docker system prune -f

# Execute commands
docker exec <container_id> python --version
docker exec -it <container_id> /bin/bash

# Check IPC files
docker exec <container_id> ls -la /tmp/iceoryx2/
docker exec <container_id> ls -la /dev/shm/
```

---

## Support

For issues not covered in this guide:

1. Check existing issues: https://github.com/your-repo/remotemedia-sdk/issues
2. Enable debug logging: `export RUST_LOG=debug`
3. Collect logs and system info
4. Create a detailed issue report with reproduction steps

---

**Last Updated:** 2025-01-13
**Version:** 1.0
**Maintainer:** RemoteMedia SDK Team
