# Docker Node Configuration Examples Guide

This directory contains comprehensive example manifests demonstrating various Docker node configuration patterns for the RemoteMedia SDK.

## Overview

The RemoteMedia SDK supports running Python nodes in Docker containers for dependency isolation, resource control, and environment reproducibility. These examples showcase best practices for different use cases.

## Available Examples

### 1. GPU Accelerated Pipeline (`gpu_accelerated_pipeline.json`)

**Use Case**: High-performance ML inference with NVIDIA GPU acceleration

**Key Features**:
- NVIDIA GPU passthrough for ML inference
- Multiple GPU allocation strategies (specific GPU IDs, all GPUs)
- High memory allocation (6-16GB per node)
- CUDA-optimized base images (PyTorch, TensorFlow, custom CUDA)
- Parallel GPU execution across multiple nodes
- Model weight volume mounts

**Nodes**:
- `speech_recognition`: Whisper ASR on GPU 0 (8GB memory, 4 CPU cores)
- `nlp_processor`: BERT NLP on GPU 1 (16GB memory, 8 CPU cores)
- `video_inference`: YOLOv8 detection with all GPUs (6GB memory, 4 CPU cores)

**Resource Requirements**:
- 2+ NVIDIA GPUs with CUDA 11.7+ or 12.x
- NVIDIA Container Toolkit (nvidia-docker2)
- 32GB+ host RAM
- Pre-downloaded model weights

**Best For**:
- Real-time audio/video processing
- Large-scale ML inference
- Multi-model ensembles
- Production ML pipelines

---

### 2. Microservices Pipeline (`microservices_pipeline.json`)

**Use Case**: Service mesh with isolated microservices running different Python versions and conflicting dependencies

**Key Features**:
- Multiple Python versions (3.9, 3.10, 3.11) in same pipeline
- TensorFlow and PyTorch isolated in separate containers
- Legacy service (Python 3.9) coexisting with modern services (Python 3.11)
- Framework-specific dependency isolation
- Lightweight Alpine containers for simple services

**Nodes**:
- `legacy_service_py39`: Python 3.9 with old numpy/scikit-learn
- `modern_service_py311`: Python 3.11 with Polars and modern packages
- `tensorflow_service`: TensorFlow 2.x isolated from PyTorch
- `pytorch_service`: PyTorch 2.x isolated from TensorFlow
- `data_aggregator`: Minimal Alpine container for aggregation

**Best For**:
- Migrating from legacy to modern code
- Running conflicting ML frameworks together
- Polyglot data processing pipelines
- Gradual system modernization

---

### 3. Resource Constrained Pipeline (`resource_constrained_pipeline.json`)

**Use Case**: Strict resource limits for multi-tenant environments and edge devices

**Key Features**:
- Ultra-low resource allocations (128MB, 0.25 CPU cores)
- CPU throttling demonstrations
- Memory-capped streaming algorithms
- Resource isolation patterns
- Burst handling with backpressure

**Nodes**:
- `minimal_preprocessor`: 128MB, 0.25 cores - minimal footprint
- `cpu_throttled_processor`: 384MB, 0.5 cores - fractional CPU
- `memory_capped_analyzer`: 768MB, 1.0 cores - streaming algorithms
- `burst_limited_service`: 192MB, 0.3 cores - lowest viable allocation
- `isolated_heavy_task`: 2048MB, 2.0 cores - isolated batch processing

**Resource Enforcement**:
- Linux cgroups for memory limits (OOM killer on exceed)
- CFS scheduler for CPU quotas (throttling on exceed)
- Shared memory (/dev/shm) limits for IPC

**Best For**:
- Multi-tenant SaaS platforms
- Edge devices with limited resources
- Cost optimization by right-sizing
- Preventing noisy neighbor problems
- Testing under resource pressure

---

### 4. Development Pipeline (`development_pipeline.json`)

**Use Case**: Developer-friendly setup with live code reload, debugging, and profiling

**Key Features**:
- Live code reload via volume mounts
- Interactive debugging with ipdb
- Comprehensive logging (structlog, colorlog)
- Performance profiling (py-spy, memory-profiler)
- Test fixtures and mocking
- Extended timeouts and no cleanup on error

**Nodes**:
- `dev_node_hot_reload`: Live code editing with watchdog
- `verbose_logging_node`: Structured JSON logging with OpenTelemetry
- `profiling_node`: CPU/memory profiling and flame graphs
- `test_fixtures_node`: pytest integration for in-container testing

**Development Workflow**:
1. Edit code in your IDE on host
2. Changes appear in container instantly
3. Add `breakpoint()` for interactive debugging
4. Profile performance with py-spy
5. View logs in real-time or analyze JSON logs

**Best For**:
- Active development and iteration
- Debugging complex issues
- Performance optimization
- Testing in isolated environments

---

### 5. Production Pipeline (`production_pipeline.json`)

**Use Case**: Production-ready configuration with security, monitoring, and resilience

