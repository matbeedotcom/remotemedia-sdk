# Research & Technology Decisions: Real-Time TTS Web Application

**Feature**: 005-nextjs-realtime-tts
**Date**: 2025-10-29
**Phase**: Phase 0 - Research & Planning

## Overview

This document captures the research and technology decisions for building a real-time text-to-speech web application using Next.js and the existing RemoteMedia gRPC infrastructure.

## Key Research Questions

### Q1: How to stream audio from Rust gRPC service to browser?

**Decision**: Use gRPC-Web with @grpc/grpc-js in Next.js

**Rationale**:
- The existing nodejs-client already uses @grpc/grpc-js for gRPC communication
- Rust Tonic server supports gRPC-Web with tonic-web middleware
- Bidirectional streaming is already implemented in `runtime/src/grpc_service/streaming.rs`
- No need for WebSocket layer - gRPC handles the streaming protocol

**Alternatives Considered**:
1. **WebSocket wrapper**: Would require building a new WebSocket server and protocol
   - Rejected: Unnecessary complexity, duplicates gRPC streaming
2. **Server-Sent Events (SSE)**: Unidirectional only, no backpressure
   - Rejected: Need bidirectional communication for control messages
3. **HTTP polling**: High latency, inefficient
   - Rejected: Doesn't meet <2s latency requirement

**Implementation Notes**:
- Next.js will run the gRPC client in server-side API routes or client-side with gRPC-Web
- Audio chunks received via StreamResponse messages
- Existing DataChunk/AudioBuffer protocol handles sequencing

### Q2: How to integrate Python KokoroTTSNode with Rust runtime?

**Decision**: Extend existing Python node integration pattern from RemoteMedia SDK

**Rationale**:
- The KokoroTTSNode already exists in `examples/audio_examples/kokoro_tts.py`
- RemoteMedia SDK has a pattern for integrating Python nodes (need to verify)
- The Node interface in Python SDK should be compatible with runtime execution
- Kokoro outputs numpy arrays which can be converted to AudioBuffer

**Implementation Approach**:
1. Create a Rust wrapper that spawns Python process running KokoroTTSNode
2. Use IPC (stdin/stdout) or PyO3 FFI to communicate between Rust and Python
3. Register the wrapper as "KokoroTTSNode" in the runtime's NodeRegistry
4. Map text input to the node, stream audio chunks back through gRPC

**Alternatives Considered**:
1. **Rewrite Kokoro in Rust**: Too much work, defeats purpose of reusing existing node
   - Rejected: Not feasible for this feature scope
2. **Run Python as separate microservice**: Adds deployment complexity
   - Rejected: Simpler to integrate directly into runtime process
3. **PyO3 embedded Python**: Tighter integration but more complex
   - Deferred: Start with subprocess, upgrade to PyO3 if needed

### Q3: How to handle audio format conversion (PCM → browser-compatible)?

**Decision**: Stream PCM Float32 directly, convert in browser using Web Audio API

**Rationale**:
- Kokoro outputs 24kHz mono PCM Float32 (standard audio format)
- Web Audio API natively supports Float32 PCM via AudioBuffer
- No transcoding needed on server - reduces latency
- Browser handles all playback, resampling, and format conversion

**Implementation**:
- Kokoro → Float32 numpy array → gRPC AudioBuffer (samples as bytes)
- Browser receives bytes → Float32Array → AudioBuffer → AudioContext
- Web Audio API automatically resamples 24kHz → 48kHz (browser sample rate)

**Alternatives Considered**:
1. **Server-side MP3 encoding**: Adds latency (100-200ms per chunk)
   - Rejected: Violates <2s playback latency requirement
2. **Opus codec**: Good compression but requires encoder/decoder
   - Rejected: Unnecessary complexity for LAN/local deployments
3. **WAV format**: Same as PCM but with headers
   - Rejected: Chunked streaming doesn't need headers

### Q4: How to handle real-time buffering and playback in browser?

**Decision**: Use Web Audio API with manual buffering via AudioContext

**Rationale**:
- Web Audio API provides low-latency audio scheduling
- Can queue audio chunks ahead of playback time
- Supports seamless playback across chunk boundaries
- Precise timing control for pause/resume/seek

**Implementation Pattern**:
```typescript
const audioContext = new AudioContext({ sampleRate: 48000 });
const chunks: AudioBuffer[] = [];

// As chunks arrive from gRPC:
function enqueueChunk(audioData: Float32Array) {
  const buffer = audioContext.createBuffer(1, audioData.length, 24000);
  buffer.copyToChannel(audioData, 0);
  chunks.push(buffer);
  schedulePlayback(buffer);
}

// Schedule playback with gap-free concatenation
function schedulePlayback(buffer: AudioBuffer) {
  const source = audioContext.createBufferSource();
  source.buffer = buffer;
  source.connect(audioContext.destination);
  source.start(nextPlaybackTime);
  nextPlaybackTime += buffer.duration;
}
```

**Alternatives Considered**:
1. **HTML5 <audio> element**: Cannot handle chunked streaming seamlessly
   - Rejected: Gaps between chunks, no precise timing control
2. **MediaSource Extensions (MSE)**: Requires containerized audio (MP4/WebM)
   - Rejected: Adds encoding overhead, more complex
3. **Third-party audio libraries (Howler.js)**: Built on Web Audio anyway
   - Rejected: Adds dependency for no benefit

### Q5: How to manage TTS voice and speed parameters?

**Decision**: Pass parameters via pipeline manifest params field

**Rationale**:
- Existing PipelineManifest has `params` field (JSON string)
- KokoroTTSNode already accepts `lang_code`, `voice`, `speed` in constructor
- Frontend builds manifest with user's selected parameters
- Rust runtime instantiates node with these params

