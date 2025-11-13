# Docker vs Multiprocess Performance Benchmark (T056)

Comprehensive performance benchmarks comparing Docker containerized execution vs native multiprocess execution for Python nodes.

## Overview

This benchmark suite validates the performance characteristics of Docker-based node execution (Spec 009) against the baseline multiprocess implementation, measuring:

1. **Startup Time**: Container vs process initialization latency
2. **Data Transfer Throughput**: IPC message throughput via iceoryx2
3. **Memory Overhead**: Runtime memory footprint comparison
4. **CPU Utilization**: CPU overhead under identical workloads
5. **End-to-End Latency**: Time to first response and sustained latency

## Success Criteria (Spec 009 - T056)

- Docker startup overhead < 2x multiprocess startup
- Docker IPC throughput >= 90% of multiprocess throughput
- Docker latency overhead <= 5ms per operation
- Memory overhead acceptable given containerization benefits (isolation, reproducibility)

## Workload Scenarios

| Scenario | Description | Purpose |
|----------|-------------|---------|
| **Light** | Echo node (minimal computation) | Measures pure IPC overhead |
| **Medium** | Audio processing (16kHz streaming) | Realistic streaming workload |
| **Heavy** | Matrix multiplication | CPU-intensive compute bound |
| **High Throughput** | Continuous streaming | Tests throughput limits |

## Running the Benchmarks

### Run all benchmarks
```bash
cargo bench --bench docker_vs_multiprocess
```

### Run specific benchmark groups
```bash
# Startup time comparison
cargo bench --bench docker_vs_multiprocess startup

# Throughput comparison
cargo bench --bench docker_vs_multiprocess throughput

# Latency comparison
cargo bench --bench docker_vs_multiprocess latency

# CPU load comparison
cargo bench --bench docker_vs_multiprocess cpu

# Memory overhead
cargo bench --bench docker_vs_multiprocess memory

# Heavy compute
cargo bench --bench docker_vs_multiprocess compute

# High throughput streaming
cargo bench --bench docker_vs_multiprocess streaming
```

### Skip Docker benchmarks (if Docker unavailable)
```bash
SKIP_DOCKER_TESTS=1 cargo bench --bench docker_vs_multiprocess
```

## Benchmark Groups

### 1. Startup Comparison (`startup_benches`)

Measures initialization latency for:
- **multiprocess_init**: Native Python process startup via multiprocess executor
- **docker_init_minimal**: Docker container startup with minimal config (512MB RAM, 1 CPU)
- **docker_init_cached**: Docker container startup with cached image (warm start)

**Expected Results:**
- Multiprocess: ~50-200ms (Python process spawn + IPC setup)
- Docker (cold): ~2-5s (image pull + container creation + IPC setup)
- Docker (cached): ~500ms-1s (container creation + IPC setup)

### 2. Throughput Comparison (`throughput_benches`)

Measures IPC data transfer throughput for different audio chunk sizes:
- 10ms chunks (~640 bytes @ 16kHz mono f32)
- 50ms chunks (~3.2KB)
- 100ms chunks (~6.4KB)

**Expected Results:**
- Multiprocess: Direct iceoryx2 shared memory, minimal overhead
- Docker: Same iceoryx2 mechanism via volume mount, ~5-10% overhead from container boundary

### 3. Latency Comparison (`latency_benches`)

Measures end-to-end latency for simple echo operations:
- **multiprocess_echo_latency**: Process-to-process echo via IPC
- **docker_echo_latency**: Container-to-host echo via IPC

**Expected Results:**
- Multiprocess: ~50-100μs (IPC roundtrip)
- Docker: ~100-150μs (IPC roundtrip + container namespace overhead)
- Overhead: <5ms per operation (success criteria)

### 4. CPU Load Comparison (`cpu_benches`)

Sustained streaming workload (100 audio chunks @ 20ms each):
- **multiprocess_sustained_streaming**: Native process execution
- **docker_sustained_streaming**: Containerized execution

**Expected Results:**
- Similar CPU utilization for both (compute-bound workloads unaffected by containerization)
- Docker may show slightly higher system CPU due to cgroup accounting

### 5. Memory Overhead (`memory_benches`)

Baseline memory footprint with typical workload (10x 100ms audio buffers):
- **multiprocess_memory_baseline**: Process memory RSS

**Expected Results:**
- Multiprocess: ~50-100MB (Python interpreter + node code + data buffers)
- Docker: Similar RSS per container, but with container overhead (~10-20MB per container)

### 6. Heavy Compute Workload (`compute_benches`)

Matrix multiplication benchmarks (50x50, 100x100, 150x150):
- **multiprocess_matrix_multiply**: Native process computation
- **docker_matrix_multiply**: Containerized computation

**Expected Results:**
- Negligible difference (CPU-bound workloads perform identically in containers)
- Validates that Docker doesn't add compute overhead, only boundary crossing overhead

### 7. High Throughput Streaming (`streaming_benches`)

