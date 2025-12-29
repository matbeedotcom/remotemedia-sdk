# transcribe-srt

A thin wrapper around the remotemedia CLI with an embedded transcription pipeline.
Supports all the same I/O options: files, named pipes, and stdin/stdout.

## Installation

```bash
cd examples
cargo build -p transcribe-srt --release
```

## Usage

### File Input/Output

```bash
# MP4, MKV, MP3, WAV, etc.
transcribe-srt -i input.mp4 -o subtitles.srt

# Output to stdout
transcribe-srt -i input.mp4 -o -
```

### Pipe Workflows

```bash
# Pipe from ffmpeg (stdin must be WAV format)
ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | transcribe-srt -i - -o subtitles.srt

# Full pipeline: extract audio → transcribe → mux subtitles back
ffmpeg -i input.mp4 -f wav -ar 16000 -ac 1 - 2>/dev/null | \
  transcribe-srt -i - -o - | \
  ffmpeg -y -i input.mp4 -f srt -i pipe:0 -map 0:v -map 0:a -map 1:0 \
    -c:v copy -c:a copy -c:s mov_text output.mp4
```

### Options

```bash
# Use faster model
transcribe-srt -i input.mp4 -o out.srt --model quantized_tiny

# Different language
transcribe-srt -i input.mp4 -o out.srt --language es

# Verbose output
transcribe-srt -vv -i input.mp4 -o out.srt
```

### Available Models

With word timestamps (recommended for SRT):
- `large-v3-turbo` (default) - Best quality
- `quantized_tiny` - Fast
- `quantized_tiny_en` - Fast, English-only

Without word timestamps:
- `tiny`, `base`, `small`, `medium`, `large`

## How It Works

This is equivalent to running:

```bash
remotemedia run pipelines/transcribe-srt.yaml -i input.mp4 -O subtitles.srt
```

But with the pipeline embedded in the binary - no external YAML file needed.
