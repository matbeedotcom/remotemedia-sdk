# transcribe-srt

A self-contained audio-to-SRT subtitle transcription tool using Whisper.

## Features

- **Single binary** - No external configuration files needed
- **Whisper integration** - Uses large-v3-turbo model by default for high-quality transcription
- **SRT output** - Generates properly formatted SRT subtitles with timestamps
- **Configurable** - Supports multiple Whisper models and languages

## Installation

Build from the examples workspace:

```bash
cd examples
cargo build -p transcribe-srt --release
```

The binary will be at `examples/target/release/transcribe-srt`.

## Usage

### Basic

```bash
# Transcribe to stdout
transcribe-srt input.wav

# Write to file
transcribe-srt input.wav -o subtitles.srt
```

### Options

```bash
# Use faster model (lower quality)
transcribe-srt --model quantized_tiny input.wav -o subtitles.srt

# Different language
transcribe-srt --language es input.wav -o spanish.srt

# Verbose output for debugging
transcribe-srt -vv input.wav -o subtitles.srt

# Custom thread count
transcribe-srt --threads 8 input.wav -o subtitles.srt
```

### Available Models

Models with word-level timestamps (recommended for SRT):
- `quantized_tiny` - Fast, lower quality
- `quantized_tiny_en` - English-only variant
- `large-v3-turbo` (default) - Best quality

Models without word timestamps:
- `tiny`, `base`, `small`, `medium`, `large`

## Example Output

```srt
1
00:00:01,000 --> 00:00:04,500
Hello, this is the first subtitle.

2
00:00:05,000 --> 00:00:08,200
And this is the second one.
```

## Comparison to CLI

Instead of:
```bash
remotemedia run pipelines/transcribe-srt.yaml -i audio.wav -O subtitles.srt
```

Use:
```bash
transcribe-srt audio.wav -o subtitles.srt
```

Benefits:
- Single binary, no YAML file needed
- Simpler command-line interface
- Built-in model/language options
- Better error messages for audio transcription
