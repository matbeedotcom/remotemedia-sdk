# Example Applications Design

**Date**: 2025-01-27
**Status**: Approved
**Author**: Claude + User

## Overview

This document describes the design for example applications demonstrating the RemoteMedia SDK's `runtime-core` and `transports` capabilities. The examples showcase different use cases with appropriate transports, all with optional `RemotePipelineNode` support for remote offloading.

## Goals

1. **Full-stack integrations** - Complete applications for different tasks using appropriate transports
2. **Hybrid approach** - Standalone examples + unified dashboard showcase
3. **Developer template** - Reusable starter kit for custom projects
4. **Desktop support** - Native applications (Tauri, Electron, PyQt)

## Deliverables

| Phase | Deliverable | Framework | Transport | Key Feature |
|-------|-------------|-----------|-----------|-------------|
| 1 | Rust CLI | Rust (clap) | runtime-core direct | `run`, `stream`, `serve`, `remote` |
| 2 | Voice Assistant | Tauri | gRPC + optional RemotePipelineNode | Local/Hybrid/Remote modes |
| 3 | Inference API | Python FastAPI + Vite | HTTP + FFI | REST API + dashboard |
| 4 | Video Transcription | PyQt6 (compiled) | FFI | SRT/VTT export, burn-in |
| 5 | Voice Chat | Electron | WebRTC | Multi-peer mesh, live transcript |
| 6 | Dashboard | Next.js | All | Unified showcase with live demos |
| 7 | Template | npx CLI | Configurable | Scaffold new projects |

## Key Design Principles

1. **RemotePipelineNode is always optional** - Every app works fully local
2. **Transport matches use case** - gRPC for streaming, HTTP for APIs, WebRTC for P2P
3. **Standalone + Integrated** - Each example works alone AND appears in dashboard
4. **Developer-friendly** - Template scaffolding, code snippets, clear extension points

---

## Phase 1: Rust CLI (`remotemedia-cli`)

### Commands

```bash
# Local execution
remotemedia run <manifest> --input <file> --output <file>
remotemedia stream <manifest> --mic --speaker

# Remote execution (connect to pipeline server)
remotemedia remote run <manifest> --server grpc://host:50051 --input <file>
remotemedia remote stream --server grpc://host:50051 --mic --speaker
remotemedia remote stream --server webrtc://host:8080 --mic --speaker

# Serve locally (expose pipeline as remote endpoint)
remotemedia serve <manifest> --port 8080 --transport grpc
remotemedia serve <manifest> --port 8080 --transport http
remotemedia serve <manifest> --port 8080 --transport webrtc

# Node with RemotePipelineNode (hybrid local + remote)
remotemedia run hybrid-pipeline.yaml  # Pipeline references remote nodes

# Utility commands
remotemedia validate <manifest>
remotemedia nodes list [--server <url>]    # List nodes (local or remote)
remotemedia servers list                    # List known remote servers
remotemedia servers add <name> <url>        # Save server config
```

### Example: Hybrid Pipeline with RemotePipelineNode

```yaml
# hybrid-pipeline.yaml
nodes:
  - id: local_vad
    node_type: SileroVADNode          # Runs locally (fast)

  - id: remote_asr
    node_type: RemotePipelineNode      # Delegates to remote server
    params:
      server_url: "grpc://gpu-server:50051"
      pipeline: "whisper-large"

  - id: local_tts
    node_type: KokoroTTSNode          # Runs locally

connections:
  - from: local_vad
    to: remote_asr
  - from: remote_asr
    to: local_tts
```

### Directory Structure

```
examples/cli/remotemedia-cli/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point + clap CLI
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── run.rs           # Local unary
│   │   ├── stream.rs        # Local streaming
│   │   ├── serve.rs         # Start server
│   │   ├── remote/          # Remote commands
│   │   │   ├── mod.rs
│   │   │   ├── run.rs       # Remote unary
│   │   │   └── stream.rs    # Remote streaming
│   │   ├── nodes.rs
│   │   └── servers.rs       # Server config management
│   ├── transports/
│   │   ├── grpc.rs          # gRPC client
│   │   ├── http.rs          # HTTP client
│   │   └── webrtc.rs        # WebRTC client
│   ├── audio/
│   │   ├── mic.rs           # Microphone capture (cpal)
│   │   └── speaker.rs       # Audio playback (cpal)
│   ├── config/
│   │   └── servers.rs       # ~/.remotemedia/servers.toml
│   └── output/
│       ├── json.rs          # JSON output formatter
│       └── progress.rs      # Progress bar (indicatif)
```

### Dependencies

