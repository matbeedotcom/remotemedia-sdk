# Speech-to-Speech Conversational AI

This document describes the speech-to-speech (S2S) conversational AI system built using the LFM2-Audio-1.5B model from Liquid AI.

## Overview

The system provides natural voice conversations with AI using audio-to-audio processing without intermediate text transcription. It supports two modes:

1. **Single-shot mode**: Record a question, get a response
2. **Continuous streaming mode** (VAD-based): Continuous conversation with automatic speech detection

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────────┐
│                      Frontend (Next.js)                      │
├─────────────────────────────────────────────────────────────┤
│  • Audio recording (microphone)                              │
│  • VAD-based speech detection (optional)                     │
│  • Audio playback                                            │
│  • Conversation UI                                           │
└──────────────────┬──────────────────────────────────────────┘
                   │ HTTP/SSE
┌──────────────────▼──────────────────────────────────────────┐
│               API Routes (Next.js)                           │
├─────────────────────────────────────────────────────────────┤
│  • /api/s2s/stream - Single-shot S2S                        │
│  • /api/s2s/vad-stream - Continuous VAD-based streaming     │
└──────────────────┬──────────────────────────────────────────┘
                   │ gRPC
┌──────────────────▼──────────────────────────────────────────┐
│            Rust gRPC Server (Runtime)                        │
├─────────────────────────────────────────────────────────────┤
│  • StreamPipeline RPC                                        │
│  • Session management                                        │
│  • Node caching (10-min TTL)                                 │
│  • Metrics collection                                        │
└──────────────────┬──────────────────────────────────────────┘
                   │ Python FFI
┌──────────────────▼──────────────────────────────────────────┐
│              Python Nodes                                    │
├─────────────────────────────────────────────────────────────┤
│  • LFM2AudioNode - Conversational AI                        │
│  • VoiceActivityDetector - Speech detection                 │
│  • VADTriggeredBuffer - Segment buffering                   │
└─────────────────────────────────────────────────────────────┘
```

## Files Created

### Python Nodes

1. **`python-client/remotemedia/nodes/ml/lfm2_audio.py`**
   - LFM2AudioNode implementation
   - Features:
     - Audio input → Text + Audio output
     - Session-based conversation history
     - Automatic session cleanup (30-min TTL)
     - Thread-safe PyTorch operations
     - RuntimeData API integration

### Next.js Frontend

2. **`src/app/s2s/page.tsx`**
   - Interactive S2S demo page
   - Features:
     - Record button
     - Conversation display
     - Audio playback
     - Metrics display

3. **`src/app/api/s2s/stream/route.ts`**
   - Single-shot S2S API endpoint
   - Accepts base64 audio, returns NDJSON stream

4. **`src/app/api/s2s/vad-stream/route.ts`**
   - VAD-based continuous streaming endpoint
   - Server-Sent Events (SSE) for responses

5. **`src/lib/s2s-streaming-client.ts`**
   - Client library for single-shot S2S
   - Audio recording and playback utilities

6. **`src/lib/vad-streaming-client.ts`**
   - Client library for VAD-based continuous streaming
   - Real-time speech detection
   - Buffer management

### Python Examples

7. **`examples/audio_examples/lfm2_audio_s2s.py`**
   - Standalone Python example
   - Single-turn and multi-turn conversations
   - Audio file input/output

8. **`examples/audio_examples/vad_lfm2_audio_streaming.py`**
   - VAD-based streaming pipeline example
   - Demonstrates full pipeline:
     - Audio Stream → VAD → Buffer → LFM2-Audio → Response

## Usage

### 1. Install Dependencies

```bash
# Python dependencies
pip install liquid-audio torch torchaudio soundfile librosa

# Next.js dependencies (already installed)
cd examples/nextjs-tts-app
npm install
```

### 2. Start the Rust Runtime

```bash
# From SDK root
cargo run --release --bin remotemedia-server
```

### 3. Run Python Example (Optional)

```bash
# Single-shot example
python examples/audio_examples/lfm2_audio_s2s.py

