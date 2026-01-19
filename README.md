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
use remotemedia_runtime_core::{
    manifest::Manifest,
    transport::{PipelineExecutor, StreamSession, TransportData},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executor = PipelineExecutor::new()?;
    let manifest = Manifest::from_file("pipeline.yaml")?;

    // Create a streaming session
    let mut session = executor.create_session(manifest.into()).await?;

    // Send input and receive output
    session.send_input(TransportData::new(input)).await?;
    if let Some(output) = session.recv_output().await? {
        println!("{:?}", output.data);
    }

    session.close().await?;
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

### Option D: Server Binaries

```bash
# gRPC server (high-performance pipeline execution)
cargo run -p remotemedia-grpc-server --release

# HTTP server (REST API with SSE streaming)
cargo run -p remotemedia-http-server --release

# WebRTC server (real-time media streaming)
cargo run -p remotemedia-webrtc-server --release -- --mode grpc

# Connect from any client (Python, Node.js, Go, browser, etc.)
```

### Option E: Packaged Pipelines (Self-Contained Wheels)

Create **self-contained Python wheels** from pipeline manifests. The wheel bundles:
- Pre-compiled Python node bytecode
- Bundled `remotemedia` runtime
- Native Rust acceleration

```bash
# Pack a pipeline into a Python wheel
cargo run -p remotemedia-pack -- python examples/shared-pipelines/tts.yaml \
    --output /tmp/packed

# Build the wheel with maturin
cd /tmp/packed/tts && maturin build --release

# Install anywhere (no remotemedia install needed!)
pip install target/wheels/tts-0.1.0-*.whl

# Use in Python
python -c "
import tts
import asyncio

async def main():
    session = tts.TtsSession()
    result = await session.send({'type': 'text', 'data': 'Hello world!'})
    print(result)
    session.close()

asyncio.run(main())
"
```

**Key Features:**
- **Self-contained** - No external `remotemedia` package required
- **Pre-compiled bytecode** - Python nodes compiled at pack time, not runtime
- **Native acceleration** - Rust nodes compiled into the wheel
- **Dependency bundled** - All Python dependencies included

See [Pack Pipeline Reference](tools/pack-pipeline/README.md) for full documentation.

## Built-in Nodes

41+ built-in nodes for audio, video, and text processing.

### Audio & Transcription (Rust)

| Node | Description |
|------|-------------|
| `RustWhisperNode` | Speech-to-text (Whisper, auto-downloads models) |
| `SileroVADNode` | Voice activity detection (ONNX) |
| `FastResampleNode` | High-quality audio resampling |
| `AudioChunkerNode` | Split audio into fixed-size chunks |
| `AudioLevelNode` | RMS/peak level metering |
| `ClippingDetectorNode` | Detect audio clipping |
| `SilenceDetectorNode` | Detect silence periods |
| `ChannelBalanceNode` | Detect stereo imbalance |

### Text-to-Speech (Python)

| Node | Description |
|------|-------------|
| `KokoroTTSNode` | TTS with 9 languages, streaming output |
| `VibeVoiceTTSNode` | TTS with voice cloning support |

### Transcription (Python)

| Node | Description |
|------|-------------|
| `WhisperXNode` | WhisperX with word-level timestamps |
| `HFWhisperNode` | HuggingFace Whisper models |

### I/O & Utility

| Node | Description |
|------|-------------|
| `MediaReaderNode` | Read audio/video files |
| `MediaWriterNode` | Write audio/video files |
| `PassThrough` | Forward data unchanged |
| `RemotePipelineNode` | Execute on remote server |
| `TextCollectorNode` | Accumulate text into sentences |

**[View Full Node Reference](docs/NODES.md)** - Complete documentation with all parameters.

## Project Structure

```
remotemedia-sdk/
├── crates/
│   ├── core/              # Core execution engine (transport-agnostic)
│   └── transports/
│       ├── grpc/          # gRPC transport library
│       ├── http/          # HTTP/REST transport library
│       ├── ffi/           # Python FFI (PyO3)
│       └── webrtc/        # WebRTC transport library
├── tools/
│   └── pack-pipeline/     # Create self-contained Python wheels
├── clients/
│   ├── python/            # Python SDK
│   └── nodejs/            # Node.js SDK
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
| [Node Reference](docs/NODES.md) | Complete node documentation with parameters |
| [Runtime Core](runtime-core/README.md) | Core library API and usage |
| [Transports](transports/README.md) | Transport implementations and error formats |
| [CLI Reference](examples/cli/remotemedia-cli/README.md) | Command-line tool usage |
| [Pack Pipeline](tools/pack-pipeline/README.md) | Create self-contained Python wheels |
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