- `clap` - CLI argument parsing
- `cpal` - Cross-platform audio I/O
- `indicatif` - Progress bars
- `remotemedia-runtime-core` - Direct runtime access

---

## Phase 2: Voice Assistant (Tauri Desktop)

### Modes of Operation

| Mode | Description | Use Case |
|------|-------------|----------|
| **Fully Local** | All nodes run on device | Privacy, offline, low-end models |
| **Hybrid** | VAD/TTS local, STT/LLM remote | Best latency + powerful models |
| **Fully Remote** | All processing on server | Thin client, battery savings |

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Voice Assistant - Mode Selection                            │
│                                                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
│  │  Local Mode     │  │  Hybrid Mode    │  │  Remote     │ │
│  │                 │  │                 │  │             │ │
│  │ [Mic]→[VAD]→    │  │ [Mic]→[VAD]→    │  │ [Mic]→      │ │
│  │ [Whisper-tiny]→ │  │ [Remote STT]→   │  │ [Remote     │ │
│  │ [Local LLM]→    │  │ [Remote LLM]→   │  │  Pipeline]→ │ │
│  │ [TTS]→[Spk]     │  │ [TTS]→[Spk]     │  │ [Spk]       │ │
│  └─────────────────┘  └─────────────────┘  └─────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Directory Structure

```
examples/voice-assistant/
├── src-tauri/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs              # Tauri entry point
│   │   ├── audio.rs             # cpal mic/speaker
│   │   ├── pipeline.rs          # Pipeline orchestration
│   │   └── commands.rs          # Tauri IPC commands
│   └── tauri.conf.json
├── src/
│   ├── App.tsx                  # Main UI
│   ├── components/
│   │   ├── VoiceButton.tsx      # Push-to-talk / always-on toggle
│   │   ├── Transcript.tsx       # Conversation history
│   │   └── Settings.tsx         # Server URL, voice selection
│   └── hooks/
│       └── useVoiceAssistant.ts # State management
├── pipelines/
│   ├── local-only.yaml          # Everything runs locally
│   ├── hybrid-remote-stt.yaml   # VAD/TTS local, STT/LLM remote
│   ├── hybrid-remote-llm.yaml   # VAD/STT/TTS local, only LLM remote
│   └── fully-remote.yaml        # All processing on server
└── package.json
```

### Pipeline Examples

**local-only.yaml**:
```yaml
version: "v1"
metadata:
  name: "voice-assistant-local"

nodes:
  - id: vad
    node_type: SileroVADNode

  - id: stt
    node_type: WhisperNode
    params:
      model: "tiny"

  - id: llm
    node_type: LocalLLMNode
    params:
      model: "phi-3-mini"

  - id: tts
    node_type: KokoroTTSNode

connections:
  - from: vad
    to: stt
  - from: stt
    to: llm
  - from: llm
    to: tts
```

**hybrid-remote-stt.yaml**:
```yaml
version: "v1"
metadata:
  name: "voice-assistant-hybrid"

nodes:
  - id: vad
    node_type: SileroVADNode

  - id: remote_stt_llm
    node_type: RemotePipelineNode
    params:
      transport: grpc
      endpoint: "${GPU_SERVER_URL}"
      pipeline_name: "whisper-llm"
      circuit_breaker:
        failure_threshold: 2
        reset_timeout_ms: 10000

  - id: tts
    node_type: KokoroTTSNode

connections:
  - from: vad
    to: remote_stt_llm
  - from: remote_stt_llm
    to: tts
```

### Fallback Behavior

When remote is unavailable (circuit breaker open):
1. Show notification: "Remote server unavailable, switching to local"
2. Auto-switch to local pipeline (if local models available)
3. Queue retry in background

---

## Phase 3: Real-time Inference API (Python + HTTP)

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Inference API Server                                        │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  FastAPI (Python)                                       │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐             │ │
│  │  │ /predict │  │ /stream  │  │ /health  │             │ │
│  │  │ (unary)  │  │ (SSE)    │  │ (status) │             │ │
│  │  └────┬─────┘  └────┬─────┘  └──────────┘             │ │
│  │       └──────┬───────┘                                  │ │
│  │              ▼                                          │ │
│  │  ┌────────────────────────────────────────────────┐    │ │
│  │  │  remotemedia FFI (zero-copy)                   │    │ │
│  │  │  execute_pipeline() / stream_pipeline()        │    │ │
│  │  └────────────────────────────────────────────────┘    │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Directory Structure

