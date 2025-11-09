# Data Model: Real-Time TTS Web Application

**Feature**: 005-nextjs-realtime-tts
**Date**: 2025-10-29
**Phase**: Phase 1 - Design

## Overview

This document defines the data structures and state management for the TTS web application. Since this is a stateless streaming application, most "data" is ephemeral state managed in the browser.

## Frontend State Model

### 1. TTS Request State

Represents a single text-to-speech synthesis request.

```typescript
interface TTSRequest {
  id: string;                    // Unique request ID (UUID)
  text: string;                  // User input text (up to 10,000 chars)
  voiceConfig: VoiceConfig;      // Voice configuration
  status: TTSStatus;             // Current status
  createdAt: Date;               // Request creation time
  error?: TTSError;              // Error information if failed
}

enum TTSStatus {
  IDLE = 'idle',                 // No active request
  INITIALIZING = 'initializing', // Connecting to gRPC service
  SYNTHESIZING = 'synthesizing', // Audio being generated
  PLAYING = 'playing',           // Audio playing back
  PAUSED = 'paused',             // Playback paused
  COMPLETED = 'completed',       // Synthesis and playback done
  FAILED = 'failed',             // Error occurred
  CANCELLED = 'cancelled'        // User cancelled
}

interface TTSError {
  type: ErrorType;               // Error category
  message: string;               // User-friendly error message
  details?: string;              // Technical details for debugging
}

enum ErrorType {
  NETWORK = 'network',           // Connection/network failure
  SERVER = 'server',             // Server/service unavailable
  VALIDATION = 'validation',     // Invalid input (empty text, etc.)
  SYNTHESIS = 'synthesis',       // TTS generation failed
  PLAYBACK = 'playback'          // Audio playback failed
}
```

### 2. Voice Configuration

User-selected voice parameters.

```typescript
interface VoiceConfig {
  language: LanguageCode;        // Selected language
  voice: VoiceId;                // Selected voice identifier
  speed: number;                 // Speech speed multiplier (0.5 - 2.0)
}

type LanguageCode = 'a' | 'b' | 'e' | 'f' | 'h' | 'i' | 'j' | 'p' | 'z';
// a=American English, b=British English, e=Spanish, f=French,
// h=Hindi, i=Italian, j=Japanese, p=Brazilian Portuguese, z=Mandarin

type VoiceId = string;           // Voice identifier (e.g., 'af_heart', 'af_sky')

// Default configuration
const DEFAULT_VOICE_CONFIG: VoiceConfig = {
  language: 'a',                 // American English
  voice: 'af_heart',             // Default voice
  speed: 1.0                     // Normal speed
};
```

### 3. Audio Stream State

Manages buffered audio chunks and playback state.

```typescript
interface AudioStreamState {
  chunks: AudioChunkInfo[];      // Buffered audio chunks
  playbackState: PlaybackState;  // Current playback state
  currentTime: number;           // Current playback position (seconds)
  duration: number;              // Total duration of buffered audio (seconds)
  bufferHealth: BufferHealth;    // Buffering health status
}

interface AudioChunkInfo {
  sequence: number;              // Chunk sequence number
  audioData: Float32Array;       // PCM audio samples
  sampleRate: number;            // Sample rate (24000)
  duration: number;              // Chunk duration in seconds
  startTime: number;             // Start time in overall audio (seconds)
  played: boolean;               // Whether chunk has been played
}

interface PlaybackState {
  state: PlaybackStatus;         // Current playback status
  volume: number;                // Volume level (0.0 - 1.0)
  muted: boolean;                // Muted state
}

enum PlaybackStatus {
  IDLE = 'idle',                 // Not playing
  PLAYING = 'playing',           // Currently playing
  PAUSED = 'paused',             // Paused
  BUFFERING = 'buffering',       // Waiting for more data
  ENDED = 'ended'                // Playback complete
}

interface BufferHealth {
  status: BufferStatus;          // Buffer health indicator
  bufferedAhead: number;         // Seconds of audio buffered ahead
  targetBuffer: number;          // Target buffer duration (2-3 seconds)
}

enum BufferStatus {
  HEALTHY = 'healthy',           // Sufficient buffer (>2s)
  WARNING = 'warning',           // Low buffer (1-2s)
  CRITICAL = 'critical',         // Very low buffer (<1s)
  STARVED = 'starved'            // No buffer (playback interrupted)
}
```

