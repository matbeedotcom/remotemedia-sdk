# Sample Files for Testing

This directory contains sample audio and video files for testing example applications.

## Audio Samples

| File | Duration | Format | Description |
|------|----------|--------|-------------|
| `hello.wav` | ~2s | WAV 16kHz mono | Simple "Hello world" speech |
| `conversation.wav` | ~30s | WAV 48kHz mono | Multi-turn conversation sample |

## Video Samples

| File | Duration | Format | Description |
|------|----------|--------|-------------|
| `interview.mp4` | ~60s | MP4 H.264 | Interview clip for transcription |

## Usage

Reference these files in examples:

```bash
# CLI
remotemedia run transcribe.yaml --input ../samples/hello.wav

# Python
audio = load_audio("../../samples/hello.wav")
```

## Generating Samples

If samples are not included (due to file size), generate them:

```bash
# Generate test tone
ffmpeg -f lavfi -i "sine=frequency=440:duration=2" -ar 16000 -ac 1 hello.wav

# Generate speech sample (requires TTS model)
remotemedia run tts.yaml --input "Hello, this is a test." --output hello.wav
```

## License

Sample files are provided under CC0 (public domain) for testing purposes.