```
examples/inference-api/
├── server/
│   ├── pyproject.toml
│   ├── src/
│   │   └── inference_api/
│   │       ├── __init__.py
│   │       ├── main.py           # FastAPI app
│   │       ├── routes/
│   │       │   ├── predict.py    # POST /predict
│   │       │   ├── stream.py     # POST /stream (SSE)
│   │       │   └── pipelines.py  # GET /pipelines
│   │       ├── services/
│   │       │   ├── pipeline.py   # Pipeline execution via FFI
│   │       │   └── registry.py   # Available pipelines
│   │       └── models/
│   │           ├── request.py    # Pydantic models
│   │           └── response.py
│   └── pipelines/
│       ├── whisper-transcribe.yaml
│       ├── tts-kokoro.yaml
│       └── custom/
│
├── dashboard/
│   ├── package.json
│   ├── src/
│   │   ├── App.tsx
│   │   ├── pages/
│   │   │   ├── Playground.tsx
│   │   │   ├── Metrics.tsx
│   │   │   └── Pipelines.tsx
│   │   └── components/
│   │       ├── AudioRecorder.tsx
│   │       └── ResponseViewer.tsx
│   └── vite.config.ts
│
└── docker-compose.yaml
```

### API Endpoints

```python
@app.post("/predict")
async def predict(request: PredictRequest) -> PredictResponse:
    """Unary prediction - single input, single output"""

@app.post("/stream")
async def stream(request: StreamRequest):
    """Streaming prediction - returns SSE stream"""

@app.get("/pipelines")
async def list_pipelines() -> list[PipelineInfo]:
    """List available pipelines"""

@app.get("/pipelines/{name}")
async def get_pipeline(name: str) -> PipelineManifest:
    """Get pipeline manifest by name"""
```

---

## Phase 4: Video Transcription (PyQt Native Desktop)

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Video Transcription App (PyQt6 - compiled with PyInstaller)│
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  UI Layer                                               │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐             │ │
│  │  │ Video    │  │ Timeline │  │ Export   │             │ │
│  │  │ Preview  │  │ + Subs   │  │ Panel    │             │ │
│  │  └──────────┘  └──────────┘  └──────────┘             │ │
│  └────────────────────────────────────────────────────────┘ │
│                              │                               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Processing Pipeline                                    │ │
│  │                                                         │ │
│  │  [Video File] → [Decode] → [Audio Extract] →           │ │
│  │  [VAD Segment] → [STT] → [Align] → [SRT/VTT]          │ │
│  │                    ↓                                    │ │
│  │            Local Whisper                                │ │
│  │            OR RemotePipelineNode                        │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Directory Structure

```
examples/video-transcription/
├── pyproject.toml
├── src/
│   └── video_transcription/
│       ├── __init__.py
│       ├── main.py
│       ├── ui/
│       │   ├── main_window.py
│       │   ├── video_player.py
│       │   ├── timeline.py
│       │   ├── export_dialog.py
│       │   └── settings.py
│       ├── core/
│       │   ├── transcriber.py
│       │   ├── video_decoder.py
│       │   ├── subtitle.py
│       │   └── alignment.py
│       └── pipelines/
│           ├── local_whisper.yaml
│           └── remote_whisper.yaml
├── resources/
│   ├── icons/
│   └── styles/
├── build/
│   ├── build_windows.spec
│   ├── build_macos.spec
│   └── build_linux.spec
└── README.md
```

### Export Formats

- SRT (SubRip)
- VTT (WebVTT)
- JSON (word-level timestamps)
- Burn-in (FFmpeg subtitle overlay)

---

## Phase 5: Multi-participant Voice Chat (Electron + WebRTC)

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Voice Chat App (Electron)                                   │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  WebRTC Transport (multi-peer mesh)                     │ │
│  │                                                         │ │
│  │  Local Peer ←──────────────────────→ Remote Peers      │ │
│  │      │                                    │             │ │
│  │      ▼                                    ▼             │ │
│  │  [Mic] → [VAD] → [Noise Gate] ──────→ Peer Audio Out   │ │
│  │  [Spk] ← [Mix] ← [Per-peer FX] ←──── Peer Audio In    │ │
│  │                                                         │ │
│  └────────────────────────────────────────────────────────┘ │
│                              │                               │
│              Optional: RemotePipelineNode                    │
│                              ▼                               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Server-side Processing (e.g., live transcription)      │ │
│  │  [Audio Mix] → [RemotePipelineNode] → [Transcript]     │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Directory Structure