**Implementation**:
```typescript
const manifest = {
  version: 'v1',
  nodes: [{
    id: 'tts_node',
    nodeType: 'KokoroTTSNode',
    params: JSON.stringify({
      lang_code: 'a',  // American English
      voice: 'af_heart',
      speed: 1.0,
      stream_chunks: true
    }),
    isStreaming: true
  }]
};
```

**Alternatives Considered**:
1. **Separate configuration endpoint**: Extra API call before synthesis
   - Rejected: Unnecessary complexity, manifest is sufficient
2. **Environment variables**: Not user-configurable
   - Rejected: Need per-request voice selection

### Q6: How to handle text input encoding and special characters?

**Decision**: UTF-8 encoding throughout, rely on Kokoro's text normalization

**Rationale**:
- Modern web stack (Next.js → gRPC → Python) all support UTF-8 natively
- Kokoro TTS has built-in text normalization and phoneme conversion
- No special preprocessing needed for emojis/special chars - Kokoro filters them
- gRPC protobuf strings are UTF-8 by default

**Implementation**:
- Frontend sends text as-is via UTF-8
- gRPC protobuf encodes as UTF-8
- Python receives as UTF-8 string
- Kokoro handles normalization (removes unsupported characters)

### Q7: How to handle long-form text (2000+ words)?

**Decision**: Send all text at once, rely on Kokoro's chunked streaming

**Rationale**:
- Kokoro internally splits text by sentence/phrase boundaries
- Streams audio chunks as they're generated (no need to wait)
- Single gRPC stream session handles entire synthesis
- Frontend buffering keeps playback smooth

**Implementation Flow**:
1. User clicks "Speak" → Send entire text (up to 10,000 chars) via StreamRequest
2. Kokoro splits text internally (split_pattern=r'\n+')
3. Each split generates audio chunk → streamed via StreamResponse
4. Browser buffers 2-3 seconds ahead, starts playing immediately
5. Playback continues while remaining text is synthesized

**Alternatives Considered**:
1. **Client-side text splitting**: Frontend splits into sentences
   - Rejected: Kokoro already does this better with phoneme awareness
2. **Pagination**: Multiple TTS requests for long text
   - Rejected: Creates gaps between sections, complicates session management

## Technology Stack Summary

### Frontend
| Component | Technology | Justification |
|-----------|------------|---------------|
| Framework | Next.js 14 (App Router) | Modern React patterns, server components, API routes |
| Language | TypeScript 5.x | Type safety, better DX with gRPC types |
| gRPC Client | @grpc/grpc-js | Already used in nodejs-client, proven |
| Audio Playback | Web Audio API | Low-latency, precise timing, browser-native |
| UI Library | TBD (optional: shadcn/ui, TailwindCSS) | Fast prototyping, accessibility |
| Testing | Jest + React Testing Library, Playwright | Unit + E2E coverage |

### Backend (Existing)
| Component | Technology | Justification |
|-----------|------------|---------------|
| gRPC Server | Rust + Tonic | Already exists, high performance |
| Streaming | Bidirectional gRPC streams | Already implemented in streaming.rs |
| TTS Engine | Python + Kokoro TTS | Already exists, high-quality voice |
| Node Integration | TBD: Subprocess or PyO3 | Need to determine existing pattern |

### Communication Protocol
| Aspect | Decision | Notes |
|--------|----------|-------|
| Protocol | gRPC bidirectional streaming | Existing RemoteMedia protocol |
| Message Types | StreamRequest, StreamResponse, AudioChunk | Already defined in protos |
| Audio Format | PCM Float32, 24kHz, mono | Native Kokoro output |
| Serialization | Protocol Buffers | Standard gRPC serialization |

## Performance Considerations

### Latency Budget (2-second requirement)
- Network RTT: ~20ms (local/LAN deployment)
- Kokoro first chunk: ~500-800ms (model inference)
- Audio encoding: 0ms (no transcoding)
- Browser buffering: ~100ms (Web Audio scheduling)
- **Total**: ~620-920ms ✅ (well under 2s target)

### Throughput
- Kokoro synthesis: ~10-15x real-time (1s audio in ~70ms)
- gRPC streaming: ~10Mbps for uncompressed audio (low bandwidth)
- Concurrent sessions: 10-50 supported (Python GIL limits per-server)

### Memory
- Browser audio buffer: ~5MB for 2000-word document
- Python Kokoro model: ~500MB per instance
- Rust runtime overhead: ~50MB per session

## Open Questions for Implementation

1. **Python node integration pattern**: How does RemoteMedia SDK currently integrate Python nodes? Need to review existing examples.

2. **Text input to node**: Does text go as a special input type, or as metadata in AudioChunk? Need to check protocol.

3. **Session management**: How to handle multiple simultaneous TTS requests from different users? Need session isolation.

4. **Error handling**: What happens if Kokoro crashes mid-synthesis? Need graceful degradation.

5. **Authentication**: Should TTS requests require auth tokens? Or is this a demo app without auth?

## Next Steps (Phase 1)

1. Review existing Python node integration pattern in RemoteMedia SDK
2. Design data model for TTS state management (frontend React state)
3. Define API contracts (gRPC message flow for TTS)
4. Create quickstart guide for setting up development environment
5. Prototype Python node wrapper for KokoroTTSNode

## References

- Kokoro TTS: `examples/audio_examples/kokoro_tts.py`
- gRPC Streaming: `runtime/src/grpc_service/streaming.rs`
- TypeScript Client: `nodejs-client/src/grpc-client.ts`
- Protocol Buffers: `runtime/protos/*.proto`
- Web Audio API: https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API
