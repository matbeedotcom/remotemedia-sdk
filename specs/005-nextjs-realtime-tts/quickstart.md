# Quickstart Guide: Real-Time TTS Web Application

**Feature**: 005-nextjs-realtime-tts
**Date**: 2025-10-29

## Overview

This guide walks you through setting up the development environment and running the real-time text-to-speech web application locally.

## Prerequisites

### Required Software

- **Node.js**: v18.17+ or v20+ (LTS recommended)
- **pnpm**: v8+ (or npm/yarn)
- **Python**: 3.11+ with pip
- **Rust**: 1.75+ with cargo (for building gRPC service)
- **Git**: For cloning the repository

### System Requirements

- **OS**: Windows, macOS, or Linux
- **RAM**: 4GB+ (8GB recommended for comfortable development)
- **Disk**: 2GB free space
- **Network**: Internet connection for downloading dependencies

## Quick Start (5 minutes)

### 1. Clone Repository

```bash
git clone https://github.com/yourusername/remotemedia-sdk.git
cd remotemedia-sdk
git checkout 005-nextjs-realtime-tts
```

### 2. Install Rust gRPC Service Dependencies

```bash
cd runtime
cargo build --release --features grpc-transport
cd ..
```

### 3. Install Python TTS Dependencies

```bash
cd examples/audio_examples
pip install -r requirements.txt
# Install Kokoro TTS
pip install kokoro>=0.9.4 soundfile
cd ../..
```

### 4. Install Next.js Frontend Dependencies

```bash
cd examples/nextjs-tts-app
pnpm install
cd ..
```

### 5. Start Development Servers

**Terminal 1: Start Rust gRPC Service**
```bash
cd runtime
RUST_LOG=info cargo run --bin grpc-server --features grpc-transport
# Server starts on localhost:50051
```

**Terminal 2: Start Next.js Dev Server**
```bash
cd examples/nextjs-tts-app
pnpm dev
# Frontend starts on http://localhost:3000
```

### 6. Test the Application

1. Open browser to http://localhost:3000
2. Type "Hello, world!" in the text input
3. Click "Speak"
4. Hear synthesized speech within 2 seconds

**Success!** 🎉 You now have a working real-time TTS application.

## Detailed Setup

### Environment Configuration

#### Backend (Rust gRPC Service)

Create `runtime/.env`:
```bash
# gRPC server configuration
GRPC_BIND_ADDRESS=127.0.0.1:50051
GRPC_REQUIRE_AUTH=false           # Disable auth for local dev
GRPC_MAX_MEMORY_MB=500            # Max memory per request
GRPC_MAX_TIMEOUT_SEC=300          # 5 minute timeout for long TTS
RUST_LOG=info                     # Logging level

# Python node configuration
PYTHON_NODE_PATH=../examples/audio_examples
```

#### Frontend (Next.js)

Create `examples/nextjs-tts-app/.env.local`:
```bash
# gRPC service endpoint
NEXT_PUBLIC_GRPC_HOST=localhost
NEXT_PUBLIC_GRPC_PORT=50051
NEXT_PUBLIC_GRPC_SSL=false        # Disable SSL for local dev

# Feature flags
NEXT_PUBLIC_ENABLE_VOICE_SELECTION=true
NEXT_PUBLIC_ENABLE_SPEED_CONTROL=true
NEXT_PUBLIC_MAX_TEXT_LENGTH=10000

# Development
NEXT_PUBLIC_DEBUG_MODE=true       # Enable debug logging
```

### Development Workflow

#### Running Tests

**Rust Backend Tests**:
```bash
cd runtime
cargo test --features grpc-transport
```

**Frontend Tests**:
```bash
cd examples/nextjs-tts-app
pnpm test              # Unit tests (Jest)
pnpm test:e2e          # E2E tests (Playwright)
```

#### Linting and Formatting

**Rust**:
```bash
cd runtime
cargo fmt              # Format code
cargo clippy           # Lint code
```

**TypeScript**:
```bash
cd examples/nextjs-tts-app
pnpm lint              # ESLint
pnpm format            # Prettier
```

#### Building for Production