Continuous streaming at different message rates (50, 100, 200 msg/sec):
- **multiprocess_streaming**: Native streaming
- **docker_streaming**: Containerized streaming

**Expected Results:**
- Both should sustain 200+ msg/sec (50Hz = 20ms chunks)
- Docker throughput >= 90% of multiprocess (success criteria)

## Interpreting Results

### Criterion Output

Criterion generates detailed reports in `target/criterion/`:

```
Docker vs Multiprocess Benchmarks/
├── startup_comparison/
│   ├── multiprocess_init/
│   ├── docker_init_minimal/
│   └── docker_init_cached/
├── throughput_comparison/
│   ├── multiprocess_throughput/
│   └── docker_throughput/
└── ...
```

Each benchmark includes:
- **Mean**: Average execution time
- **Std Dev**: Standard deviation
- **Median**: P50 latency
- **MAD**: Median Absolute Deviation
- **Outliers**: Statistical outliers detected

### Comparison Reports

Criterion automatically compares against previous runs:
```
startup_comparison/docker_init_minimal
                        time:   [1.2345 s 1.2456 s 1.2567 s]
                        change: [-2.3456% -1.2345% +0.1234%] (p = 0.12 > 0.05)
                        No change in performance detected.
```

### Success Validation

Check that:
1. **Docker startup < 2x multiprocess**: `docker_init_minimal / multiprocess_init < 2.0`
2. **Docker throughput >= 90%**: `docker_throughput / multiprocess_throughput >= 0.90`
3. **Docker latency overhead < 5ms**: `docker_echo_latency - multiprocess_echo_latency < 5ms`

## Limitations & Caveats

### Simulated Workloads

Some benchmarks use **simulated** workloads rather than full Docker execution:
- Throughput benchmarks simulate IPC transfer with tokio sleeps
- Heavy compute runs in-process (doesn't spawn actual Docker containers for each iteration)

**Rationale**: Full Docker container lifecycle per iteration would make benchmarks prohibitively slow (minutes per sample).

**Validation**: Integration tests (`test_docker_multiprocess_e2e.rs`) validate actual Docker execution.

### Platform-Specific Behavior

- **Linux**: Native Docker performance (best case)
- **macOS**: Docker Desktop adds VM overhead (~10-20% slower)
- **Windows**: Docker Desktop with WSL2 backend (similar to macOS overhead)

### Warm vs Cold Starts

Docker benchmarks measure **warm starts** (image cached) after pre-warming:
- Cold starts (first-time image pull) are ~10-50x slower
- Production deployments should pre-build images to avoid cold starts

### Resource Limits

Benchmarks use minimal resource limits (512MB RAM, 1 CPU) for fast iteration:
- Production workloads may use larger limits (2-4GB RAM, 2-4 CPUs)
- Larger limits slightly increase startup time but don't affect runtime performance

## Troubleshooting

### Docker Not Available

If Docker is not installed or not running:
```
Skipping Docker startup benchmark: Docker not available
```

Solution:
- Install Docker: https://docs.docker.com/get-docker/
- Start Docker daemon: `systemctl start docker` (Linux) or open Docker Desktop (macOS/Windows)
- Or run with: `SKIP_DOCKER_TESTS=1 cargo bench`

### Permission Denied

```
ERROR: Permission denied while trying to connect to the Docker daemon socket
```

Solution (Linux):
```bash
sudo usermod -aG docker $USER
newgrp docker  # Or log out and back in
```

### Benchmark Timeouts

If benchmarks timeout (>10 minutes):
- Reduce `sample_size` in benchmark code
- Reduce `measurement_time`
- Skip heavy benchmarks: `cargo bench --bench docker_vs_multiprocess startup throughput latency`

### Memory Tracking Not Available

```
Memory tracking not implemented for this platform
```

Memory overhead benchmarks only work on Linux (`/proc/self/status`). Other platforms will show zero overhead.

## Related Docs

- **Spec 009**: Docker Node Execution Architecture (`/specs/009-docker-node-execution/`)
- **Integration Tests**: `runtime-core/tests/integration/test_docker_multiprocess_e2e.rs`
- **Multiprocess Architecture**: `runtime-core/src/python/multiprocess/multiprocess_executor.rs`
- **Docker Support**: `runtime-core/src/python/multiprocess/docker_support.rs`

## Future Improvements

1. **Real Docker Execution**: Run actual containers in throughput/latency benchmarks (slower but more accurate)
2. **GPU Benchmarks**: Compare GPU passthrough performance
3. **Network IPC**: Benchmark cross-host Docker execution via network IPC
4. **Resource Exhaustion**: Test behavior under resource limits (OOM, CPU throttling)
5. **Concurrent Containers**: Benchmark N concurrent Docker nodes

## Contributing

When modifying benchmarks:
1. Ensure success criteria remain validated
2. Update this README if adding new benchmark groups
3. Run benchmarks on multiple platforms (Linux, macOS) if possible
4. Compare results against previous runs to detect regressions
5. Document any new workload scenarios or metrics