```
examples/voice-chat/
├── package.json
├── electron/
│   ├── main.ts
│   ├── preload.ts
│   └── native/
│       └── audio.ts
├── src/
│   ├── App.tsx
│   ├── components/
│   │   ├── Room.tsx
│   │   ├── PeerTile.tsx
│   │   ├── Controls.tsx
│   │   ├── AudioMixer.tsx
│   │   └── Transcript.tsx
│   ├── hooks/
│   │   ├── useWebRTC.ts
│   │   ├── usePeers.ts
│   │   └── useAudioPipeline.ts
│   ├── lib/
│   │   ├── signaling.ts
│   │   ├── pipeline.ts
│   │   └── audio-worklet.ts
│   └── pipelines/
│       ├── local-audio.yaml
│       └── with-transcription.yaml
├── pipelines/
│   └── server-transcription.yaml
└── electron-builder.json
```

---

## Phase 6: Dashboard Integration (nextjs-tts-app)

### New Structure

```
examples/nextjs-tts-app/
├── src/app/
│   ├── page.tsx                    # Dashboard home
│   ├── layout.tsx                  # Updated with new nav
│   │
│   ├── demos/
│   │   ├── page.tsx               # Demo index
│   │   ├── tts/page.tsx           # Existing TTS (moved)
│   │   ├── s2s/page.tsx           # Existing S2S (moved)
│   │   ├── voice-assistant/page.tsx
│   │   ├── inference/page.tsx
│   │   ├── transcription/page.tsx
│   │   └── voice-chat/page.tsx
│   │
│   ├── docs/
│   │   ├── page.tsx
│   │   ├── getting-started/
│   │   ├── transports/
│   │   └── pipelines/
│   │
│   └── api/
│       ├── tts/
│       ├── s2s/
│       └── demos/
│
├── src/components/
│   ├── dashboard/
│   │   ├── DemoCard.tsx
│   │   ├── DemoGrid.tsx
│   │   ├── CodeSnippet.tsx
│   │   └── LivePreview.tsx
│   ├── nav/
│   │   ├── Sidebar.tsx
│   │   └── Header.tsx
│   └── shared/
```

### Dashboard Features

- Demo cards with preview + "Try it" button
- Live interactive demos with code snippets
- Links to standalone desktop apps
- Quick start commands

---

## Phase 7: Template / Starter Kit

### CLI Tool

```bash
# Interactive mode
npx create-remotemedia

# With options
npx create-remotemedia my-app \
  --transport grpc \
  --frontend nextjs \
  --features tts,remote-pipeline
```

### Options

**Transports**:
- gRPC (streaming, production-ready)
- HTTP (simple REST API)
- WebRTC (peer-to-peer)
- FFI (Python integration)

**Frontends**:
- Next.js (React, full-stack)
- Vite + React (lightweight SPA)
- Tauri (desktop app)
- Electron (desktop + WebRTC)
- Python CLI (no frontend)
- None (backend only)

**Features**:
- TTS (Text-to-Speech)
- STT (Speech-to-Text)
- Voice Assistant
- RemotePipelineNode support
- Video processing

### Directory Structure

```
examples/template/
├── create-remotemedia/
│   ├── package.json
│   ├── bin/
│   │   └── create-remotemedia.js
│   ├── src/
│   │   ├── index.ts
│   │   ├── prompts.ts
│   │   ├── generator.ts
│   │   └── templates.ts
│   └── templates/
│       ├── base/
│       ├── transports/
│       │   ├── grpc/
│       │   ├── http/
│       │   ├── webrtc/
│       │   └── ffi/
│       └── frontends/
│           ├── nextjs/
│           ├── vite-react/
│           ├── tauri/
│           ├── electron/
│           └── python-cli/
│
└── templates/
    ├── nextjs-grpc/
    ├── tauri-grpc/
    ├── python-ffi/
    └── electron-webrtc/
```

---

## File Structure Overview

```
examples/
├── cli/                      # Phase 1: Rust CLI
│   └── remotemedia-cli/
├── voice-assistant/          # Phase 2: Tauri desktop
├── inference-api/            # Phase 3: Python + HTTP
│   ├── server/
│   └── dashboard/
├── video-transcription/      # Phase 4: PyQt native
├── voice-chat/               # Phase 5: Electron + WebRTC
├── nextjs-tts-app/           # Phase 6: Dashboard (existing, enhanced)
└── template/                 # Phase 7: Starter kit
    ├── create-remotemedia/
    └── templates/
```

---

## Implementation Priority

1. **Phase 1: Rust CLI** - Foundation for all other examples
2. **Phase 2: Voice Assistant** - Extends existing TTS, shows Rust transport
3. **Phase 3: Inference API** - High demand use case, simpler pipeline
4. **Phase 4: Video Transcription** - Shows video support, compiled desktop
5. **Phase 5: Voice Chat** - Most complex - WebRTC multi-peer
6. **Phase 6-7: Dashboard + Template** - Ties everything together
