# Docker Node Examples - Quick Reference Card

## Example Comparison Matrix

| Example | Primary Use Case | Key Features | Resource Profile | Complexity |
|---------|-----------------|--------------|------------------|------------|
| **gpu_accelerated** | ML inference with GPUs | GPU passthrough, high memory, parallel GPUs | 6-16GB RAM, 2-8 CPUs, GPUs | Advanced |
| **microservices** | Mixed Python versions | TF+PyTorch, Py3.9+3.11, service mesh | 256MB-4GB RAM, 0.5-3 CPUs | Advanced |
| **resource_constrained** | Edge devices, multi-tenant | Strict limits, throttling, streaming | 128MB-2GB RAM, 0.25-2 CPUs | Intermediate |
| **development** | Active development | Live reload, debugging, profiling | 512MB-1GB RAM, 1-2 CPUs | Beginner |
| **production** | Production deployment | Security, monitoring, resilience | 512MB-8GB RAM, 1-4 CPUs | Advanced |

## Use Case Decision Tree

```
Do you need GPU acceleration?
├─ Yes → gpu_accelerated_pipeline.json
└─ No
   ├─ Do you have conflicting dependencies?
   │  ├─ Yes → microservices_pipeline.json
   │  └─ No
   │     ├─ Are resources very limited?
   │     │  ├─ Yes → resource_constrained_pipeline.json
   │     │  └─ No
   │     │     ├─ Is this for development?
   │     │     │  ├─ Yes → development_pipeline.json
   │     │     │  └─ No → production_pipeline.json
```

## Configuration Quick Lookup

### GPU Devices
```json
"gpu_devices": ["0"]        // Single GPU (ID 0)
"gpu_devices": ["0", "1"]   // Multiple specific GPUs
"gpu_devices": ["all"]      // All available GPUs
"gpu_devices": []           // No GPU (CPU only)
```

### Memory Sizing
```json
"memory_mb": 128     // Minimal (routing, passthrough)
"memory_mb": 512     // Light processing
"memory_mb": 1024    // Standard workload
"memory_mb": 4096    // ML inference (CPU)
"memory_mb": 8192    // Large ML models (GPU)
"memory_mb": 16384   // Very large models
```

### CPU Allocation
```json
"cpu_cores": 0.25    // Ultra-minimal (25% of one core)
"cpu_cores": 0.5     // Light processing
"cpu_cores": 1.0     // Standard workload
"cpu_cores": 2.0     // Moderate parallel work
"cpu_cores": 4.0     // Heavy computation
"cpu_cores": 8.0     // Very heavy parallel work
```

### Shared Memory
```json
"shm_size_mb": 256    // Minimal IPC
"shm_size_mb": 512    // Light IPC
"shm_size_mb": 1024   // Standard (default)
"shm_size_mb": 2048   // Heavy IPC, small batches
"shm_size_mb": 4096   // Large batches, PyTorch DataLoader
"shm_size_mb": 8192   // Very large batches
```

## Base Image Selection

| Image | Size | Boot Time | Use For |
|-------|------|-----------|---------|
| `python:3.11-alpine` | ~50MB | ~2s | Minimal services |
| `python:3.11-slim` | ~120MB | ~3s | General workloads |
| `pytorch/pytorch:2.1.0-cuda12.1` | ~5GB | ~10s | PyTorch + GPU |
| `tensorflow/tensorflow:2.14.0-gpu` | ~4GB | ~10s | TensorFlow + GPU |
| `nvidia/cuda:12.2.0-runtime` | ~2GB | ~5s | Custom CUDA |

## Common Environment Variables

### Development
```json
"PYTHONUNBUFFERED": "1"          // Real-time logs
"PYTHONDONTWRITEBYTECODE": "1"   // No .pyc files
"PYTHONDEVMODE": "1"             // Extra checks
"LOG_LEVEL": "DEBUG"             // Verbose logging
```

### Production
```json
"PYTHONUNBUFFERED": "1"       // Real-time logs
"PRODUCTION": "true"          // Production mode
"LOG_LEVEL": "INFO"           // Moderate logging
"METRICS_ENABLED": "true"     // Enable monitoring
```

### GPU/CUDA
```json
"CUDA_VISIBLE_DEVICES": "0"             // Specific GPU
"TORCH_CUDA_ARCH_LIST": "7.0;8.0;9.0"  // CUDA architectures
"CUDNN_BENCHMARK": "1"                  // Auto-tuning
```

## Volume Mount Examples

### Model Weights (Read-Only)
```json
{
  "host_path": "/data/models/my_model",
  "container_path": "/models",
  "read_only": true,
  "mount_type": "bind"
}
```

### Source Code (Development)
```json
{
  "host_path": "/home/user/project/src",
  "container_path": "/workspace/src",
  "read_only": false,
  "mount_type": "bind"
}
```

### Logs/Output
```json
{
  "host_path": "/tmp/logs",
  "container_path": "/logs",
  "read_only": false,
  "mount_type": "bind"
}
```

## Monitoring Commands

```bash
# List running containers
docker ps --filter "name=remotemedia_"

# Live resource monitoring
docker stats --filter "name=remotemedia_"

# Container logs
docker logs remotemedia_<session>_<node> -f

# GPU utilization
nvidia-smi -l 1

# Execute command in container
docker exec -it remotemedia_<session>_<node> bash

# Inspect configuration
docker inspect remotemedia_<session>_<node>

# Health check
docker exec remotemedia_<session>_<node> curl localhost:8080/health
```

## Troubleshooting Quick Reference

| Symptom | Likely Cause | Quick Fix |
|---------|--------------|-----------|
| Exit code 137 | OOMKilled | Increase `memory_mb` |
| Exit code 1 | Python error | Check logs: `docker logs` |
| Container not starting | Config error | `docker inspect` + check logs |
| GPU not available | nvidia-docker | Install Container Toolkit |
| IPC channel not found | iceoryx2 mount | Check `/tmp/iceoryx2` permissions |
| High CPU % (constant at limit) | Throttling | Increase `cpu_cores` |
| Slow startup | Large image | Use cached images, smaller base |

## Performance Characteristics

| Configuration | Cold Start | Warm Start | Request Latency |
|---------------|------------|------------|-----------------|
| Alpine minimal | 2-3s | 1-2s | +5ms |
| Slim standard | 3-5s | 2-3s | +10ms |
| GPU workload | 10-15s | 5-8s | +20-50ms |
| Development | 5-10s | 3-5s | +20-50ms |

## Example File Sizes

| File | Size | Nodes | Lines |
|------|------|-------|-------|
| gpu_accelerated_pipeline.json | 13KB | 6 | ~280 |
| microservices_pipeline.json | 15KB | 7 | ~300 |
| resource_constrained_pipeline.json | 18KB | 7 | ~350 |
| development_pipeline.json | 20KB | 6 | ~400 |
| production_pipeline.json | 24KB | 6 | ~500 |

## Next Steps

1. **Choose example**: Use decision tree above
2. **Customize**: Edit for your specific needs
3. **Test locally**: Run with gRPC server
4. **Monitor**: Check docker stats, logs
5. **Iterate**: Adjust resources based on monitoring
6. **Deploy**: Use production example as template

## Additional Resources

- Full guide: [EXAMPLES_GUIDE.md](EXAMPLES_GUIDE.md)
- Main README: [README.md](README.md)
- Specification: [../../specs/010-docker-multiprocess-integration/](../../specs/010-docker-multiprocess-integration/)