### 4. Progress State

Tracks synthesis and playback progress for UI display.

```typescript
interface ProgressState {
  synthesisProgress: SynthesisProgress;
  playbackProgress: PlaybackProgress;
}

interface SynthesisProgress {
  chunksReceived: number;        // Number of audio chunks received
  totalChunks?: number;          // Total expected chunks (unknown initially)
  estimatedProgress: number;     // Estimated completion (0.0 - 1.0)
  charactersProcessed?: number;  // Characters synthesized so far
}

interface PlaybackProgress {
  currentTime: number;           // Current playback time (seconds)
  duration: number;              // Total duration (seconds)
  percentage: number;            // Percentage complete (0 - 100)
}
```

## Backend Data (gRPC Protocol)

### 1. Pipeline Manifest

Sent once at the start of each TTS request.

```typescript
interface PipelineManifest {
  version: string;               // Protocol version ('v1')
  metadata: {
    name: string;                // Pipeline name ('tts_pipeline')
    description?: string;        // Optional description
    createdAt?: string;          // ISO 8601 timestamp
  };
  nodes: NodeManifest[];         // Pipeline nodes (1 TTS node)
  connections?: Connection[];    // Node connections (empty for single node)
}

interface NodeManifest {
  id: string;                    // Node ID ('tts_node')
  nodeType: string;              // Node type ('KokoroTTSNode')
  params: string;                // JSON-encoded parameters
  isStreaming: boolean;          // true (streaming node)
}

// Example node params (JSON string):
interface KokoroTTSParams {
  lang_code: string;             // Language code ('a', 'b', etc.)
  voice: string;                 // Voice identifier
  speed: number;                 // Speech speed (0.5 - 2.0)
  split_pattern: string;         // Text split pattern (r'\n+')
  sample_rate: number;           // Output sample rate (24000)
  stream_chunks: boolean;        // Enable streaming (true)
}
```

### 2. Stream Request/Response

gRPC streaming messages (from existing protocol).

```typescript
// From nodejs-client/src/grpc-client.ts and runtime protos

interface StreamRequest {
  pipelineId?: string;           // Pipeline ID (from initialization)
  dataChunk?: DataChunk;         // Audio/data input chunk
  control?: StreamControl;       // Control message (close, pause, etc.)
}

interface DataChunk {
  nodeId: string;                // Target node ID ('tts_node')
  buffer?: DataBuffer;           // Data buffer (text input)
  namedBuffers?: Record<string, DataBuffer>; // Multi-input buffers
  sequence: number;              // Sequence number
  timestampMs: number;           // Timestamp (ms since epoch)
}

interface DataBuffer {
  type: 'text';                  // Buffer type (text for TTS input)
  data: {
    textData: Uint8Array;        // UTF-8 encoded text
    encoding: string;            // 'utf-8'
  };
  metadata?: Record<string, string>; // Optional metadata
}

interface StreamResponse {
  dataChunk?: DataChunk;         // Output data chunk (audio)
  metrics?: StreamMetrics;       // Performance metrics
  error?: ErrorResponse;         // Error information
  ready?: StreamReady;           // Initialization complete
  closed?: StreamClosed;         // Stream closed
}

// Audio output chunk
interface AudioBufferOutput extends DataBuffer {
  type: 'audio';
  data: {
    samples: Uint8Array;         // PCM Float32 samples (little-endian)
    sampleRate: number;          // 24000
    channels: number;            // 1 (mono)
    format: 'F32';               // Float32 format
    numSamples: number;          // Number of samples
  };
}
```

## State Transitions

### TTS Request State Machine

```
IDLE
  → [User clicks "Speak"] → INITIALIZING

INITIALIZING
  → [gRPC connection established] → SYNTHESIZING
  → [Connection failed] → FAILED

SYNTHESIZING
  → [First audio chunk received] → PLAYING
  → [Synthesis error] → FAILED
  → [User cancels] → CANCELLED

PLAYING
  → [User clicks pause] → PAUSED
  → [User clicks stop] → COMPLETED
  → [Playback error] → FAILED
  → [All audio played] → COMPLETED

PAUSED
  → [User clicks resume] → PLAYING
  → [User clicks stop] → COMPLETED

COMPLETED / FAILED / CANCELLED
  → [User clicks "Speak" again] → IDLE
```