**Rust Service**:
```bash
cd runtime
cargo build --release --features grpc-transport
# Binary: target/release/grpc-server
```

**Frontend**:
```bash
cd examples/nextjs-tts-app
pnpm build             # Next.js production build
pnpm start             # Start production server
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Browser                              │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │         Next.js Frontend (Port 3000)                 │  │
│  │                                                       │  │
│  │  - React Components (TextInput, AudioPlayer, etc.)   │  │
│  │  - Web Audio API (audio playback)                    │  │
│  │  - gRPC Client (TypeScript)                          │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          │ gRPC Streaming                   │
│                          │ (Audio Chunks)                   │
└──────────────────────────┼──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              Rust gRPC Service (Port 50051)                 │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │     StreamingPipelineService                         │  │
│  │                                                       │  │
│  │  - Bidirectional Streaming (streaming.rs)            │  │
│  │  - Pipeline Execution (executor.rs)                  │  │
│  │  - Node Registry (nodes/mod.rs)                      │  │
│  └──────────────────────────────────────────────────────┘  │
│                          │                                  │
│                          │ IPC / PyO3                       │
│                          │                                  │
│  ┌──────────────────────▼───────────────────────────────┐  │
│  │     Python Node Wrapper                              │  │
│  │                                                       │  │
│  │  - KokoroTTSNode Adapter (Rust → Python)            │  │
│  │  - Audio Buffer Conversion                           │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────┼──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│          Python TTS Node (KokoroTTSNode)                    │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │     Kokoro TTS Engine                                │  │
│  │                                                       │  │
│  │  - Text → Phonemes → Audio                           │  │
│  │  - Chunked Streaming (100ms chunks)                  │  │
│  │  - Multi-language Support                            │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Troubleshooting

### Common Issues

#### 1. gRPC Connection Refused

**Symptom**: Frontend shows "Connection refused" or "Service unavailable"

**Solutions**:
```bash
# Check if gRPC server is running
lsof -i :50051  # macOS/Linux
netstat -an | findstr 50051  # Windows

# Check firewall (allow port 50051)
# Check .env.local has correct GRPC_HOST and GRPC_PORT
```

#### 2. Kokoro TTS Not Found

**Symptom**: Error "Kokoro TTS is not installed"

**Solutions**:
```bash
# Install Kokoro TTS
pip install kokoro>=0.9.4 soundfile

# Verify installation
python -c "from kokoro import KPipeline; print('OK')"
```

#### 3. Audio Not Playing

**Symptom**: TTS succeeds but no audio

**Solutions**:
- Check browser console for Web Audio errors
- Verify browser audio permissions
- Try different browser (Chrome/Firefox recommended)
- Check system audio output/volume

#### 4. High Latency (>5s)

**Symptom**: Long delay before audio starts

**Solutions**:
```bash
# Check CPU usage during synthesis
# Kokoro requires significant CPU for first chunk

# Enable hardware acceleration
export OMP_NUM_THREADS=4  # Use 4 CPU cores

# Check network latency
ping localhost  # Should be <1ms
```

#### 5. Build Errors

**Rust compilation errors**:
```bash
# Update Rust toolchain
rustup update stable

# Clean build
cargo clean
cargo build --release
```

**Node.js errors**:
```bash
# Clear cache
pnpm store prune
rm -rf node_modules
pnpm install
```

## Development Tips

### Hot Reload

- **Frontend**: Next.js has hot reload by default (Fast Refresh)
- **Backend**: Rust service requires restart after code changes
  - Use `cargo watch` for auto-restart: `cargo watch -x 'run --bin grpc-server'`

### Debugging

**Frontend**:
```typescript
// Enable debug logging in browser console
localStorage.setItem('DEBUG', 'tts:*');

// View gRPC messages
window.__GRPC_DEBUG__ = true;
```

**Backend**:
```bash
# Verbose Rust logging
RUST_LOG=debug,remotemedia=trace cargo run

# gRPC message tracing
RUST_LOG=tonic=trace cargo run
```

**Python**:
```python
# Enable Python logging in KokoroTTSNode
import logging
logging.basicConfig(level=logging.DEBUG)
```

### Performance Profiling

**Frontend**:
```bash
# React DevTools Profiler
# Chrome → DevTools → Profiler tab

