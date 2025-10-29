# TTS Streaming Protocol Contract

**Feature**: 005-nextjs-realtime-tts
**Date**: 2025-10-29
**Protocol**: gRPC Bidirectional Streaming

## Overview

This document defines the API contract for real-time text-to-speech streaming using the existing RemoteMedia gRPC protocol. The frontend (NextJS) acts as a gRPC client, and the backend (Rust gRPC service) executes the Python KokoroTTSNode.

## Protocol

**Base Protocol**: RemoteMedia gRPC Streaming Protocol v1
**Proto Files**: `runtime/protos/streaming.proto`, `runtime/protos/common.proto`
**Service**: `StreamingPipelineService`
**RPC Method**: `StreamData` (bidirectional streaming)

## Message Flow

### 1. Initialization Phase

**Client → Server: StreamRequest (Init)**

```protobuf
message StreamRequest {
  oneof request {
    StreamInit init = 1;
  }
}

message StreamInit {
  PipelineManifest manifest = 1;
  VersionInfo client_version = 2;
  map<string, string> metadata = 3;
}

message PipelineManifest {
  string version = 1;               // "v1"
  map<string, string> metadata = 2; // {"name": "tts_pipeline", "createdAt": "..."}
  repeated NodeManifest nodes = 3;
  repeated Connection connections = 4;
}

message NodeManifest {
  string id = 1;                    // "tts_node"
  string node_type = 2;             // "KokoroTTSNode"
  string params = 3;                // JSON string (see below)
  bool is_streaming = 4;            // true
  map<string, string> capabilities = 5;
  string host = 6;
  int32 runtime_hint = 7;
}
```

**Node Params (JSON)**:
```json
{
  "lang_code": "a",
  "voice": "af_heart",
  "speed": 1.0,
  "split_pattern": "\\n+",
  "sample_rate": 24000,
  "stream_chunks": true
}
```

**Server → Client: StreamResponse (Ready)**

```protobuf
message StreamResponse {
  oneof response {
    StreamReady ready = 1;
  }
}

message StreamReady {
  string session_id = 1;            // Unique session identifier
  string pipeline_id = 2;           // Pipeline instance ID
  VersionInfo server_version = 3;
  map<string, string> metadata = 4;
}
```

**Initialization Success Criteria**:
- Client receives `StreamReady` within 500ms
- `session_id` and `pipeline_id` are valid UUIDs
- Server version is compatible (v1.x)

### 2. Text Input Phase

**Client → Server: StreamRequest (Data)**

```protobuf
message StreamRequest {
  oneof request {
    DataChunk data_chunk = 2;
  }
}

message DataChunk {
  string node_id = 1;               // "tts_node"
  DataBuffer buffer = 2;            // Text input
  map<string, DataBuffer> named_buffers = 3;
  uint64 sequence = 4;              // Sequence number (0)
  uint64 timestamp_ms = 5;          // Client timestamp
}

message DataBuffer {
  oneof buffer {
    TextBuffer text = 6;
  }
  map<string, string> metadata = 10;
}

message TextBuffer {
  bytes text_data = 1;              // UTF-8 encoded text
  string encoding = 2;              // "utf-8"
  string language = 3;              // Optional language hint
}
```

**Example Text Input**:
```typescript
const textInput: DataChunk = {
  nodeId: 'tts_node',
  buffer: {
    text: {
      textData: new TextEncoder().encode("Hello, world! This is a test."),
      encoding: 'utf-8',
      language: 'en'
    }
  },
  sequence: 0,
  timestampMs: Date.now()
};
```

### 3. Audio Streaming Phase

**Server → Client: StreamResponse (Audio Chunks)**