### Buffer Health State Machine

```
HEALTHY (>2s buffered)
  → [Buffer drains to 1-2s] → WARNING

WARNING (1-2s buffered)
  → [More chunks arrive] → HEALTHY
  → [Buffer drains to <1s] → CRITICAL

CRITICAL (<1s buffered)
  → [More chunks arrive] → WARNING
  → [Buffer depletes] → STARVED

STARVED (no buffer)
  → [Chunks arrive] → WARNING
  → [No recovery] → FAILED (playback error)
```

## Validation Rules

### Text Input Validation

- **Min Length**: 1 character (non-empty after trim)
- **Max Length**: 10,000 characters
- **Encoding**: Valid UTF-8
- **Rejected**: Empty string, only whitespace

### Voice Config Validation

- **Language**: Must be one of 9 supported codes (a, b, e, f, h, i, j, p, z)
- **Voice**: Must be valid voice ID for selected language
- **Speed**: Must be in range [0.5, 2.0]

### Audio Buffer Validation

- **Sample Rate**: Must be 24000 Hz (Kokoro output)
- **Format**: Must be Float32
- **Channels**: Must be 1 (mono)
- **Sequence**: Must be monotonically increasing (detect dropped chunks)

## React State Management

### Recommended Approach: Zustand Store

```typescript
interface TTSStore {
  // Current request
  request: TTSRequest | null;

  // Voice configuration
  voiceConfig: VoiceConfig;

  // Audio stream
  audioStream: AudioStreamState;

  // Progress
  progress: ProgressState;

  // Actions
  startTTS: (text: string) => Promise<void>;
  pauseTTS: () => void;
  resumeTTS: () => void;
  stopTTS: () => void;
  updateVoiceConfig: (config: Partial<VoiceConfig>) => void;

  // Internal state updates (called by gRPC client)
  _onAudioChunk: (chunk: AudioChunkInfo) => void;
  _onError: (error: TTSError) => void;
  _onComplete: () => void;
}
```

**Rationale**: Zustand provides lightweight state management without React Context complexity. Good for this medium-complexity application.

**Alternatives**:
- **React Context + useReducer**: More verbose, same functionality
- **Redux**: Overkill for this application size
- **Jotai/Recoil**: Atom-based, more complex for this use case

## Persistence

**No persistence required** - this is a stateless application. Each session is ephemeral.

**Future Enhancement**: Could add localStorage for:
- Voice configuration preferences (remember user's last selected voice)
- Text history (recent TTS requests)

## Data Flow

```
User Input → React State
  ↓
Voice Config → Pipeline Manifest → gRPC StreamRequest (init)
  ↓
Text Input → DataChunk (text buffer) → gRPC StreamRequest (data)
  ↓
gRPC StreamResponse (audio chunks) → AudioStreamState
  ↓
AudioChunkInfo[] → Web Audio API → Browser Speakers
  ↓
Progress Updates → React UI
```

## Security Considerations

### Input Sanitization

- **XSS Protection**: Text is never rendered as HTML (React escaping)
- **Injection**: Text goes directly to gRPC (no SQL/command injection risk)
- **Size Limits**: Enforced 10,000 char limit prevents DoS

### Data Privacy

- **No Server Storage**: Text is not persisted on server
- **No Logging**: Text should not be logged (PII risk)
- **Transport Security**: gRPC should use TLS in production

## Performance Optimization

### Memory Management

- **Chunk Cleanup**: Remove played chunks from buffer (keep last 5s for seek)
- **Buffer Limit**: Max 100 chunks in memory (prevents memory bloat)
- **Audio Context**: Reuse single AudioContext (one per session)

### Network Optimization

- **Backpressure**: Honor gRPC flow control (don't overflow server)
- **Compression**: Use gRPC message compression (if available)
- **Connection Reuse**: Keep gRPC connection alive for multiple requests
