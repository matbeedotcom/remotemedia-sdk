# gRPC Multiprocess Integration - Quick Start Guide

## Overview

This guide shows how to execute pipelines containing Python nodes using the gRPC service with multiprocess execution enabled. Existing clients work without code changes - simply update manifests to configure multiprocess behavior.

## Prerequisites

- gRPC service running with `multiprocess` feature enabled
- Python 3.11+ installed and accessible
- Python nodes registered in `remotemedia.core.multiprocess`

## Basic Usage

### 1. Start gRPC Service

```bash
# Start service with multiprocess support
cargo run --bin grpc_server --features multiprocess,grpc-transport

# Service starts on default port 50051
```

### 2. Create Pipeline Manifest

**Without multiprocess configuration** (uses defaults):

```json
{
  "version": "v1",
  "metadata": {
    "name": "speech-to-speech-pipeline"
  },
  "nodes": [
    {
      "id": "input_chunker",
      "node_type": "AudioChunkerNode",
      "params": {
        "chunk_size": 1024
      }
    },
    {
      "id": "whisper_asr",
      "node_type": "WhisperNode",
      "params": {
        "model": "base",
        "language": "en"
      }
    },
    {
      "id": "lfm2_s2s",
      "node_type": "LFM2AudioNode",
      "params": {
        "model": "large"
      }
    },
    {
      "id": "vibe_tts",
      "node_type": "VibeVoiceNode",
      "params": {
        "voice": "sarah"
      }
    }
  ],
  "connections": [
    { "from": "input_chunker", "to": "whisper_asr" },
    { "from": "whisper_asr", "to": "lfm2_s2s" },
    { "from": "lfm2_s2s", "to": "vibe_tts" }
  ]
}
```

**With multiprocess configuration** (per-pipeline settings):

```json
{
  "version": "v1",
  "metadata": {
    "name": "speech-to-speech-pipeline",
    "multiprocess": {
      "max_processes_per_session": 10,
      "channel_capacity": 100,
      "init_timeout_secs": 30,
      "python_executable": "python",
      "enable_backpressure": true
    }
  },
  "nodes": [...],
  "connections": [...]
}
```

### 3. Execute Pipeline via gRPC

**Using grpcurl** (command line):

```bash
# Save manifest to file
cat > manifest.json << 'EOF'
{
  "version": "v1",
  "metadata": {"name": "test-pipeline"},
  "nodes": [...],
  "connections": [...]
}
EOF

# Execute pipeline
grpcurl -plaintext \
  -d @manifest.json \
  localhost:50051 \
  remotemedia.v1.PipelineExecutionService/ExecutePipeline
```

**Using Node.js client** (existing code works!):

```javascript
const grpc = require('@grpc/grpc-js');
const protoLoader = require('@grpc/proto-loader');

// Load proto
const packageDefinition = protoLoader.loadSync('remotemedia.proto');
const proto = grpc.loadPackageDefinition(packageDefinition).remotemedia.v1;

// Create client
const client = new proto.PipelineExecutionService(
  'localhost:50051',
  grpc.credentials.createInsecure()
);

// Load manifest from file
const manifest = require('./manifest.json');

// Execute pipeline (no code changes needed!)
client.executePipeline({ manifest }, (err, response) => {
  if (err) {
    console.error('Error:', err);
    return;
  }
  console.log('Pipeline executed:', response);
});
```

**Using Python client**:

```python
import grpc
from remotemedia_v1_pb2 import ExecuteRequest, PipelineManifest
from remotemedia_v1_pb2_grpc import PipelineExecutionServiceStub
import json

# Load manifest
with open('manifest.json') as f:
    manifest_data = json.load(f)

# Convert to protobuf
manifest = PipelineManifest(
    version=manifest_data['version'],
    metadata={'name': manifest_data['metadata']['name']},
    nodes=[...],  # Convert nodes
    connections=[...]  # Convert connections
)

# Create channel
channel = grpc.insecure_channel('localhost:50051')
client = PipelineExecutionServiceStub(channel)

# Execute pipeline
request = ExecuteRequest(manifest=manifest)
response = client.ExecutePipeline(request)

print(f"Execution status: {response.status}")
```

---

## Monitoring & Debugging

### Check Process Status

**View active Python processes**:

```bash
# Linux
ps aux | grep python | grep remotemedia

# Windows
tasklist | findstr python
```

**Expected output** (3 Python nodes in example):
```
python  12345  user  whisper_asr  --session vad_s2s_123...
python  12346  user  lfm2_s2s     --session vad_s2s_123...
python  12347  user  vibe_tts     --session vad_s2s_123...
```

### Monitor Shared Memory Usage

**Linux** (iceoryx2):
```bash
# Check iceoryx2 shared memory segments
ipcs -m | grep iceoryx2

# Expected: Multiple segments for each IPC channel
```

**Windows** (iceoryx2):
```powershell
# Check named shared memory objects
Get-WmiObject -Class Win32_MappedLogicalDisk
```

### View gRPC Service Logs

```bash
# Run with debug logging
RUST_LOG=debug cargo run --bin grpc_server --features multiprocess,grpc-transport

# Look for multiprocess-specific logs:
# [INFO] Executor registry initialized: 3 Python nodes registered
# [INFO] Session abc123: Spawning process for node whisper_asr
# [DEBUG] Data bridge created: Native → Multiprocess (input_chunker → whisper_asr)
# [INFO] Session abc123: All nodes initialized
```

---

## Configuration

### Service-Wide Defaults (runtime.toml)

