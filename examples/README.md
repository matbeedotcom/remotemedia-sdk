# RemoteMedia SDK Examples

This directory contains example applications demonstrating RemoteMedia SDK capabilities.

## Quick Start

| Example | Description | Quick Start |
|---------|-------------|-------------|
| [CLI](cli/remotemedia-cli/) | Command-line pipeline execution | `cargo install --path cli/remotemedia-cli` |
| [Voice Assistant](voice-assistant/) | Tauri desktop voice assistant | `cd voice-assistant && npm run tauri dev` |
| [Inference API](inference-api/) | REST API for pipeline execution | `cd inference-api/server && uvicorn main:app` |
| [Video Transcription](video-transcription/) | PyQt desktop transcription app | `cd video-transcription && python -m video_transcription` |
| [Voice Chat](voice-chat/) | Electron multi-peer voice chat | `cd voice-chat && npm run dev` |
| [Dashboard](nextjs-tts-app/) | Next.js demo dashboard | `cd nextjs-tts-app && npm run dev` |
| [Template](template/) | Project scaffolding tool | `npx create-remotemedia my-app` |

## Shared Resources

- **[shared-pipelines/](shared-pipelines/)** - Reusable pipeline manifests
- **[samples/](samples/)** - Sample audio/video files for testing

## Architecture

All examples follow these principles:

1. **Local-First**: Every example works offline without network connectivity
2. **Optional Remote**: RemotePipelineNode enables opt-in remote processing
3. **Transport Agnostic**: Same pipelines work across gRPC, HTTP, WebRTC, FFI

## Example Categories

### Desktop Applications

| Example | Framework | Use Case |
|---------|-----------|----------|
| Voice Assistant | Tauri 2.x | Local/hybrid voice assistant |
| Video Transcription | PyQt6 | Video subtitle generation |
| Voice Chat | Electron | Multi-peer voice rooms |

### APIs & Services

| Example | Framework | Use Case |
|---------|-----------|----------|
| CLI | Rust + clap | Pipeline execution from terminal |
| Inference API | FastAPI | REST endpoints for pipelines |
| Dashboard | Next.js | Interactive SDK showcase |

### Developer Tools

| Example | Purpose |
|---------|---------|
| Template | Scaffold new RemoteMedia projects |

## Development

### Prerequisites

- **Rust**: For CLI and Tauri backend
- **Node.js 18+**: For web frontends and Electron
- **Python 3.9+**: For inference API and video transcription

### Running Tests

```bash
# CLI tests
cd cli/remotemedia-cli && cargo test

# Python tests
cd inference-api/server && pytest
cd video-transcription && pytest

# TypeScript tests
cd voice-chat && npm test
cd nextjs-tts-app && npm test
```

## Related Documentation

- [SDK Overview](../docs/README.md)
- [Pipeline Manifest Reference](../docs/PIPELINES.md)
- [Transport Documentation](../transports/README.md)