**Key Features**:
- Security hardening (non-root user, read-only filesystem)
- Comprehensive monitoring (Prometheus metrics, OpenTelemetry)
- Health checks and auto-restart
- Circuit breakers and retry logic
- Rate limiting and input validation
- At-least-once delivery guarantees

**Nodes**:
- `security_hardened_gateway`: Input validation, rate limiting, minimal attack surface
- `monitored_processor`: Prometheus metrics, health checks, process monitoring
- `resilient_ml_service`: Circuit breaker, retries, fallback strategies
- `production_data_sink`: Batching, compression, durable storage

**Production Checklist**:
- ✓ Security: Non-root, no privileged mode, input validation
- ✓ Monitoring: Metrics, health checks, distributed tracing
- ✓ Resilience: Circuit breakers, retries, graceful degradation
- ✓ Reliability: Resource limits, auto-restart, disaster recovery

**Best For**:
- Production deployments
- Mission-critical pipelines
- SLA/SLO commitments
- Regulated environments

---

## Common Configuration Patterns

### Docker Config Structure

All Docker nodes follow this configuration pattern:

```json
{
  "id": "node_id",
  "node_type": "module.NodeClass",
  "executor": "multiprocess",
  "metadata": {
    "use_docker": true,
    "docker_config": {
      "python_version": "3.11",
      "base_image": "python:3.11-slim",
      "python_packages": ["package1", "package2"],
      "system_packages": ["apt-package1"],
      "memory_mb": 1024,
      "cpu_cores": 1.0,
      "shm_size_mb": 1024,
      "env_vars": {"VAR": "value"},
      "gpu_devices": ["0"],
      "volumes": [{"host_path": "/host", "container_path": "/container"}]
    }
  }
}
```

### Resource Allocation Guidelines

| Use Case | Memory | CPU Cores | Shared Memory |
|----------|---------|-----------|---------------|
| Minimal routing | 128-256MB | 0.25-0.5 | 256-512MB |
| Light processing | 512MB | 0.5-1.0 | 512-1024MB |
| Moderate computation | 1-2GB | 1.0-2.0 | 1-2GB |
| ML inference (CPU) | 2-4GB | 2.0-4.0 | 2-4GB |
| ML inference (GPU) | 4-16GB | 2.0-8.0 | 4-8GB |
| Batch processing | 8-32GB | 4.0-16.0 | 4-16GB |

### Base Image Selection

| Image Type | Size | Use Case |
|------------|------|----------|
| `python:3.11-alpine` | ~50MB | Minimal services, routing |
| `python:3.11-slim` | ~120MB | General Python workloads |
| `python:3.11` | ~900MB | Complex system dependencies |
| `pytorch/pytorch:*` | ~5GB | PyTorch ML workloads |
| `tensorflow/tensorflow:*` | ~4GB | TensorFlow ML workloads |
| `nvidia/cuda:*` | ~2-8GB | Custom CUDA applications |

### Volume Mount Patterns

```json
{
  "volumes": [
    {
      "host_path": "/models",
      "container_path": "/models",
      "read_only": true,
      "mount_type": "bind"
    }
  ]
}
```

**Common Mounts**:
- Model weights: Read-only, persistent storage
- Source code: Read-write for development, read-only for production
- Logs/profiles: Read-write for output collection
- Credentials: Read-only, mounted from secrets manager
- Cache: Read-write for performance optimization

### Environment Variables

**Development**:
- `PYTHONUNBUFFERED=1`: Real-time log output
- `PYTHONDONTWRITEBYTECODE=1`: No .pyc files
- `PYTHONDEVMODE=1`: Additional runtime checks
- `LOG_LEVEL=DEBUG`: Verbose logging

**Production**:
- `PYTHONUNBUFFERED=1`: Real-time log output
- `PRODUCTION=true`: Application knows environment
- `LOG_LEVEL=INFO`: Moderate logging
- `METRICS_ENABLED=true`: Enable monitoring

**GPU Workloads**:
- `CUDA_VISIBLE_DEVICES=0`: Specific GPU
- `CUDA_VISIBLE_DEVICES=all`: All GPUs
- `TORCH_CUDA_ARCH_LIST=7.0;8.0;9.0`: Supported architectures
- `CUDNN_BENCHMARK=1`: Enable cuDNN auto-tuning

## Running the Examples

### Prerequisites

1. **Docker daemon** running:
   ```bash
   docker --version  # Should be 20.10.0+
   ```

2. **iceoryx2 setup**:
   ```bash
   sudo mkdir -p /tmp/iceoryx2
   sudo chmod 777 /tmp/iceoryx2
   ```

3. **For GPU examples**: NVIDIA Container Toolkit
   ```bash
   docker run --gpus all nvidia/cuda:12.1.0-base-ubuntu22.04 nvidia-smi
   ```

### Quick Start