```protobuf
message StreamResponse {
  oneof response {
    DataChunk data_chunk = 2;
  }
}

message DataChunk {
  string node_id = 1;               // "tts_node"
  DataBuffer buffer = 2;            // Audio output
  uint64 sequence = 3;              // Chunk sequence (0, 1, 2, ...)
  uint64 timestamp_ms = 4;          // Server timestamp
}

message DataBuffer {
  oneof buffer {
    AudioBuffer audio = 1;
  }
  map<string, string> metadata = 10;
}

message AudioBuffer {
  bytes samples = 1;                // PCM Float32 little-endian
  uint32 sample_rate = 2;           // 24000
  uint32 channels = 3;              // 1 (mono)
  AudioFormat format = 4;           // AUDIO_FORMAT_F32
  uint32 num_samples = 5;           // Number of samples
}

enum AudioFormat {
  AUDIO_FORMAT_UNSPECIFIED = 0;
  AUDIO_FORMAT_F32 = 1;
  AUDIO_FORMAT_I16 = 2;
  AUDIO_FORMAT_I32 = 3;
}
```

**Audio Chunk Characteristics**:
- Sample Rate: 24,000 Hz (fixed)
- Format: Float32 PCM, little-endian
- Channels: 1 (mono)
- Chunk Size: Variable (typically 2400-4800 samples = 100-200ms)
- Sequence: Monotonically increasing (0, 1, 2, ...)

**Decoding Audio Bytes**:
```typescript
// Convert bytes to Float32Array
const audioBuffer: AudioBuffer = response.dataChunk.buffer.audio;
const float32Samples = new Float32Array(
  audioBuffer.samples.buffer,
  audioBuffer.samples.byteOffset,
  audioBuffer.numSamples
);

// Create Web Audio API buffer
const webAudioBuffer = audioContext.createBuffer(
  1,                          // mono
  float32Samples.length,
  24000                       // sample rate
);
webAudioBuffer.copyToChannel(float32Samples, 0);
```

### 4. Metrics Phase (Optional)

**Server → Client: StreamResponse (Metrics)**

```protobuf
message StreamResponse {
  oneof response {
    StreamMetrics metrics = 3;
  }
}

message StreamMetrics {
  uint64 chunks_processed = 1;      // Total chunks processed
  uint64 total_items_processed = 2; // Total samples synthesized
  double processing_time_ms = 3;    // Cumulative processing time
  map<string, string> metadata = 4; // Additional metrics
}
```

**Metrics Frequency**: Every 10 audio chunks (configurable)

### 5. Completion Phase

**Client → Server: StreamRequest (Close)**

```protobuf
message StreamRequest {
  oneof request {
    StreamControl control = 3;
  }
}

message StreamControl {
  Command command = 1;
  map<string, string> metadata = 2;
}

enum Command {
  STREAM_COMMAND_UNSPECIFIED = 0;
  STREAM_COMMAND_CLOSE = 1;
  STREAM_COMMAND_PAUSE = 2;
  STREAM_COMMAND_RESUME = 3;
}
```

**Server → Client: StreamResponse (Closed)**

```protobuf
message StreamResponse {
  oneof response {
    StreamClosed closed = 4;
  }
}

message StreamClosed {
  string session_id = 1;
  uint64 total_chunks = 2;          // Total chunks sent
  double total_time_ms = 3;         // Total session time
  map<string, string> metadata = 4;
}
```

### 6. Error Handling

**Server → Client: StreamResponse (Error)**

```protobuf
message StreamResponse {
  oneof response {
    ErrorResponse error = 5;
  }
}

message ErrorResponse {
  ErrorType error_type = 1;
  string message = 2;               // User-friendly message
  string failing_node_id = 3;       // Node that failed
  string context = 4;               // Additional context
  string stack_trace = 5;           // Debug info
}

enum ErrorType {
  ERROR_TYPE_UNSPECIFIED = 0;
  ERROR_TYPE_VALIDATION = 1;        // Invalid input
  ERROR_TYPE_NODE_EXECUTION = 2;    // Node execution failed
  ERROR_TYPE_RESOURCE_LIMIT = 3;    // Resource exhaustion
  ERROR_TYPE_AUTHENTICATION = 4;    // Auth failure
  ERROR_TYPE_VERSION_MISMATCH = 5;  // Protocol version mismatch
  ERROR_TYPE_INTERNAL = 6;          // Internal server error
}
```