# Lighthouse audit
pnpm build
pnpm start
# Open Chrome DevTools → Lighthouse
```

**Backend**:
```bash
# Rust flamegraph
cargo install flamegraph
sudo flamegraph -- ./target/release/grpc-server

# Python profiling
python -m cProfile -o output.pstats kokoro_tts.py
```

## Next Steps

1. **Implement Frontend**: Follow `/speckit.tasks` to create React components
2. **Integrate Python Node**: Register KokoroTTSNode with Rust runtime
3. **Add Tests**: Write unit and integration tests
4. **Polish UI**: Add styling with Tailwind CSS
5. **Deploy**: Deploy to production environment

## Resources

### Documentation

- **Feature Spec**: `specs/005-nextjs-realtime-tts/spec.md`
- **Implementation Plan**: `specs/005-nextjs-realtime-tts/plan.md`
- **API Contract**: `specs/005-nextjs-realtime-tts/contracts/tts-streaming-protocol.md`
- **Data Model**: `specs/005-nextjs-realtime-tts/data-model.md`

### External References

- **Next.js Docs**: https://nextjs.org/docs
- **Web Audio API**: https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API
- **Kokoro TTS**: https://github.com/hexgrad/kokoro (example, verify actual repo)
- **gRPC**: https://grpc.io/docs/languages/node/
- **Tonic (Rust gRPC)**: https://docs.rs/tonic/

### Community

- **Issues**: GitHub Issues for bug reports
- **Discussions**: GitHub Discussions for questions
- **Contributing**: See CONTRIBUTING.md

## Environment Variables Reference

### Backend (.env)

| Variable | Default | Description |
|----------|---------|-------------|
| `GRPC_BIND_ADDRESS` | `127.0.0.1:50051` | gRPC server bind address |
| `GRPC_REQUIRE_AUTH` | `false` | Enable authentication |
| `GRPC_AUTH_TOKENS` | `""` | Comma-separated auth tokens |
| `GRPC_MAX_MEMORY_MB` | `500` | Max memory per request (MB) |
| `GRPC_MAX_TIMEOUT_SEC` | `300` | Max request timeout (seconds) |
| `RUST_LOG` | `info` | Log level (error, warn, info, debug, trace) |
| `PYTHON_NODE_PATH` | `../examples/audio_examples` | Path to Python nodes |

### Frontend (.env.local)

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXT_PUBLIC_GRPC_HOST` | `localhost` | gRPC server hostname |
| `NEXT_PUBLIC_GRPC_PORT` | `50051` | gRPC server port |
| `NEXT_PUBLIC_GRPC_SSL` | `false` | Enable TLS/SSL |
| `NEXT_PUBLIC_ENABLE_VOICE_SELECTION` | `true` | Show voice selector UI |
| `NEXT_PUBLIC_ENABLE_SPEED_CONTROL` | `true` | Show speed control UI |
| `NEXT_PUBLIC_MAX_TEXT_LENGTH` | `10000` | Max characters in text input |
| `NEXT_PUBLIC_DEBUG_MODE` | `false` | Enable debug logging |

## FAQ

**Q: Can I use this without Rust?**
A: No, the gRPC service is written in Rust for performance. However, the frontend can theoretically connect to any gRPC-compatible TTS service.

**Q: Does this work offline?**
A: The Kokoro TTS model runs locally, so it works offline once dependencies are installed. However, the frontend needs to connect to the local gRPC service.

**Q: Can I deploy this to Vercel/Netlify?**
A: The frontend (Next.js) can be deployed to Vercel. The backend (Rust + Python) needs a VPS/container (e.g., AWS, DigitalOcean, Docker).

**Q: How do I add a new voice?**
A: Kokoro TTS has built-in voices. Check the Kokoro documentation for available voice IDs and add them to the voice selector in the frontend.

**Q: Can I use a different TTS engine?**
A: Yes! Create a new Python node implementing the Node interface, register it with the runtime, and update the frontend manifest.

**Q: What's the audio quality?**
A: Kokoro TTS produces 24kHz mono audio, comparable to commercial TTS services. Quality depends on the model and voice selected.
