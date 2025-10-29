# Implementation Plan: Real-Time Text-to-Speech Web Application

**Branch**: `005-nextjs-realtime-tts` | **Date**: 2025-10-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/005-nextjs-realtime-tts/spec.md`

## Summary

Build a NextJS web application that enables users to convert text to speech in real-time using a remote Kokoro TTS engine. Users enter text in the browser, which is sent to a Python backend running the KokoroTTSNode. Audio chunks are streamed back to the browser as they're generated, providing immediate playback without waiting for full synthesis. The application supports voice customization, playback controls, and handles network/error conditions gracefully.

**Technical Approach**:
- Frontend: NextJS (React) single-page application with Web Audio API
- Backend: RemoteMedia Rust gRPC service (already exists at `runtime/src/grpc_service/`)
- TTS Node: Python KokoroTTSNode (already exists at `examples/audio_examples/kokoro_tts.py`)
- Communication: gRPC streaming (using existing TypeScript gRPC client from `nodejs-client/`)
- Audio Format: PCM 24kHz from Kokoro, streamed via gRPC AudioBuffer messages

## Technical Context

**Language/Version**:
- Frontend: TypeScript 5.x with Next.js 14+
- Backend: Python 3.11+ (compatible with Kokoro TTS and RemoteMedia SDK)

**Primary Dependencies**:
- Frontend: Next.js 14+, React 18+, TypeScript 5.x, Web Audio API
- gRPC Client: @grpc/grpc-js, @grpc/proto-loader (from nodejs-client/)
- Backend: Rust gRPC service (already exists), Tonic framework
- TTS Engine: Python 3.11+, Kokoro TTS (>=0.9.4), KokoroTTSNode
- Communication: gRPC bidirectional streaming (existing RemoteMedia protocol)

**Storage**: N/A (stateless application, no persistent storage required)

**Testing**:
- Frontend: Jest + React Testing Library for unit tests, Playwright for E2E
- Backend: pytest for unit/integration tests, pytest-asyncio for async tests

**Target Platform**:
- Frontend: Modern web browsers (Chrome 90+, Firefox 88+, Safari 14+, Edge 90+)
- Backend: Linux/macOS server with Python 3.11+

**Project Type**: Web application (frontend + backend)

**Performance Goals**:
- Audio playback latency: <2 seconds from button click
- Smooth streaming: 95% sessions without buffer gaps
- Concurrent users: Support 10+ simultaneous synthesis sessions
- Control responsiveness: <100ms for pause/resume/stop actions

**Constraints**:
- Audio format: Kokoro TTS outputs 24kHz PCM (mono/stereo as configured)
- Network bandwidth: Minimum 64 kbps per user for real-time audio
- Browser compatibility: Must work without browser plugins
- Real-time requirement: Cannot batch-process; must stream incrementally

**Scale/Scope**:
- Text input: Up to 10,000 characters per request
- Synthesis duration: Up to 5 minutes for 2000-word documents
- Concurrent users: 10-50 simultaneous sessions (MVP target)
- Voice options: 9 languages, multiple voice profiles per language

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Note**: No project-specific constitution file found (`.specify/memory/constitution.md` contains only template). Applying standard best practices:

✅ **Modularity**: Frontend and backend are independently testable
✅ **Testability**: Both layers will have unit + integration tests
✅ **Documentation**: Quickstart and API contracts will be generated
✅ **Simplicity**: Using standard patterns (REST/WebSocket, Web Audio API)
✅ **Dependencies**: Leveraging existing KokoroTTSNode and RemoteMedia SDK

**No violations** - Standard web application architecture with clear separation of concerns.

## Project Structure

### Documentation (this feature)

```text
specs/005-nextjs-realtime-tts/
├── plan.md              # This file
├── research.md          # Phase 0: Technology decisions and patterns
├── data-model.md        # Phase 1: State management and data structures
├── quickstart.md        # Phase 1: Setup and development guide
├── contracts/           # Phase 1: API specifications
│   ├── tts-api.yaml    # OpenAPI spec for REST endpoints
│   └── audio-stream.md # Audio streaming protocol specification
└── tasks.md             # Phase 2: Implementation tasks (created by /speckit.tasks)
```

### Source Code (repository root)

```text
# Next.js application (frontend only - backend is existing Rust gRPC service)

examples/nextjs-tts-app/
├── README.md
├── package.json
├── next.config.js
├── tsconfig.json
├── .env.example
├── .env.local.example
│
├── src/
│   ├── app/                    # Next.js 14 app router
│   │   ├── page.tsx           # Main TTS page
│   │   ├── layout.tsx         # Root layout
│   │   ├── globals.css        # Global styles
│   │   └── api/               # Next.js API routes (optional proxy if needed)
│   │       └── health/
│   │           └── route.ts   # Health check endpoint
│   │
│   ├── components/             # React components
│   │   ├── TextInput.tsx      # Text entry component
│   │   ├── VoiceSelector.tsx  # Voice/language selection
│   │   ├── AudioPlayer.tsx    # Playback controls
│   │   ├── ProgressBar.tsx    # Synthesis progress
│   │   └── ErrorDisplay.tsx   # Error messaging
│   │
│   ├── lib/                    # Business logic & services
│   │   ├── grpc-client.ts     # gRPC client wrapper (uses nodejs-client)
│   │   ├── audio-player.ts    # Web Audio API wrapper
│   │   ├── stream-handler.ts  # Audio streaming & buffering logic
│   │   └── tts-pipeline.ts    # TTS pipeline manifest builder
│   │
│   ├── hooks/                  # React hooks
│   │   ├── useTTS.ts          # TTS synthesis hook
│   │   ├── useAudioPlayer.ts  # Audio playback hook
│   │   └── useStreamBuffer.ts # Buffering hook
│   │
│   └── types/                  # TypeScript definitions
│       ├── tts.ts             # TTS-related types
│       └── audio.ts           # Audio-related types
│
└── tests/
    ├── unit/                  # Component unit tests (Jest)
    └── e2e/                   # Playwright E2E tests

# Existing infrastructure (reused)

runtime/                        # Rust gRPC service (already exists)
├── src/grpc_service/          # gRPC server implementation
│   ├── streaming.rs           # Bidirectional streaming handler
│   └── ...

examples/audio_examples/        # Python TTS nodes (already exist)
├── kokoro_tts.py              # KokoroTTSNode implementation
└── ...

nodejs-client/                  # TypeScript gRPC client (already exists)
├── src/
│   ├── grpc-client.ts         # gRPC client implementation
│   └── data-types.ts          # Type-safe data types
└── ...

# New Python integration (register KokoroTTSNode with runtime)

runtime/src/nodes/              # Node registry
├── mod.rs                     # Register Python nodes
└── python_nodes/              # Python node wrappers
    └── kokoro_tts_wrapper.rs  # Rust wrapper for KokoroTTSNode
```

**Structure Decision**: NextJS application as a standalone frontend that communicates with the existing Rust gRPC service. The Rust service already has bidirectional streaming support and will execute the Python KokoroTTSNode. This approach:
1. Reuses existing gRPC infrastructure (no new backend needed)
2. Leverages existing TypeScript gRPC client from `nodejs-client/`
3. Integrates KokoroTTSNode through the runtime's node registry
4. All new code is the NextJS frontend in `examples/nextjs-tts-app/`

## Complexity Tracking

> **No constitutional violations** - No complexity justification needed.