**Error Scenarios**:

| Error Type | Cause | Client Action |
|------------|-------|---------------|
| VALIDATION | Empty text, invalid params | Show validation error, allow retry |
| NODE_EXECUTION | Kokoro TTS crash | Show synthesis error, suggest retry |
| RESOURCE_LIMIT | Memory/timeout exceeded | Show server busy error |
| AUTHENTICATION | Invalid auth token | Show auth error, require re-auth |
| INTERNAL | Server bug/crash | Show generic error, log details |

## Sequence Diagrams

### Happy Path: Basic TTS

```
Client                          Server (gRPC)                   KokoroTTSNode (Python)
  |                                 |                                   |
  |--StreamRequest(Init)----------->|                                   |
  |  manifest: tts_pipeline         |                                   |
  |                                 |---Init Node-------------------->  |
  |<--StreamResponse(Ready)---------|                                   |
  |  session_id: uuid               |                                   |
  |                                 |                                   |
  |--StreamRequest(Data)----------->|                                   |
  |  text: "Hello world"            |---Process Text----------------->  |
  |                                 |                                   |
  |                                 |                                   |--Synthesize
  |<--StreamResponse(Audio #0)------|<--Audio Chunk #0------------------|
  |  samples: [...], seq: 0         |                                   |
  |--Play Audio Chunk #0            |                                   |
  |                                 |                                   |
  |<--StreamResponse(Audio #1)------|<--Audio Chunk #1------------------|
  |  samples: [...], seq: 1         |                                   |
  |--Play Audio Chunk #1            |                                   |
  |                                 |                                   |
  |<--StreamResponse(Audio #2)------|<--Audio Chunk #2------------------|
  |  samples: [...], seq: 2         |                                   |--Complete
  |--Play Audio Chunk #2            |                                   |
  |                                 |                                   |
  |--StreamRequest(Close)----------->|                                   |
  |<--StreamResponse(Closed)--------|                                   |
  |  total_chunks: 3                |                                   |
```

### Error Path: Synthesis Failure

```
Client                          Server (gRPC)                   KokoroTTSNode (Python)
  |                                 |                                   |
  |--StreamRequest(Init)----------->|                                   |
  |<--StreamResponse(Ready)---------|                                   |
  |                                 |                                   |
  |--StreamRequest(Data)----------->|                                   |
  |  text: "..."                    |---Process Text----------------->  |
  |                                 |                                   |
  |                                 |                                   |--Crash/Error
  |                                 |<--Error---------------------------|
  |<--StreamResponse(Error)---------|                                   |
  |  error_type: NODE_EXECUTION     |                                   |
  |  message: "TTS synthesis failed"|                                   |
  |--Show Error to User             |                                   |
```

## Latency Requirements

| Phase | Target Latency | Measured By |
|-------|----------------|-------------|
| Init → Ready | <500ms | Client timestamp → Ready received |
| Data → First Audio Chunk | <2000ms | Data sent → First audio received |
| Audio Chunk Interval | ~100ms | Time between consecutive chunks |
| Control → Response | <100ms | Control sent → Response received |

## Throughput Requirements

| Metric | Target | Notes |
|--------|--------|-------|
| Audio Chunk Rate | ~10 chunks/sec | 100ms per chunk |
| Audio Data Rate | ~96 KB/s | 24kHz × 4 bytes/sample × 1 channel |
| Text Processing | 1000 chars/s | Kokoro synthesis speed |
| Concurrent Sessions | 10-50 | Per server instance |

## Client Implementation Pseudocode

