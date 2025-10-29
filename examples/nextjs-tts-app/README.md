# Real-Time Text-to-Speech Web Application

A Next.js web application that provides real-time text-to-speech synthesis using the Kokoro TTS engine through the RemoteMedia gRPC service.

## Features

- ✅ Real-time text-to-speech synthesis
- ✅ Streaming audio playback (starts within 2 seconds)
- ✅ Support for long-form text (up to 10,000 characters)
- ✅ Multiple voice and language options
- ✅ Playback controls (play, pause, stop, seek)
- ✅ Progress indication and buffer management
- ✅ Error handling with user-friendly messages

## Prerequisites

- Node.js 18.17+ or 20+
- pnpm 8+
- Running RemoteMedia gRPC service (localhost:50051)
- Kokoro TTS engine installed and configured

## Quick Start

### 1. Install Dependencies

```bash
pnpm install
```

### 2. Configure Environment

Copy the environment template and adjust if needed:

```bash
cp .env.local.example .env.local
```

Default configuration connects to `localhost:50051` (gRPC service).

### 3. Start Development Server

```bash
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

### 4. Test the Application

1. Type or paste text in the input field
2. Click the "Speak" button
3. Hear the synthesized speech within 2 seconds

## Project Structure

```
src/
├── app/              # Next.js 14 App Router
│   ├── page.tsx     # Main TTS page
│   ├── layout.tsx   # Root layout
│   └── globals.css  # Global styles
├── components/       # React components
│   ├── TextInput.tsx
│   ├── AudioPlayer.tsx
│   ├── VoiceSelector.tsx
│   ├── ProgressBar.tsx
│   └── ErrorDisplay.tsx
├── hooks/           # Custom React hooks
│   ├── useTTS.ts
│   ├── useAudioPlayer.ts
│   └── useStreamBuffer.ts
├── lib/             # Business logic
│   ├── grpc-client.ts
│   ├── audio-player.ts
│   ├── stream-handler.ts
│   └── tts-pipeline.ts
└── types/           # TypeScript definitions
    ├── tts.ts
    └── audio.ts
```

## Architecture

```
Browser (Next.js) → gRPC Client → Rust gRPC Service → Python Kokoro TTS
                                                    ↓
                              ← Audio Chunks ←
```

## Development

### Type Checking

```bash
pnpm type-check
```

### Linting

```bash
pnpm lint
```

### Formatting

```bash
pnpm format
```

### Build for Production

```bash
pnpm build
pnpm start
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXT_PUBLIC_GRPC_HOST` | `localhost` | gRPC server hostname |
| `NEXT_PUBLIC_GRPC_PORT` | `50051` | gRPC server port |
| `NEXT_PUBLIC_GRPC_SSL` | `false` | Enable TLS/SSL |
| `NEXT_PUBLIC_ENABLE_VOICE_SELECTION` | `true` | Show voice selector |
| `NEXT_PUBLIC_ENABLE_SPEED_CONTROL` | `true` | Show speed control |
| `NEXT_PUBLIC_MAX_TEXT_LENGTH` | `10000` | Max input characters |
| `NEXT_PUBLIC_DEBUG_MODE` | `false` | Enable debug logging |

## Troubleshooting

### gRPC Connection Errors

**Problem**: "Connection refused" or "Service unavailable"

**Solution**:
- Verify gRPC service is running: `lsof -i :50051` (macOS/Linux) or `netstat -an | findstr 50051` (Windows)
- Check `.env.local` has correct `GRPC_HOST` and `GRPC_PORT`
- Ensure firewall allows port 50051

### Audio Not Playing

**Problem**: TTS succeeds but no audio

**Solution**:
- Check browser console for Web Audio API errors
- Verify browser audio permissions
- Try a different browser (Chrome/Firefox recommended)
- Check system audio output/volume

### High Latency

**Problem**: Long delay before audio starts

**Solution**:
- Check network latency: `ping localhost`
- Verify Kokoro TTS service is running
- Check CPU usage during synthesis

## Contributing

See the main repository's CONTRIBUTING.md for guidelines.

## License

See the main repository's LICENSE file.

## Links

- [Feature Specification](../../specs/005-nextjs-realtime-tts/spec.md)
- [Implementation Plan](../../specs/005-nextjs-realtime-tts/plan.md)
- [API Documentation](../../specs/005-nextjs-realtime-tts/contracts/)
- [Main Repository](../../README.md)
