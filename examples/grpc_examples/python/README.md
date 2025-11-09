# Python gRPC Client Examples

Practical examples demonstrating the Rust gRPC service from Python.

## Prerequisites

```bash
# Install Python client
cd python-grpc-client
pip install -r requirements.txt

# Generate proto stubs
python generate_protos.py
```

## Running Examples

Ensure the Rust gRPC server is running:

```bash
cd runtime
cargo run --bin grpc_server --features grpc-transport
```

Then run examples (in another terminal):

```bash
cd examples/grpc_examples/python

# Simple calculator pipeline
python simple_execution.py

# Multi-node pipeline
python multi_node_pipeline.py

# Bidirectional streaming
python streaming_example.py
```

## Examples

### 1. simple_execution.py

**What it demonstrates**:
- Connecting to the service
- Version compatibility checking
- Executing simple calculator pipelines
- Handling results and errors

**Expected output**:
```
✅ Connected to service v1
   Runtime version: 0.2.1
   
=== Executing Calculator Pipeline ===
✅ Execution successful
   Input: 10.0
   Operation: add 5.0
   Result: 15.0
   Wall time: 3.45ms
```

**Performance**: ~3-5ms per execution

### 2. multi_node_pipeline.py

**What it demonstrates**:
- Chaining multiple nodes together
- Using connections to pass data between nodes
- Processing audio through multiple stages
- Per-node metrics collection

**Expected output**:
```
=== Multi-Node Pipeline: PassThrough -> Echo ===
✅ Execution successful
   Wall time: 4.23ms
   Nodes executed: 2
   - passthrough: 1.12ms
   - echo: 1.05ms
   
   Output audio: 16000 samples @ 16000Hz
```

**Performance**: ~4-6ms for 2-node pipeline

### 3. streaming_example.py

**What it demonstrates**:
- Bidirectional streaming
- Chunked audio processing
- Real-time latency measurement
- Session management

**Expected output**:
```
=== Processing Chunks ===
Chunk  0:   0.04ms (  1600 samples total)
Chunk  1:   0.03ms (  3200 samples total)
...
Chunk 19:   0.04ms ( 32000 samples total)

=== Statistics ===
Total chunks: 20
Average latency: 0.04ms
✅ Target met: 0.04ms < 50ms
```

**Performance**: ~0.04ms per chunk (100ms audio)

## Error Handling

All examples include comprehensive error handling:

```python
try:
    result = await client.execute_pipeline(manifest, {}, {})
except RemoteMediaError as e:
    print(f"Error: {e.message}")
    print(f"Type: {e.error_type.name}")
    print(f"Node: {e.failing_node_id}")
```

## Authentication

To use with authentication enabled:

```python
# Pass API key when creating client
client = RemoteMediaClient(
    address="localhost:50051",
    api_key="your-secret-token"
)
```

## Next Steps

- Try modifying pipeline parameters
- Chain more complex node sequences
- Experiment with different chunk sizes for streaming
- Add custom error handling logic
- Integrate into your application

## Reference

- **Client API**: See `python-grpc-client/README.md`
- **Proto contracts**: See `specs/003-rust-grpc-service/contracts/`
- **Server docs**: See `specs/003-rust-grpc-service/QUICKSTART.md`
