# Pipeline Manifests and Configuration

This document describes where the pipeline manifests are located and how they're used in the speech-to-speech system.

## Pipeline Locations

### 1. Static Pipeline Manifests (JSON Files)

Located in `pipelines/` directory:

#### `pipelines/simple-s2s-pipeline.json`
Single-node pipeline for direct speech-to-speech without VAD.

**Flow**: `Audio Input → LFM2AudioNode → Text + Audio Output`

**Use case**: Single-shot questions/responses

```json
{
  "nodes": [
    {
      "id": "lfm2_audio",
      "nodeType": "LFM2AudioNode",
      "params": {...}
    }
  ],
  "connections": []
}
```

#### `pipelines/vad-s2s-pipeline.json`
Multi-node pipeline with VAD for continuous conversation.

**Flow**: `Audio Stream → AudioTransform → VAD → VADBuffer → LFM2AudioNode → Output`

**Use case**: Continuous conversation with automatic speech detection

```json
{
  "nodes": [
    {"id": "audio_transform", "nodeType": "AudioTransform"},
    {"id": "vad", "nodeType": "VoiceActivityDetector"},
    {"id": "vad_buffer", "nodeType": "VADTriggeredBuffer"},
    {"id": "lfm2_audio", "nodeType": "LFM2AudioNode"}
  ],
  "connections": [
    {"from": "audio_transform", "to": "vad"},
    {"from": "vad", "to": "vad_buffer"},
    {"from": "vad_buffer", "to": "lfm2_audio"}
  ]
}
```

### 2. Dynamic Pipeline Builder

Located in `src/lib/pipeline-builder.ts`:

**Functions**:
- `createSimpleS2SPipeline(options)` - Build simple S2S pipeline programmatically
- `createVADS2SPipeline(options)` - Build VAD-based pipeline with custom parameters
- `createTTSPipeline(options)` - Build TTS pipeline
- `validatePipelineManifest(manifest)` - Validate pipeline structure
- `loadPipelineManifest(filePath)` - Load pipeline from JSON file

**Example usage**:
```typescript
import { createSimpleS2SPipeline } from '@/lib/pipeline-builder';

const manifest = createSimpleS2SPipeline({
  sessionId: 'my-session-123',
  systemPrompt: 'You are a helpful assistant.',
  audioTemperature: 1.0,
  audioTopK: 4,
  maxNewTokens: 512,
});
```

### 3. API Routes Using Pipelines

#### `/api/s2s/stream/route.ts`
Uses `createSimpleS2SPipeline()` to create a single-node LFM2Audio pipeline.

**Pipeline created**: Simple S2S (no VAD)

```typescript
const manifest = createSimpleS2SPipeline({
  sessionId: actualSessionId,
  systemPrompt: systemPrompt || 'Default prompt',
  audioTemperature: 1.0,
  audioTopK: 4,
  maxNewTokens: 512,
});

// Send to gRPC StreamPipeline
for await (const chunk of client.streamPipeline(manifest, audioDataGenerator())) {
  // Process responses...
}
```

#### `/api/s2s/vad-stream/route.ts`
Uses `createVADS2SPipeline()` to create a multi-node VAD pipeline.

**Pipeline created**: VAD-based S2S

```typescript
const manifest = createVADS2SPipeline({
  sessionId: actualSessionId,
  systemPrompt: systemPrompt || 'Default prompt',
  audioTemperature: 1.0,
  audioTopK: 4,
  maxNewTokens: 512,
  vadEnergyThreshold: 0.02,
  minSpeechDuration: 0.8,
  maxSpeechDuration: 10.0,
  silenceDuration: 1.0,
});
```

#### `/api/tts/stream/route.ts`
Creates a simple Kokoro TTS pipeline inline.

**Pipeline created**: Single TTS node

```typescript
const manifest = {
  version: 'v1',
  nodes: [{
    id: 'tts',
    nodeType: 'KokoroTTSNode',
    params: JSON.stringify({text, language, voice, speed}),
  }],
  connections: []
};
```

## Pipeline Execution Flow

### How Pipelines are Executed

1. **Client** (Browser/Python) sends audio data
2. **API Route** creates pipeline manifest using builder or JSON
3. **API Route** calls `client.streamPipeline(manifest, dataGenerator())`
4. **gRPC Client** sends manifest to Rust runtime via `StreamPipeline` RPC
5. **Rust Runtime** (`runtime/src/grpc_service/streaming.rs`):
   - Parses manifest
   - Creates nodes from registry
   - Caches nodes globally (10-min TTL)
   - Connects nodes according to `connections` array
   - Streams data through pipeline
6. **Python Nodes** process data via FFI
7. **Responses** stream back through gRPC to client

### gRPC StreamPipeline RPC

**Proto definition**: `runtime/protos/streaming.proto`