```toml
# runtime.toml (service root directory)
[multiprocess]
max_processes_per_session = 10
channel_capacity = 100
init_timeout_secs = 30
cleanup_grace_period_secs = 5
enable_backpressure = true
shm_segment_size = 268435456  # 256MB

[multiprocess.python]
executable = "python"
venv_path = ".venv"
extra_paths = ["./nodes"]
```

### Per-Pipeline Overrides (manifest metadata)

```json
{
  "version": "v1",
  "metadata": {
    "name": "high-throughput-pipeline",
    "multiprocess": {
      "max_processes_per_session": 20,
      "channel_capacity": 500,
      "init_timeout_secs": 60
    }
  },
  "nodes": [...],
  "connections": [...]
}
```

**Priority**: Manifest > runtime.toml > hardcoded defaults

---

## Performance Tuning

### Optimize for Latency

```json
{
  "metadata": {
    "multiprocess": {
      "channel_capacity": 50,      // Smaller buffers, lower latency
      "enable_backpressure": true  // Prevent buffer bloat
    }
  }
}
```

### Optimize for Throughput

```json
{
  "metadata": {
    "multiprocess": {
      "channel_capacity": 1000,    // Larger buffers
      "enable_backpressure": false // Don't block on full buffers
    }
  }
}
```

### Resource-Constrained Environments

```json
{
  "metadata": {
    "multiprocess": {
      "max_processes_per_session": 3,  // Limit concurrent processes
      "init_timeout_secs": 60          // Allow slower initialization
    }
  }
}
```

---

## Troubleshooting

### Issue: "Manifest validation error: Unknown node type"

**Cause**: Node type not registered in executor registry

**Solution**:
1. Check node is registered in Python SDK:
   ```python
   from remotemedia.core.multiprocessing import register_node

   @register_node("MyCustomNode")
   class MyCustomNode(MultiprocessNode):
       ...
   ```

2. Verify node module is imported before pipeline execution

### Issue: "Process spawn timeout"

**Cause**: Python process failed to start within init_timeout

**Solution**:
1. Increase timeout in manifest:
   ```json
   {"multiprocess": {"init_timeout_secs": 60}}
   ```

2. Check Python executable is accessible:
   ```bash
   which python
   python --version
   ```

3. Verify Python dependencies installed:
   ```bash
   pip list | grep remotemedia-sdk
   ```

### Issue: "Channel full, backpressure active"

**Cause**: Consumer (downstream node) slower than producer

**Solution**:
1. Increase channel capacity:
   ```json
   {"multiprocess": {"channel_capacity": 500}}
   ```

2. Optimize slow node (profile Python code)

3. Add intermediate buffering node

### Issue: "Data conversion latency >10ms"

**Cause**: Large payloads crossing executor boundaries

**Solution**:
1. Keep data in same executor type (don't mix Native ↔ Multiprocess unnecessarily)

2. Use native nodes for preprocessing (chunking, resampling)

3. Batch operations to reduce boundary crossings

### Issue: "Orphaned Python processes after service shutdown"

**Cause**: Cleanup failure or service crash

**Solution**:
1. Check cleanup logs:
   ```
   [INFO] Session abc123: Terminating 3 processes
   [DEBUG] Process 12345 (whisper_asr) terminated
   ```

2. Manual cleanup (Linux):
   ```bash
   pkill -f "python.*remotemedia"
   ```

3. Manual cleanup (Windows):
   ```powershell
   Get-Process python | Where-Object {$_.CommandLine -like "*remotemedia*"} | Stop-Process
   ```

---

## Best Practices

### 1. Node Placement

**Do**: Keep Python nodes together to maximize shared memory benefits
```json
{
  "nodes": [
    {"id": "native_preprocess", "node_type": "AudioChunkerNode"},
    {"id": "python_asr", "node_type": "WhisperNode"},
    {"id": "python_s2s", "node_type": "LFM2Node"},
    {"id": "python_tts", "node_type": "VibeVoiceNode"},
    {"id": "native_output", "node_type": "AudioOutputNode"}
  ]
}
```

**Don't**: Alternate between Native and Multiprocess unnecessarily (adds conversion overhead)

### 2. Resource Limits

**Do**: Set realistic process limits based on available memory
```json
{"multiprocess": {"max_processes_per_session": 5}}
```

**Don't**: Request unlimited processes (`null`) in production

### 3. Configuration Management

**Do**: Use runtime.toml for service-wide defaults, manifest for special cases

**Don't**: Hardcode configurations in client code

### 4. Error Handling

**Do**: Handle initialization timeout gracefully in client
```javascript
client.executePipeline({ manifest }, (err, response) => {
  if (err && err.code === grpc.status.DEADLINE_EXCEEDED) {
    console.log('Initialization timeout - increase init_timeout_secs');
  }
});
```

**Don't**: Retry indefinitely on process crashes

---

## Migration Guide

### Existing Pipelines

**Before** (all nodes in single Python process):
- End-to-end latency: 10+ seconds
- CPU utilization: 1 core
- Memory: 4GB (all models loaded together)

**After** (multiprocess execution):
- End-to-end latency: <500ms
- CPU utilization: 3 cores (concurrent processing)
- Memory: 4.5GB (overhead for separate processes)

**Required Changes**:
1. None! Existing manifests work as-is
2. Optional: Add multiprocess config to manifest metadata for tuning

### Client Code

**No changes required** for:
- gRPC client setup
- ExecutePipeline / StreamPipeline RPC calls
- Result handling

**Optional enhancements**:
- Monitor initialization progress (for long-running models)
- Handle process crash errors specifically

---

## Next Steps

- [API Reference](./contracts/grpc-service-extension.md) - Extended gRPC service behavior
- [Architecture](./data-model.md) - Detailed system design
- [Examples](../../examples/grpc-multiprocess/) - Sample implementations
- [Spec](./spec.md) - Feature specification