```bash
# 1. Navigate to examples directory
cd examples/docker-node

# 2. Choose an example manifest
export MANIFEST=gpu_accelerated_pipeline.json

# 3. Run via gRPC transport (recommended)
cd ../../transports/remotemedia-grpc
cargo run --bin grpc_server --release -- --manifest ../../examples/docker-node/$MANIFEST
```

### Monitoring

```bash
# List running containers
docker ps --filter "name=remotemedia_"

# View container logs
docker logs remotemedia_<session_id>_<node_id> -f

# Monitor resource usage
docker stats --filter "name=remotemedia_"

# Check GPU utilization
nvidia-smi -l 1
```

### Debugging

```bash
# Inspect container
docker inspect remotemedia_<session_id>_<node_id>

# Execute command in container
docker exec -it remotemedia_<session_id>_<node_id> bash

# Check IPC channels
ls -la /tmp/iceoryx2/services/

# View health status
docker exec remotemedia_<session_id>_<node_id> curl http://localhost:8080/health
```

## Troubleshooting

### Container Exits Immediately

**Check exit code**:
```bash
docker ps -a | grep remotemedia_
```

**Common exit codes**:
- `137`: OOMKilled (memory limit exceeded)
- `1`: Python error (check logs)
- `139`: Segmentation fault

### GPU Not Available

**Verify GPU support**:
```bash
docker run --gpus all nvidia/cuda:12.1.0-base-ubuntu22.04 nvidia-smi
```

**If fails**: Install NVIDIA Container Toolkit

### IPC Channel Not Found

**Check iceoryx2 setup**:
```bash
ls -ld /tmp/iceoryx2
# Should be drwxrwxrwx

# Check volume mount
docker inspect <container_id> | grep iceoryx
```

### High Memory Usage

**Solutions**:
1. Increase `memory_mb` in docker_config
2. Implement streaming/chunked processing
3. Use memory-mapped I/O for large datasets
4. Check for memory leaks

### CPU Throttling

**Solutions**:
1. Increase `cpu_cores` in docker_config
2. Optimize hot paths in code
3. Set thread pool limits (OMP_NUM_THREADS)
4. Consider native Rust nodes for performance-critical code

## Best Practices

### Security

1. **Never run as root**: Use non-root user in Dockerfile
2. **Read-only filesystem**: Where possible, make root filesystem read-only
3. **Secrets management**: Mount credentials from external secrets manager
4. **Input validation**: Validate all inputs at gateway nodes
5. **Network policies**: Restrict container-to-container communication
6. **Image scanning**: Scan for vulnerabilities before deployment

### Performance

1. **Right-size resources**: Start small, scale up based on monitoring
2. **Cache images**: Use image caching to reduce startup time
3. **Zero-copy IPC**: Leverage iceoryx2 for high-throughput data transfer
4. **Batch processing**: Amortize overhead across multiple requests
5. **GPU utilization**: Monitor and optimize GPU usage
6. **Connection pooling**: Reuse connections to external services

### Reliability

1. **Health checks**: Configure health check endpoints
2. **Auto-restart**: Enable automatic restart on failure
3. **Circuit breakers**: Prevent cascade failures
4. **Graceful degradation**: Fallback strategies for failures
5. **Resource limits**: Prevent resource exhaustion
6. **Monitoring**: Comprehensive metrics and alerting

### Development

1. **Volume mounts**: Mount source code for live editing
2. **Extended timeouts**: Allow time for debugging
3. **Verbose logging**: Enable DEBUG level logging
4. **Keep containers**: Set `docker_cleanup_on_exit: false`
5. **Interactive debugging**: Use ipdb with `breakpoint()`
6. **Profiling**: Enable profiling tools for optimization

## Example Selection Guide

Choose the right example for your use case:

| Your Need | Recommended Example |
|-----------|---------------------|
| ML inference with GPU | `gpu_accelerated_pipeline.json` |
| Different Python versions | `microservices_pipeline.json` |
| Conflicting dependencies | `microservices_pipeline.json` |
| Limited resources | `resource_constrained_pipeline.json` |
| Active development | `development_pipeline.json` |
| Production deployment | `production_pipeline.json` |
| Learning Docker basics | `simple_docker_test.json` |
| Mixed native/Docker | `mixed_execution_pipeline.json` |

## Further Reading

- **Specification**: [../../specs/010-docker-multiprocess-integration/spec.md](../../specs/010-docker-multiprocess-integration/spec.md)
- **Quickstart**: [../../specs/010-docker-multiprocess-integration/quickstart.md](../../specs/010-docker-multiprocess-integration/quickstart.md)
- **Architecture**: [../../specs/010-docker-multiprocess-integration/plan.md](../../specs/010-docker-multiprocess-integration/plan.md)
- **README**: [README.md](README.md)

---

**Contributing**: When adding new examples, ensure they:
1. Include comprehensive inline documentation
2. Demonstrate specific features or patterns
3. Follow the established JSON structure
4. Include performance and security notes
5. Are validated and tested

**Questions?** Open an issue or discussion in the RemoteMedia SDK repository.