```protobuf
service StreamingPipelineService {
  rpc StreamPipeline(stream StreamRequest) returns (stream StreamResponse);
}

message StreamInit {
  PipelineManifest manifest = 1;
  map<string, DataBuffer> data_inputs = 2;
  // ...
}
```

**Request flow**:
1. `StreamRequest::Init` with manifest
2. Multiple `StreamRequest::DataChunk` with audio
3. `StreamRequest::Control(CLOSE)` to finish

**Response flow**:
1. `StreamResponse::Ready` (session established)
2. Multiple `StreamResponse::ChunkResult` (audio/text outputs)
3. Periodic `StreamResponse::Metrics`
4. `StreamResponse::Closed` (final metrics)

## Node Registry

Nodes are registered in the Rust runtime at `runtime/src/nodes/streaming_registry.rs`.

**Current registered nodes**:
- `KokoroTTSNode` - Text-to-speech
- `LFM2AudioNode` - Speech-to-speech conversation
- `VoiceActivityDetector` - Speech detection
- `VADTriggeredBuffer` - Speech segment buffering
- `AudioTransform` - Resampling/channel conversion

To add new nodes, register them in `create_default_streaming_registry()`.

## Python Node Implementations

Located in `python-client/remotemedia/nodes/`:

- `nodes/tts.py` - `KokoroTTSNode`
- `nodes/ml/lfm2_audio.py` - `LFM2AudioNode`
- `nodes/audio.py` - `VoiceActivityDetector`, `AudioTransform`, `AudioBuffer`

## Examples

### Python Pipeline Example

`examples/audio_examples/vad_lfm2_audio_streaming.py`

Shows how to construct a VAD pipeline programmatically in Python:

```python
# Create nodes
vad_node = VoiceActivityDetector(...)
vad_buffer = VADTriggeredBufferNode(...)
lfm2_audio = LFM2AudioNode(...)

# Connect them
audio_stream = audio_chunk_generator(...)
vad_stream = vad_process_wrapper(vad_node, audio_stream)
buffered_stream = vad_buffer.process(vad_stream)

# Process
async for response in lfm2_audio.process(buffered_stream, session_id):
    # Handle response...
```

### Next.js Usage Example

See `src/app/s2s/page.tsx` for full implementation:

```typescript
// Record audio
const audioData = await recorder.stopRecording();

// Stream to API (which creates pipeline internally)
await streamS2S(audioData, {sessionId, sampleRate: 24000}, {
  onText: (text) => console.log('AI:', text),
  onAudio: (audio) => playAudio(audio, 24000),
});
```

## Pipeline Configuration Options

### Common Parameters

**LFM2AudioNode**:
- `system_prompt` - Conversation context
- `audio_temperature` - Audio generation randomness (0.0-2.0)
- `audio_top_k` - Top-k sampling for audio tokens
- `max_new_tokens` - Maximum tokens to generate
- `sample_rate` - Audio sample rate (24000Hz)
- `session_timeout_minutes` - Session expiration time

**VoiceActivityDetector**:
- `frame_duration_ms` - VAD frame size (10, 20, or 30ms)
- `energy_threshold` - Energy threshold for speech detection
- `speech_threshold` - Ratio of speech frames to trigger
- `filter_mode` - Whether to filter non-speech
- `include_metadata` - Add VAD metadata to output

**VADTriggeredBuffer**:
- `min_speech_duration_s` - Minimum speech to trigger
- `max_speech_duration_s` - Maximum before forced trigger
- `silence_duration_s` - Silence duration to confirm end
- `sample_rate` - Audio sample rate

## Debugging

### View Pipeline Logs

```bash
# Rust runtime logs
RUST_LOG=debug cargo run --release --bin remotemedia-server

# Next.js API logs
npm run dev
# Check console for "[S2S API]" and "[VAD S2S API]" logs
```

### Validate Pipeline Manifest

```typescript
import { validatePipelineManifest } from '@/lib/pipeline-builder';

const validation = validatePipelineManifest(manifest);
if (!validation.valid) {
  console.error('Invalid pipeline:', validation.errors);
}
```

### Check Node Cache

Pipeline metrics include cache information:
- `cacheHits` - Number of times nodes were reused
- `cacheMisses` - Number of times nodes were created
- `cachedNodesCount` - Total cached nodes
- `cacheHitRate` - Efficiency ratio (0.0-1.0)

## Future Enhancements

1. **Pipeline Templates** - Pre-defined pipeline configurations
2. **Visual Pipeline Editor** - Drag-and-drop pipeline builder UI
3. **Pipeline Composition** - Combine pipelines dynamically
4. **Conditional Routing** - Route data based on conditions
5. **Pipeline Monitoring** - Real-time visualization of data flow

## References

- [StreamPipeline Proto](../../runtime/protos/streaming.proto)
- [Streaming Service Implementation](../../runtime/src/grpc_service/streaming.rs)
- [Node Registry](../../runtime/src/nodes/streaming_registry.rs)
- [Python Nodes](../../python-client/remotemedia/nodes/)