# VAD-based streaming example
python examples/audio_examples/vad_lfm2_audio_streaming.py
```

### 4. Start Next.js Dev Server

```bash
cd examples/nextjs-tts-app
npm run dev
```

### 5. Access Demo Pages

- **S2S Demo**: http://localhost:3000/s2s
- **TTS Demo**: http://localhost:3000

## API Reference

### Single-Shot S2S API

**Endpoint**: `POST /api/s2s/stream`

**Request**:
```json
{
  "audio": "base64-encoded-pcm-audio",
  "sampleRate": 24000,
  "sessionId": "optional-session-id",
  "systemPrompt": "Optional system prompt",
  "reset": false
}
```

**Response**: Newline-delimited JSON (NDJSON)
```json
{"type": "text", "content": "Hello!", "sequence": 0}
{"type": "audio", "content": "base64-audio", "sampleRate": 24000, "sequence": 1}
{"type": "metrics", "content": {...}, "sequence": 2}
{"type": "complete", "sessionId": "...", "sequence": 3}
```

### VAD Streaming API (In Development)

**Endpoint**: `POST /api/s2s/vad-stream`

Uses Server-Sent Events (SSE) for bidirectional communication.

## Pipeline Configurations

### Simple Pipeline (Single-Shot)

```
Audio Input → LFM2AudioNode → Text + Audio Output
```

### VAD Pipeline (Continuous)

```
Audio Stream → VAD → VAD Buffer → LFM2AudioNode → Text + Audio Output
             ↓
         (metadata)
```

**Node Configuration**:

```javascript
// VAD Node
{
  id: 'vad',
  nodeType: 'VoiceActivityDetector',
  params: {
    frameDurationMs: 30,
    energyThreshold: 0.02,
    speechThreshold: 0.3,
    filterMode: false,
    includeMetadata: true
  }
}

// VAD Buffer Node
{
  id: 'vad_buffer',
  nodeType: 'VADTriggeredBuffer',
  params: {
    minSpeechDurationS: 0.8,  // Min 0.8s speech
    maxSpeechDurationS: 10.0, // Max 10s
    silenceDurationS: 1.0,    // 1s silence to end
    sampleRate: 24000
  }
}

// LFM2-Audio Node
{
  id: 'lfm2_audio',
  nodeType: 'LFM2AudioNode',
  params: {
    systemPrompt: "You are a helpful AI assistant.",
    audioTemperature: 1.0,
    audioTopK: 4,
    maxNewTokens: 512,
    sampleRate: 24000,
    sessionTimeoutMinutes: 30
  }
}
```

## Performance

### Latency

- **Cold start**: ~2-3 seconds (first request, model loading)
- **Cached**: ~500-800ms (subsequent requests)
- **Streaming**: First audio chunk in ~500ms

### Caching

The Rust runtime caches Python nodes globally:
- **Cache TTL**: 10 minutes (configurable)
- **Cache key**: `{node_type}:{params_hash}`
- **Benefit**: 4-6x latency reduction for subsequent requests

### Session Management

- **Conversation history**: Maintained per session
- **Session TTL**: 30 minutes (configurable)
- **Automatic cleanup**: Background task removes expired sessions

## Model Information

**LFM2-Audio-1.5B**
- **Provider**: Liquid AI
- **HuggingFace**: `LiquidAI/LFM2-Audio-1.5B`
- **Parameters**: 1.5 billion
- **Sample Rate**: 24kHz
- **Input**: Audio
- **Output**: Interleaved text and audio tokens
- **Languages**: Multiple (English, etc.)

## Troubleshooting

### Model Loading Issues

```bash
# Ensure liquid-audio is installed
pip install liquid-audio

# Check CUDA availability (optional, for GPU)
python -c "import torch; print(torch.cuda.is_available())"
```

### Audio Issues

- **No audio output**: Check browser audio permissions
- **Low quality**: Ensure 24kHz sample rate
- **Choppy playback**: Check network latency, enable caching

### Session Issues

- **Lost conversation history**: Check session ID consistency
- **Sessions not expiring**: Verify session timeout configuration

## Future Enhancements

1. **WebSocket Support**: Replace SSE with WebSocket for true bidirectional streaming
2. **Real-time VAD**: Use Silero VAD or WebRTC VAD for better accuracy
3. **Audio Preprocessing**: Noise reduction, echo cancellation
4. **Multi-language Support**: UI for language selection
5. **Voice Selection**: Multiple voice options
6. **Conversation Export**: Save conversations as audio/text

## References

- [Liquid AI LFM2-Audio](https://huggingface.co/LiquidAI/LFM2-Audio-1.5B)
- [RemoteMedia SDK Documentation](../../README.md)
- [Next.js TTS Demo](./README.md)

## License

Same as RemoteMedia SDK
