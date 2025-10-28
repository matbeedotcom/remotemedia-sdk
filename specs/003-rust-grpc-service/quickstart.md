# Quickstart Guide: Rust gRPC Service

**Feature**: `003-rust-grpc-service` | **Audience**: Developers integrating the service  
**Generated**: 2025-10-28

This guide helps you build, configure, run, and test the Rust gRPC service for remote audio pipeline execution. By the end of this guide, you'll have a working service instance and know how to execute pipelines from client applications.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Building the Service](#building-the-service)
3. [Configuration](#configuration)
4. [Running the Service](#running-the-service)
5. [Testing with ghz](#testing-with-ghz)
6. [Client Integration](#client-integration)
7. [Troubleshooting](#troubleshooting)

---

## Prerequisites

Before building the service, ensure you have:

### Required Software

- **Rust 1.75+** (stable toolchain)
  ```bash
  # Install via rustup
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  
  # Verify installation
  rustc --version  # Should show 1.75 or higher
  ```

- **Protocol Buffers Compiler (protoc)**
  ```bash
  # macOS (via Homebrew)
  brew install protobuf
  
  # Linux (Ubuntu/Debian)
  sudo apt-get install protobuf-compiler
  
  # Windows (via Chocolatey)
  choco install protoc
  
  # Verify installation
  protoc --version  # Should show libprotoc 3.21.0 or higher
  ```

- **Python 3.9+** (for client SDK)
  ```bash
  python --version  # Should show 3.9 or higher
  ```

- **Node.js 18+** (for TypeScript client)
  ```bash
  node --version  # Should show v18 or higher
  ```

### Optional Tools

- **ghz** (gRPC load testing): [https://ghz.sh/](https://ghz.sh/)
  ```bash
  # macOS
  brew install ghz
  
  # Linux (download binary)
  wget https://github.com/bojand/ghz/releases/download/v0.117.0/ghz-linux-x86_64.tar.gz
  tar -xzf ghz-linux-x86_64.tar.gz
  sudo mv ghz /usr/local/bin/
  ```

- **grpcurl** (gRPC CLI testing): [https://github.com/fullstorydev/grpcurl](https://github.com/fullstorydev/grpcurl)
  ```bash
  brew install grpcurl  # macOS
  ```

---

## Building the Service

### Clone and Navigate

```bash
cd remotemedia-sdk/runtime
```

### Install Dependencies

The service dependencies are declared in `Cargo.toml`. Cargo will download them automatically during the first build.

**Key Dependencies**:
- `tonic = "0.10"` - gRPC framework
- `prost = "0.12"` - Protocol Buffers
- `tokio = { version = "1.35", features = ["full"] }` - Async runtime
- `serde_json = "1.0"` - JSON manifest parsing
- `tracing = "0.1"` - Structured logging
- `prometheus = "0.13"` - Metrics

### Build for Development

```bash
# Debug build (faster compilation, slower runtime)
cargo build

# Binary location: target/debug/grpc_server
```

### Build for Production

```bash
# Release build (optimized, production-ready)
cargo build --release

# Binary location: target/release/grpc_server
```

**Build Time**: Expect 2-5 minutes for the first build (downloads dependencies). Subsequent builds are incremental and much faster.

### Verify Build

```bash
# Run the binary with --version flag
./target/release/grpc_server --version

# Expected output:
# remotemedia-grpc-service v0.2.1
# Protocol: v1
# Runtime: 0.2.1
```

---

## Configuration

The service can be configured via environment variables or a YAML configuration file.

### Configuration File (config.yaml)

Create `config.yaml` in the working directory:

```yaml
# gRPC server configuration
server:
  # Server bind address
  host: "0.0.0.0"
  port: 50051
  
  # TLS configuration (optional, recommended for production)
  tls:
    enabled: true
    cert_path: "/path/to/server.crt"
    key_path: "/path/to/server.key"
    # Client certificate verification (mutual TLS)
    client_ca_path: "/path/to/client-ca.crt"

# Authentication configuration
auth:
  # API token validation
  enabled: true
  # Valid bearer tokens (in production, load from secure store)
  tokens:
    - "dev-token-12345"
    - "test-token-67890"

# Resource limits (service-wide defaults)
limits:
  # Default per-pipeline limits
  default_max_memory_bytes: 104857600  # 100MB
  default_max_timeout_ms: 5000         # 5 seconds
  default_max_audio_samples: 10485760  # 10M samples (~200MB stereo F32)
  
  # Maximum limits (clients cannot exceed these)
  max_memory_bytes: 1073741824         # 1GB
  max_timeout_ms: 30000                # 30 seconds
  max_audio_samples: 52428800          # 50M samples (~1GB stereo F32)
  
  # Concurrent execution limits
  max_concurrent_executions: 1000

# Observability configuration
observability:
  # Structured logging
  logging:
    format: "json"  # "json" or "pretty"
    level: "info"   # "trace", "debug", "info", "warn", "error"
    
  # Metrics endpoint (Prometheus)
  metrics:
    enabled: true
    port: 9090
    path: "/metrics"

# Runtime configuration
runtime:
  # Node registry (loaded from environment)
  node_types:
    - "AudioResample"
    - "VAD"
    - "AudioFormatConverter"
    - "HFPipelineNode"
```

### Environment Variables

Override configuration via environment variables:

```bash
# Server configuration
export GRPC_HOST="0.0.0.0"
export GRPC_PORT="50051"
export GRPC_TLS_ENABLED="true"
export GRPC_TLS_CERT="/path/to/server.crt"
export GRPC_TLS_KEY="/path/to/server.key"

# Authentication
export AUTH_ENABLED="true"
export AUTH_TOKENS="token1,token2,token3"

# Resource limits
export MAX_MEMORY_BYTES="104857600"
export MAX_TIMEOUT_MS="5000"
export MAX_CONCURRENT_EXECUTIONS="1000"

# Logging
export LOG_LEVEL="info"
export LOG_FORMAT="json"
```

**Priority**: Environment variables override values in `config.yaml`.

---

## Running the Service

### Development Mode (No TLS)

For local development and testing:

```bash
# Run with default configuration (no TLS, no auth)
./target/debug/grpc_server

# Expected output:
# {"timestamp":"2025-10-28T10:30:00Z","level":"INFO","message":"Starting gRPC service","host":"0.0.0.0","port":50051}
# {"timestamp":"2025-10-28T10:30:00Z","level":"INFO","message":"Service ready","protocol_version":"v1","runtime_version":"0.2.1"}
# {"timestamp":"2025-10-28T10:30:00Z","level":"INFO","message":"Metrics endpoint listening","port":9090}
```

**Warning**: This mode is INSECURE. Do not use in production.

### Production Mode (With TLS)

Generate TLS certificates (self-signed for testing):

```bash
# Generate CA key and certificate
openssl genrsa -out ca.key 2048
openssl req -new -x509 -days 365 -key ca.key -out ca.crt \
  -subj "/CN=RemoteMedia-CA"

# Generate server key and certificate
openssl genrsa -out server.key 2048
openssl req -new -key server.key -out server.csr \
  -subj "/CN=localhost"
openssl x509 -req -days 365 -in server.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out server.crt
```

Run with TLS:

```bash
# Using config file
./target/release/grpc_server --config config.yaml

# Using environment variables
GRPC_TLS_ENABLED=true \
GRPC_TLS_CERT=server.crt \
GRPC_TLS_KEY=server.key \
AUTH_ENABLED=true \
AUTH_TOKENS=my-secret-token \
./target/release/grpc_server
```

### Docker Deployment

```dockerfile
# Dockerfile
FROM rust:1.75 as builder
WORKDIR /build
COPY runtime/ .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/grpc_server /usr/local/bin/
EXPOSE 50051 9090
CMD ["grpc_server"]
```

Build and run:

```bash
docker build -t remotemedia-grpc:latest .
docker run -p 50051:50051 -p 9090:9090 \
  -e AUTH_ENABLED=true \
  -e AUTH_TOKENS=my-token \
  remotemedia-grpc:latest
```

### Health Check

Verify the service is running:

```bash
# Check gRPC endpoint (requires grpcurl)
grpcurl -plaintext localhost:50051 remotemedia.v1.PipelineExecutionService/GetVersion

# Expected response:
# {
#   "versionInfo": {
#     "protocolVersion": "v1",
#     "runtimeVersion": "0.2.1",
#     "supportedNodeTypes": ["AudioResample", "VAD", "AudioFormatConverter"],
#     "supportedProtocols": ["v1"],
#     "buildTimestamp": "2025-10-28T10:00:00Z"
#   },
#   "compatible": true
# }

# Check metrics endpoint
curl http://localhost:9090/metrics | grep grpc_requests_total
```

---

## Testing with ghz

### Simple Load Test

Test the service under load:

```bash
# Create test manifest (pipeline.json)
cat > pipeline.json << 'EOF'
{
  "version": "v1",
  "metadata": {"name": "test-resample"},
  "nodes": [
    {
      "id": "resample",
      "node_type": "AudioResample",
      "params": "{\"target_sample_rate\": 16000}"
    }
  ],
  "connections": []
}
EOF

# Create test request (request.json)
cat > request.json << 'EOF'
{
  "manifest": <PASTE_PIPELINE_JSON_HERE>,
  "audio_inputs": {
    "resample": {
      "samples": "<BASE64_ENCODED_AUDIO>",
      "sample_rate": 44100,
      "channels": 2,
      "format": "AUDIO_FORMAT_F32",
      "num_samples": 44100
    }
  },
  "client_version": "v1"
}
EOF

# Run load test: 100 requests/sec for 10 seconds
ghz --insecure \
  --proto contracts/execution.proto \
  --call remotemedia.v1.PipelineExecutionService/ExecutePipeline \
  --data-file request.json \
  --rps 100 \
  --duration 10s \
  --connections 50 \
  localhost:50051

# Expected output (meeting SC-001):
# Summary:
#   Count:        1000
#   Total:        10.00 s
#   Slowest:      8.23 ms
#   Fastest:      1.12 ms
#   Average:      3.45 ms  ← Should be <5ms (SC-001)
#   Requests/sec: 100.00
```

### Streaming Load Test

Test streaming pipeline:

```bash
# This requires a custom ghz script for bidirectional streaming
# See ghz documentation: https://ghz.sh/docs/examples#bidirectional-streaming
```

---

## Client Integration

### Python Client

**Install Dependencies**:

```bash
cd python-client
pip install grpcio grpcio-tools
```

**Generate Python Stubs**:

```bash
# From repository root
python -m grpc_tools.protoc \
  -I specs/003-rust-grpc-service/contracts \
  --python_out=python-client/remotemedia/proto \
  --grpc_python_out=python-client/remotemedia/proto \
  common.proto execution.proto streaming.proto
```

**Example Usage**:

```python
import grpc
from remotemedia.proto import execution_pb2, execution_pb2_grpc

# Connect to service
channel = grpc.insecure_channel('localhost:50051')
stub = execution_pb2_grpc.PipelineExecutionServiceStub(channel)

# Create pipeline manifest
manifest = execution_pb2.PipelineManifest(
    version="v1",
    metadata=execution_pb2.ManifestMetadata(name="test-pipeline"),
    nodes=[
        execution_pb2.NodeManifest(
            id="resample",
            node_type="AudioResample",
            params='{"target_sample_rate": 16000}'
        )
    ],
    connections=[]
)

# Create audio input
audio = execution_pb2.AudioBuffer(
    samples=b'\x00' * 4 * 44100,  # 1 second of silence (F32)
    sample_rate=44100,
    channels=1,
    format=execution_pb2.AUDIO_FORMAT_F32,
    num_samples=44100
)

# Execute pipeline
request = execution_pb2.ExecuteRequest(
    manifest=manifest,
    audio_inputs={"resample": audio},
    client_version="v1"
)

response = stub.ExecutePipeline(request)

if response.HasField('result'):
    print(f"Success! Wall time: {response.result.metrics.wall_time_ms}ms")
    output_audio = response.result.audio_outputs['resample']
    print(f"Output: {output_audio.num_samples} samples @ {output_audio.sample_rate}Hz")
else:
    print(f"Error: {response.error.message}")
```

### TypeScript Client

**Install Dependencies**:

```bash
cd nodejs-client
npm install @grpc/grpc-js @grpc/proto-loader
```

**Generate TypeScript Stubs**:

```bash
# Install protoc plugin
npm install -g grpc-tools

# Generate stubs
grpc_tools_node_protoc \
  --plugin=protoc-gen-ts=./node_modules/.bin/protoc-gen-ts \
  --ts_out=grpc_js:nodejs-client/generated-types \
  --js_out=import_style=commonjs:nodejs-client/generated-types \
  --grpc_out=grpc_js:nodejs-client/generated-types \
  -I specs/003-rust-grpc-service/contracts \
  common.proto execution.proto streaming.proto
```

**Example Usage**:

```typescript
import * as grpc from '@grpc/grpc-js';
import { PipelineExecutionServiceClient } from './generated-types/execution_grpc_pb';
import { ExecuteRequest, PipelineManifest, AudioBuffer } from './generated-types/execution_pb';

// Connect to service
const client = new PipelineExecutionServiceClient(
  'localhost:50051',
  grpc.credentials.createInsecure()
);

// Create manifest
const manifest = new PipelineManifest();
manifest.setVersion('v1');
manifest.setMetadata({name: 'test-pipeline'});

// Create audio input
const audio = new AudioBuffer();
audio.setSamples(Buffer.alloc(4 * 44100)); // 1 second silence
audio.setSampleRate(44100);
audio.setChannels(1);
audio.setFormat(AudioFormat.AUDIO_FORMAT_F32);
audio.setNumSamples(44100);

// Execute pipeline
const request = new ExecuteRequest();
request.setManifest(manifest);
request.getAudioInputsMap().set('resample', audio);
request.setClientVersion('v1');

client.executePipeline(request, (err, response) => {
  if (err) {
    console.error('Error:', err);
    return;
  }
  
  if (response.hasResult()) {
    const result = response.getResult()!;
    console.log(`Success! Wall time: ${result.getMetrics()?.getWallTimeMs()}ms`);
  } else {
    console.log(`Error: ${response.getError()?.getMessage()}`);
  }
});
```

### Authentication

Add bearer token for authenticated requests:

**Python**:

```python
import grpc

# Create channel with bearer token metadata
def token_metadata(context, callback):
    callback([('authorization', 'Bearer my-secret-token')], None)

auth_credentials = grpc.metadata_call_credentials(token_metadata)
channel_credentials = grpc.ssl_channel_credentials()
composite_credentials = grpc.composite_channel_credentials(
    channel_credentials, auth_credentials
)

channel = grpc.secure_channel('localhost:50051', composite_credentials)
```

**TypeScript**:

```typescript
const metadata = new grpc.Metadata();
metadata.add('authorization', 'Bearer my-secret-token');

client.executePipeline(request, metadata, (err, response) => {
  // ...
});
```

---

## Troubleshooting

### Common Issues

#### 1. Connection Refused

**Symptom**:
```
Error: failed to connect to all addresses
```

**Solutions**:
- Verify service is running: `ps aux | grep grpc_server`
- Check port binding: `netstat -an | grep 50051`
- Check firewall rules: `sudo ufw status`

#### 2. TLS Certificate Errors

**Symptom**:
```
Error: x509: certificate signed by unknown authority
```

**Solutions**:
- For development, use `--insecure` flag or `grpc.credentials.createInsecure()`
- For production, ensure client has CA certificate: `grpc.ssl_channel_credentials(ca_cert)`
- Verify certificate chain: `openssl verify -CAfile ca.crt server.crt`

#### 3. Authentication Failures

**Symptom**:
```json
{"error_type": "ERROR_TYPE_AUTHENTICATION", "message": "Invalid bearer token"}
```

**Solutions**:
- Verify token is in service config: Check `AUTH_TOKENS` environment variable
- Ensure metadata is set: `metadata.add('authorization', 'Bearer <token>')`
- Check token format: Must be `Bearer <token>`, not just `<token>`

#### 4. Protocol Version Mismatch

**Symptom**:
```json
{"error_type": "ERROR_TYPE_VERSION_MISMATCH", "compatible": false}
```

**Solutions**:
- Upgrade client library to match service version
- Check supported versions: Call `GetVersion()` RPC
- Verify `client_version` field in request matches supported protocols

#### 5. Resource Limit Exceeded

**Symptom**:
```json
{"error_type": "ERROR_TYPE_RESOURCE_LIMIT", "message": "Memory limit exceeded"}
```

**Solutions**:
- Reduce audio buffer size or pipeline complexity
- Request higher limits in `ExecuteRequest.resource_limits`
- Check service-wide limits in config: `max_memory_bytes`, `max_timeout_ms`

### Logs Analysis

**View structured logs** (JSON format):

```bash
# Tail logs in real-time
./target/release/grpc_server 2>&1 | jq -C

# Filter by log level
./target/release/grpc_server 2>&1 | jq 'select(.level == "ERROR")'

# Filter by message pattern
./target/release/grpc_server 2>&1 | jq 'select(.message | contains("ExecutePipeline"))'
```

**Key log fields**:
- `timestamp`: ISO 8601 timestamp
- `level`: Log level (TRACE, DEBUG, INFO, WARN, ERROR)
- `message`: Human-readable message
- `request_id`: Correlation ID for request tracking
- `wall_time_ms`: Execution time (for performance analysis)

### Metrics Analysis

**Query Prometheus metrics**:

```bash
# Request count by RPC method
curl -s http://localhost:9090/metrics | grep grpc_requests_total

# Request latency histogram (p50, p95, p99)
curl -s http://localhost:9090/metrics | grep grpc_request_duration_seconds

# Active concurrent executions
curl -s http://localhost:9090/metrics | grep grpc_active_executions

# Memory usage
curl -s http://localhost:9090/metrics | grep process_resident_memory_bytes
```

---

## Next Steps

- **Production Deployment**: See `docs/DEPLOYMENT.md` (Phase 4)
- **Performance Tuning**: See `docs/PERFORMANCE_TUNING.md` (Phase 4)
- **Client SDKs**: Explore `python-client/` and `nodejs-client/` directories
- **Advanced Features**: Streaming pipelines, custom node types

---

## Support

- **Documentation**: `/specs/003-rust-grpc-service/`
- **Issues**: GitHub Issues (link TBD)
- **Slack**: #remotemedia-grpc (link TBD)
