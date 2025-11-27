# Shared Pipeline Manifests

This directory contains reusable pipeline manifests shared across example applications.

## Pipelines

| File | Description | Use Case |
|------|-------------|----------|
| `voice-assistant.yaml` | VAD → STT → LLM → TTS | Voice assistant applications |
| `transcribe.yaml` | Whisper transcription | Audio/video transcription |
| `tts.yaml` | Text-to-speech synthesis | TTS applications |

## Usage

Reference these manifests in your example application:

```rust
let manifest = Manifest::from_file("../../shared-pipelines/voice-assistant.yaml")?;
```

```python
manifest = load_manifest("../../shared-pipelines/transcribe.yaml")
```

## Local vs Remote

All pipelines work in local-only mode by default. For remote processing, use the corresponding `*-remote.yaml` variant or configure `RemotePipelineNode` in your application.