```typescript
import { RemoteMediaGrpcClient } from 'nodejs-client';

class TTSStreamingClient {
  private client: RemoteMediaGrpcClient;
  private stream: AsyncIterableIterator<StreamResponse>;

  async startTTS(text: string, voiceConfig: VoiceConfig): Promise<void> {
    // 1. Create manifest
    const manifest = {
      version: 'v1',
      metadata: { name: 'tts_pipeline' },
      nodes: [{
        id: 'tts_node',
        nodeType: 'KokoroTTSNode',
        params: JSON.stringify({
          lang_code: voiceConfig.language,
          voice: voiceConfig.voice,
          speed: voiceConfig.speed,
          stream_chunks: true
        }),
        isStreaming: true
      }],
      connections: []
    };

    // 2. Initialize stream
    this.stream = await this.client.streamPipeline(manifest);

    // 3. Send text input
    await this.stream.sendData({
      nodeId: 'tts_node',
      buffer: {
        text: {
          textData: new TextEncoder().encode(text),
          encoding: 'utf-8'
        }
      },
      sequence: 0,
      timestampMs: Date.now()
    });

    // 4. Receive audio chunks
    for await (const response of this.stream) {
      if (response.dataChunk?.buffer?.audio) {
        this.handleAudioChunk(response.dataChunk.buffer.audio);
      } else if (response.error) {
        this.handleError(response.error);
        break;
      } else if (response.closed) {
        this.handleComplete(response.closed);
        break;
      }
    }
  }

  async stop(): Promise<void> {
    await this.stream.sendControl({ command: 'STREAM_COMMAND_CLOSE' });
  }
}
```

## Testing Contract

### Unit Tests

1. **Manifest Generation**: Verify correct manifest structure
2. **Text Encoding**: Verify UTF-8 encoding/decoding
3. **Audio Decoding**: Verify Float32 byte array conversion
4. **Sequence Tracking**: Verify sequence number monotonicity
5. **Error Parsing**: Verify error message extraction

### Integration Tests

1. **Happy Path**: Send text, receive audio, verify playback
2. **Empty Text**: Send empty text, expect VALIDATION error
3. **Long Text**: Send 2000-word text, verify streaming
4. **Network Drop**: Simulate connection loss, verify recovery
5. **Concurrent Sessions**: Start 10 sessions, verify isolation

### Contract Tests

1. **Message Format**: Verify all protobuf messages match schema
2. **Enum Values**: Verify enum values match proto definitions
3. **Required Fields**: Verify all required fields are present
4. **Field Types**: Verify field types match expectations

## Backward Compatibility

This feature uses the existing RemoteMedia gRPC protocol (v1) without modifications. It is **fully backward compatible** with existing clients and servers.

**Version Negotiation**:
- Client sends `VersionInfo` in `StreamInit`
- Server validates protocol version (must be v1.x)
- If incompatible, server returns VERSION_MISMATCH error

**Future Extensions**:
- Additional node types can be added without breaking protocol
- New message fields use protobuf evolution (new optional fields)
- Major changes require v2 protocol with negotiation

## Security Considerations

### Authentication

**Recommended**: Use gRPC metadata for API tokens
```typescript
const metadata = new grpc.Metadata();
metadata.set('authorization', `Bearer ${apiToken}`);
await client.streamPipeline(manifest, { metadata });
```

**Server Validation**:
- Check `authorization` metadata in request
- Reject with AUTHENTICATION error if invalid

### Rate Limiting

**Recommended Limits**:
- Max 10 concurrent streams per IP
- Max 1 request per second per user
- Max 10,000 characters per request

### Input Sanitization

**Server-Side**:
- Validate text encoding (reject non-UTF-8)
- Enforce character limit (10,000)
- Filter/escape dangerous characters (if logging)

## References

- **Protocol Buffers**: `runtime/protos/streaming.proto`, `runtime/protos/common.proto`
- **Rust Implementation**: `runtime/src/grpc_service/streaming.rs`
- **TypeScript Client**: `nodejs-client/src/grpc-client.ts`
- **Data Model**: `specs/005-nextjs-realtime-tts/data-model.md`
