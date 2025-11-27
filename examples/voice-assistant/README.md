# Voice Assistant

A desktop voice assistant application built with Tauri 2.x and RemoteMedia SDK.

## Features

- **Voice Input**: Press-to-talk or continuous listening with VAD
- **Text Input**: Type messages directly
- **Multiple Modes**:
  - **Local**: All processing on device (requires local models)
  - **Hybrid**: Local VAD + remote inference (responsive + powerful)
  - **Remote**: All processing on server (minimal device requirements)
- **Automatic Fallback**: Hybrid mode falls back to local when remote unavailable
- **Real-time Feedback**: Voice activity indicator, streaming transcription

## Prerequisites

### Local Mode Requirements

For local processing, you need:

1. **Whisper Model** (for speech-to-text)
   ```bash
   # Download from Hugging Face
   huggingface-cli download ggerganov/whisper.cpp models/ggml-base.en.bin
   ```

2. **Ollama** (for LLM inference)
   ```bash
   # Install Ollama
   curl -fsSL https://ollama.com/install.sh | sh

   # Pull model
   ollama pull llama3.2:1b
   ```

3. **Kokoro TTS Model**
   ```bash
   # Download from Hugging Face
   huggingface-cli download hexgrad/kokoro-v0_19
   ```

### Remote/Hybrid Mode Requirements

- A running RemoteMedia server (see server setup in main docs)

## Installation

### From Source

```bash
# Install frontend dependencies
npm install

# Build and run in development
npm run tauri dev

# Build for production
npm run tauri build
```

### Pre-built Binaries

Download from the releases page for your platform.

## Usage

### Quick Start

1. Launch the application
2. Select your execution mode (Local/Hybrid/Remote)
3. If using Hybrid or Remote, enter your server URL in Settings
4. Click the microphone button or press Space to start listening
5. Speak naturally - the assistant will respond

### Keyboard Shortcuts

- `Space`: Toggle listening
- `Enter`: Send typed message
- `Esc`: Stop current response

### Settings

Access settings via the gear icon:

- **Mode**: Local, Hybrid, or Remote
- **Remote Server**: URL for hybrid/remote modes
- **LLM Model**: Which language model to use
- **TTS Voice**: Voice selection for responses
- **VAD Threshold**: Speech detection sensitivity
- **Auto-listen**: Continue listening after response

## Architecture

```
┌─────────────────────────────────────────────────┐
│                    App.tsx                       │
│  ┌──────────────────────────────────────────┐   │
│  │           TranscriptPanel                 │   │
│  │  [User messages and assistant responses]  │   │
│  └──────────────────────────────────────────┘   │
│                                                  │
│  ┌─────────────┐  ┌────────────────────────┐   │
│  │ VoiceIndicator │  │    MicrophoneButton    │   │
│  └─────────────┘  └────────────────────────┘   │
│                                                  │
│  ┌──────────────────────────────────────────┐   │
│  │           Text Input Field                │   │
│  └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│              Tauri Backend (Rust)                │
│                                                  │
│  Commands: initialize_pipeline, start_listening, │
│           stop_listening, send_text_input       │
│                                                  │
│  Events: transcription, response, vad_state,    │
│          mode_changed, error, audio_output      │
│                                                  │
│  Modes: Local → Direct pipeline execution       │
│         Hybrid → RemotePipelineNode + fallback  │
│         Remote → Full remote execution          │
└─────────────────────────────────────────────────┘
```

## Development

### Project Structure

```
voice-assistant/
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── store/              # Zustand state management
│   ├── hooks/              # React hooks
│   └── __tests__/          # Frontend tests
├── src-tauri/              # Tauri backend (Rust)
│   ├── src/
│   │   ├── commands/       # Tauri command handlers
│   │   ├── events/         # Event emitters
│   │   └── modes/          # Execution mode logic
│   └── tests/              # Backend tests
└── pipelines/              # Pipeline manifests
```

### Running Tests

```bash
# Frontend tests
npm test

# Backend tests
cd src-tauri
cargo test
```

### Building for Release

```bash
npm run tauri build
```

Binaries will be in `src-tauri/target/release/`.

## Troubleshooting

### "No audio input device found"

- Check microphone permissions in system settings
- Ensure a microphone is connected

### "Model not found"

- Verify models are downloaded to the correct location
- Check MODELS.md for download instructions

### "Connection refused" (Remote/Hybrid)

- Verify server URL is correct
- Check network connectivity
- Ensure server is running

## License

Apache-2.0
