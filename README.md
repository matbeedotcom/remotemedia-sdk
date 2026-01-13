# RemoteMedia SDK

A high-performance, multi-runtime pipeline execution framework for real-time media processing.

Build audio/video pipelines that run **locally** with native Rust performance (2-16x faster), or offload to **remote** servers and **Docker** containers seamlessly.

## Why RemoteMedia?

- **Native Rust Audio** - VAD, resampling, Whisper STT run 2-16x faster than Python
- **Transport Agnostic** - Same pipeline runs over gRPC, HTTP/SSE, WebRTC, or Python FFI
- **Zero-Copy IPC** - Process-isolated Python nodes with iceoryx2 shared memory
- **Production Ready** - Stream health monitoring, capability validation, resource limits

## Quick Start

### Option A: CLI (Fastest)

```bash
# Build and install
cd examples/cli/remotemedia-cli
cargo install --path .

# List available nodes (41+ built-in nodes)
remotemedia nodes list

# Filter nodes by name
remotemedia nodes list --filter whisper

# Validate a pipeline manifest
remotemedia validate pipeline.yaml

# Run a pipeline with input file
remotemedia run pipeline.yaml -i audio.wav

# Run with output file
remotemedia run pipeline.yaml -i audio.wav -O result.txt

# Stream mode with stdin/stdout
cat audio.wav | remotemedia stream pipeline.yaml -i - -O -
```

See [CLI Reference](examples/cli/remotemedia-cli/README.md) for full documentation.

### Option B: Rust Library

```rust
use remotemedia_runtime_core::transport::PipelineRunner;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runner = PipelineRunner::new()?;
    let manifest = Manifest::from_file("pipeline.yaml")?;

    let output = runner.execute_unary(manifest.into(), input).await?;
    println!("{:?}", output.data);
    Ok(())
}
```

### Option C: Python SDK

```python
from remotemedia.core import Pipeline
from remotemedia.nodes import MediaReaderNode, AudioResampleNode

pipeline = Pipeline(
    MediaReaderNode(file_path="input.mp3"),
    AudioResampleNode(target_sample_rate=16000),
)
pipeline.run()
```

### Option D: gRPC Server

```bash
# Start the server
cargo run --bin grpc-server --release -p remotemedia-grpc

# Connect from any gRPC client (Python, Node.js, Go, etc.)
```

## Built-in Nodes

### Audio Processing (Rust - Native Performance)

| Node | Description |
|------|-------------|
| `WhisperNode` | Speech-to-text transcription (Whisper) |
| `SileroVADNode` | Voice activity detection |
| `ResampleStreamingNode` | High-quality audio resampling |
| `AudioChunkerNode` | Split audio into fixed-size chunks |
| `AudioLevelNode` | RMS/peak level metering |
| `SpeakerDiarizationNode` | Identify who spoke when |
| `ClippingDetectorNode` | Detect audio clipping |
| `SilenceDetectorNode` | Detect silence periods |

### Audio Processing (Python)

| Node | Description |
|------|-------------|
| `KokoroTTSNode` | Text-to-speech synthesis |
| `WhisperXTranscriber` | WhisperX transcription with alignment |
| `AudioResampler` | Python-based resampling |
| `VoiceActivityDetector` | Python VAD wrapper |

### I/O Nodes

| Node | Description |
|------|-------------|
| `MicInputNode` | Microphone input capture |
| `SpeakerOutputNode` | Speaker/headphone output |
| `MediaReaderNode` | Read audio/video files |
| `MediaWriterNode` | Write audio/video files |

### Utility Nodes

| Node | Description |
|------|-------------|
| `PassThrough` | Forward data unchanged |
| `RemotePipelineNode` | Execute on remote server |
| `TextCollectorNode` | Accumulate text segments |

See [docs/NODE_CAPABILITIES.md](docs/NODE_CAPABILITIES.md) for full node reference.

## Project Structure

```
remotemedia-sdk/
├── runtime-core/          # Core execution engine (transport-agnostic)
├── transports/
│   ├── grpc/              # gRPC transport + server binary
│   ├── http/              # HTTP/REST with SSE streaming
│   ├── ffi/               # Python FFI (PyO3)
│   └── webrtc/            # WebRTC real-time streaming
├── libs/
│   ├── pipeline-runner/   # Shared pipeline execution
│   └── stream-health-analyzer/  # Health monitoring utilities
├── services/
│   └── ingest-srt/        # SRT ingest gateway
├── python-client/         # Python SDK
├── examples/              # Example applications
│   ├── cli/               # Command-line tool
│   ├── voice-assistant/   # Tauri desktop app
│   ├── video-transcription/  # PyQt transcription app
│   └── shared-pipelines/  # Reusable pipeline manifests
└── docs/                  # Documentation
```

## Documentation

| Guide | Description |
|-------|-------------|
| [Runtime Core](runtime-core/README.md) | Core library API and usage |
| [Transports](transports/README.md) | Transport implementations and error formats |
| [CLI Reference](examples/cli/remotemedia-cli/README.md) | Command-line tool usage |
| [Examples](examples/README.md) | Example applications overview |
| [Custom Transport Guide](docs/CUSTOM_TRANSPORT_GUIDE.md) | Build your own transport |
| [Custom Nodes](docs/CUSTOM_NODE_REGISTRATION.md) | Register custom processing nodes |
| [Performance Tuning](docs/PERFORMANCE_TUNING.md) | Optimization strategies |
| [Architecture](docs/TRANSPORT_DECOUPLING_ARCHITECTURE.md) | System design deep-dive |

## Requirements

| Component | Version |
|-----------|---------|
| Rust | 1.87+ |
| Python | 3.10+ (for Python nodes) |
| Node.js | 18+ (for JS/TS clients) |
| FFmpeg | 7.x (for media I/O) |

### Platform Support

| Platform | Status |
|----------|--------|
| Linux (x86_64) | Full support |
| macOS (Apple Silicon) | Full support |
| macOS (Intel) | Full support |
| Windows | Partial (no iceoryx2 IPC) |

## Building from Source

```bash
# Clone
git clone https://github.com/matbeeDOTcom/remotemedia-sdk
cd remotemedia-sdk

# Build all crates
cargo build --release

# Run tests
cargo test

# Build Python client
cd python-client && pip install -e .
```

## License

MIT OR Apache-2.0

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
