# Advanced Examples

Production-ready patterns and advanced features for experienced RemoteMedia SDK users.

## Prerequisites

Before diving into advanced examples, you should have:
- ✅ Completed [Getting Started examples](../00-getting-started/)
- ✅ Understanding of async Python or Rust
- ✅ Familiarity with pipeline manifests and node types
- ✅ Basic systems programming knowledge helpful

---

## Examples Overview

| Example | Complexity | Time | Key Concepts |
|---------|------------|------|--------------|
| [multiprocess-nodes](multiprocess-nodes/) | ⭐⭐⭐ | 30 min | Process isolation, iceoryx2 IPC, fault tolerance |
| [streaming-pipelines](streaming-pipelines/) | ⭐⭐⭐ | 25 min | Real-time processing, VAD, chunked execution |
| [custom-transports](custom-transports/) | ⭐⭐⭐⭐ | 45 min | Custom transport layers, plugin architecture |
| [grpc-remote-execution](grpc-remote-execution/) | ⭐⭐ | 20 min | Remote pipelines, gRPC streaming, distributed systems |

**Complexity Scale**:
- ⭐⭐ = Intermediate - Requires understanding of SDK basics
- ⭐⭐⭐ = Advanced - Complex concepts, performance tuning
- ⭐⭐⭐⭐ = Expert - Deep SDK knowledge, custom implementations

---

## Multiprocess Nodes

**Path**: [multiprocess-nodes/](multiprocess-nodes/)

Run Python nodes in separate processes with zero-copy IPC using iceoryx2.

### What You'll Learn
- Process isolation for fault tolerance
- iceoryx2 shared memory IPC
- Health monitoring and process recovery
- Performance vs reliability trade-offs

### Use Cases
- Long-running services requiring fault isolation
- CPU-intensive Python processing
- Isolation of third-party libraries
- Scalable multiprocess architectures

### Performance
- **Throughput**: 10-100x higher than multiprocessing.Queue
- **Latency**: <1ms IPC overhead with zero-copy
- **Memory**: Shared memory eliminates serialization

**Prerequisites**: Understanding of multiprocessing concepts

---

## Streaming Pipelines

**Path**: [streaming-pipelines/](streaming-pipelines/)

Real-time audio streaming with Voice Activity Detection and chunked processing.

### What You'll Learn
- Streaming vs batch processing
- Chunked audio handling
- Real-time VAD with Silero
- Session management
- Backpressure handling

### Use Cases
- Live audio transcription
- Real-time audio effects
- Voice activity detection
- WebRTC integration

### Performance
- **Latency**: <20ms end-to-end (configurable chunk size)
- **Throughput**: Up to 100 concurrent streams (gRPC)
- **Accuracy**: 95%+ VAD accuracy with Silero

**Prerequisites**: Understanding of async programming

---

## Custom Transports

**Path**: [custom-transports/](custom-transports/)

Build custom transport layers for RemoteMedia pipelines.

### What You'll Learn
- Transport plugin architecture
- Implementing `PipelineTransport` trait
- Session lifecycle management
- Custom serialization formats
- Integration with existing systems

### Use Cases
- HTTP/WebSocket transports
- Message queue integration (Kafka, RabbitMQ)
- Custom RPC protocols
- Proprietary communication systems

### Architecture
```
┌─────────────────────────────────────┐
│  Your Custom Transport              │
│  (implements PipelineTransport)     │
├─────────────────────────────────────┤
│  runtime-core (transport-agnostic)  │
│  • Executor                         │
│  • Node Registry                    │
│  • Session Router                   │
└─────────────────────────────────────┘
```

**Prerequisites**: Rust knowledge, understanding of async traits

---

## gRPC Remote Execution

**Path**: [grpc-remote-execution/](grpc-remote-execution/)

Execute pipelines on remote gRPC servers for distributed processing.

### What You'll Learn
- gRPC bidirectional streaming
- Remote pipeline execution
- Load balancing strategies
- Authentication and authorization
- Error handling across network

### Use Cases
- Microservices architectures
- GPU-accelerated remote processing
- Scalable cloud deployments
- Multi-region processing

### Architecture
```
┌──────────┐  gRPC   ┌──────────────┐
│  Client  ├────────→│ gRPC Server  │
│          │←────────│ (remotemedia)│
└──────────┘ stream  └──────────────┘
```

**Prerequisites**: gRPC basics, protobuf understanding

---

## Performance Optimization Guide

### General Principles

1. **Choose the Right Runtime**:
   - Native Rust: Best performance (2-16x faster for audio)
   - Multiprocess Python: Fault isolation, parallel processing
   - Single-process Python: Simplest, good for development

2. **Minimize Data Copies**:
   - Use zero-copy numpy arrays with FFI
   - Enable iceoryx2 for multiprocess (shared memory)
   - Avoid unnecessary serialization

3. **Tune for Latency vs Throughput**:
   - Small chunks = Low latency, higher overhead
   - Large chunks = High throughput, more latency
   - Find balance for your use case

### Benchmarking

Each advanced example includes benchmarking code:
```bash
cd multiprocess-nodes/
python benchmark.py --runs=100
```

Expected results in README.md for comparison.

---

## Common Patterns

### Error Handling
```python
try:
    result = await pipeline.run(data)
except remotemedia.ExecutionError as e:
    # Handle node failures
    logger.error(f"Node {e.node_id} failed: {e}")
except remotemedia.TransportError as e:
    # Handle network/IPC errors
    logger.error(f"Transport error: {e}")
```

### Resource Cleanup
```python
async with pipeline.create_session() as session:
    # Session auto-cleanup on exit
    result = await session.run(data)
```

### Metrics Collection
```python
pipeline = Pipeline.from_yaml(
    "pipeline.yaml",
    enable_metrics=True
)
metrics = pipeline.get_metrics()
print(f"Duration: {metrics['duration_ms']}ms")
```

---

## Troubleshooting

### Performance Issues

**Problem**: Pipeline slower than expected

**Solutions**:
1. Check runtime selection: `is_rust_runtime_available()`
2. Enable metrics to identify bottleneck nodes
3. Consider multiprocess for CPU-bound Python nodes
4. Profile with `py-spy` or `flamegraph`

### IPC Errors (Multiprocess)

**Problem**: `iceoryx2 channel not found`

**Solutions**:
1. Ensure services started in correct order
2. Check session_id matches across processes
3. Verify iceoryx2 cleanup: `rm /dev/shm/iox2_*`
4. Check process health monitoring logs

### gRPC Connection Issues

**Problem**: `Connection refused` or timeouts

**Solutions**:
1. Verify server is running: `grpc_health_v1`
2. Check network/firewall configuration
3. Enable gRPC logging: `GRPC_VERBOSITY=debug`
4. Verify authentication credentials

---

## Next Steps

**After completing advanced examples**:

1. **Build an Application** → [../02-applications/](../02-applications/)
2. **Contribute Custom Nodes** → See [CONTRIBUTING.md](../../CONTRIBUTING.md)
3. **Production Deployment** → Check [docs/deployment/](../../docs/deployment/)

---

## Getting Help

**Documentation**: [docs.remotemedia.dev/advanced](https://docs.remotemedia.dev/advanced)
**Architecture Deep Dive**: [CLAUDE.md](../../CLAUDE.md)
**Issues**: [GitHub Issues](https://github.com/org/remotemedia-sdk/issues)

---

**Ready for advanced concepts?** Pick an example above and dive in!

**Last Updated**: 2025-11-07
**SDK Version**: v0.4.0+
