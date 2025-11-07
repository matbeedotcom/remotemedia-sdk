# RemoteMedia SDK Examples

Welcome to the RemoteMedia SDK examples! This directory contains examples organized by complexity and use case to help you learn and build with the SDK.

## Quick Start

**New to RemoteMedia?** Start here:
1. [Hello Pipeline](00-getting-started/01-hello-pipeline/) - Your first RemoteMedia pipeline in 5 minutes
2. [Basic Audio Processing](00-getting-started/02-basic-audio/) - Process audio with native Rust nodes
3. [Python-Rust Interop](00-getting-started/03-python-rust-interop/) - Combine Python and Rust nodes

**Looking for something specific?** Browse by:
- **Complexity** → [Getting Started](#getting-started) | [Advanced](#advanced) | [Applications](#applications)
- **Transport** → [gRPC](#by-transport) | [FFI](#by-transport) | [WebRTC](#by-transport)
- **Feature** → [Audio Processing](#by-feature) | [Multiprocess](#by-feature) | [Streaming](#by-feature)

---

## Getting Started

**Directory**: [00-getting-started/](00-getting-started/)

Beginner-friendly examples with step-by-step tutorials. Perfect for your first experience with RemoteMedia SDK.

**Prerequisites**: Basic Python knowledge, 10-15 minutes per example

| Example | Description | Time |
|---------|-------------|------|
| [01-hello-pipeline](00-getting-started/01-hello-pipeline/) | Create your first pipeline with audio processing | 5 min |
| [02-basic-audio](00-getting-started/02-basic-audio/) | Resample and process audio files | 10 min |
| [03-python-rust-interop](00-getting-started/03-python-rust-interop/) | Use Rust-accelerated nodes from Python | 15 min |

**What you'll learn**:
- Pipeline manifest format (YAML)
- Connecting nodes and data flow
- Audio processing basics
- Runtime selection (Python vs Rust)

---

## Advanced

**Directory**: [01-advanced/](01-advanced/)

Production-ready patterns and advanced features for experienced users.

**Prerequisites**: Completed Getting Started examples, understanding of async Python or Rust

| Example | Description | Complexity |
|---------|-------------|------------|
| [multiprocess-nodes](01-advanced/multiprocess-nodes/) | Run Python nodes in separate processes with iceoryx2 IPC | ⭐⭐⭐ |
| [streaming-pipelines](01-advanced/streaming-pipelines/) | Real-time audio streaming with VAD | ⭐⭐⭐ |
| [custom-transports](01-advanced/custom-transports/) | Build custom transport layer | ⭐⭐⭐⭐ |
| [grpc-remote-execution](01-advanced/grpc-remote-execution/) | Execute pipelines on remote gRPC servers | ⭐⭐ |

**What you'll learn**:
- Performance optimization techniques
- Process isolation and IPC
- Custom node development
- Streaming data patterns
- Remote execution architectures

---

## Applications

**Directory**: [02-applications/](02-applications/)

Complete, production-ready applications demonstrating real-world use cases.

**Prerequisites**: Familiarity with web frameworks, deployment concepts

| Application | Description | Stack |
|-------------|-------------|-------|
| [nextjs-tts-app](02-applications/nextjs-tts-app/) | Next.js web app with text-to-speech streaming | Next.js + Python + Rust |
| [webrtc-processor](02-applications/webrtc-processor/) | Real-time WebRTC audio processing | WebRTC + Rust |

**What you'll learn**:
- Full-stack integration patterns
- Deployment and scaling
- Real-time web audio
- Production monitoring

---

## Browse by Transport

**Directory**: [by-transport/](by-transport/)

Examples organized by the transport layer they use. Great for learning a specific integration method.

### gRPC Transport
**See**: [by-transport/grpc/](by-transport/grpc/)

Execute pipelines on remote servers using gRPC. Ideal for microservices and distributed systems.

**Examples**:
- [grpc-streaming-client](by-transport/grpc/streaming-client/) - Bidirectional streaming
- [grpc-authentication](by-transport/grpc/authentication/) - Secure gRPC connections
- [grpc-load-balancing](by-transport/grpc/load-balancing/) - Scale across multiple servers

### FFI Transport
**See**: [by-transport/ffi/](by-transport/ffi/)

Python-to-Rust FFI for maximum performance. Use when you need native speed from Python.

**Examples**:
- [ffi-audio-processing](by-transport/ffi/audio-processing/) - Zero-copy audio with numpy
- [ffi-performance-comparison](by-transport/ffi/performance/) - Benchmark Python vs Rust

### WebRTC Transport
**See**: [by-transport/webrtc/](by-transport/webrtc/)

Real-time communication for browser-based audio/video processing.

**Examples**:
- [webrtc-echo-cancellation](by-transport/webrtc/echo-cancellation/) - Browser-based AEC
- [webrtc-streaming-vad](by-transport/webrtc/streaming-vad/) - Voice activity detection

---

## Browse by Feature

**Directory**: [by-feature/](by-feature/)

Examples organized by specific SDK features. Perfect for finding solutions to specific problems.

### Audio Processing
**See**: [by-feature/audio-processing/](by-feature/audio-processing/)

All examples involving audio manipulation, resampling, format conversion, and effects.

**Key Examples**:
- High-quality resampling (rubato)
- Voice Activity Detection (Silero VAD)
- Format conversion (PCM, opus, etc.)
- Audio effects and filters

### Multiprocess
**See**: [by-feature/multiprocess/](by-feature/multiprocess/)

Process isolation, IPC with iceoryx2, and distributed execution patterns.

**Key Examples**:
- Multiprocess Python nodes
- Zero-copy IPC with iceoryx2
- Process health monitoring
- Fault isolation

### Streaming
**See**: [by-feature/streaming/](by-feature/streaming/)

Real-time data processing, chunked execution, and streaming audio/video.

**Key Examples**:
- Bidirectional streaming
- Chunked audio processing
- Real-time VAD pipelines
- WebRTC integration

---

## Shared Assets

**Directory**: [assets/](assets/)

Shared audio files, models, and data used across multiple examples.

**Contents**:
- `transcribe_demo.wav` - Sample audio for transcription examples
- `sample_audio_16k.wav` - 16kHz audio for VAD testing
- (More assets added as examples grow)

**Usage**: Reference from any example using relative paths:
```yaml
input_audio: "../assets/transcribe_demo.wav"
```

---

## Running Examples

### Prerequisites

**System Requirements**:
- Python 3.9+ or Rust 1.75+
- 4GB RAM minimum
- Linux, macOS, or Windows

**Install SDK**:
```bash
# Python
pip install remotemedia>=0.4.0

# Rust (for Rust-based examples)
cargo install remotemedia-runtime
```

### Basic Workflow

1. **Navigate to example**:
   ```bash
   cd examples/00-getting-started/01-hello-pipeline/
   ```

2. **Read the README**:
   ```bash
   cat README.md
   ```

3. **Install dependencies**:
   ```bash
   pip install -r requirements.txt
   ```

4. **Run the example**:
   ```bash
   python main.py
   ```

### Example-Specific Instructions

Each example includes:
- ✅ **README.md** - Complete setup and usage guide
- ✅ **requirements.txt** - Python dependencies
- ✅ **pipeline.yaml** - Pipeline configuration
- ✅ **main.py** or **main.rs** - Entry point
- ✅ **Expected output** - What success looks like

---

## Getting Help

**Documentation**: [docs.remotemedia.dev](https://docs.remotemedia.dev)
**Issues**: [GitHub Issues](https://github.com/org/remotemedia-sdk/issues)
**Contributing**: See [CONTRIBUTING.md](../CONTRIBUTING.md)

**Common Issues**:
- **"Module not found"** → Install SDK: `pip install remotemedia`
- **"Rust runtime not available"** → Fallback to Python (slower but works)
- **"iceoryx2 error"** → Check multiprocess setup in example README

---

## Contributing Examples

Want to add an example? See our [Example Creation Guide](../CONTRIBUTING.md#example-creation-guide).

**Quick checklist**:
- ✅ Use the [example README template](../specs/001-repo-cleanup/contracts/example-readme-template.md)
- ✅ Choose appropriate complexity tier (00/01/02)
- ✅ Include all required sections (Prerequisites, Quick Start, Expected Output)
- ✅ Test on clean environment
- ✅ Add to this navigation README

---

**Last Updated**: 2025-11-07
**SDK Version**: v0.4.0+
**Total Examples**: Growing! Check directories for latest additions.
