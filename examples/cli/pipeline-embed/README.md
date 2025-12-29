# pipeline-embed

Compile any pipeline YAML into a standalone executable at build time, with comprehensive audio device support and build-time configurable defaults.

## Usage

### Build with a specific pipeline

```bash
# From the examples directory (use absolute path for PIPELINE_YAML)
cd examples
PIPELINE_YAML=$PWD/cli/pipelines/transcribe-srt.yaml cargo build -p pipeline-embed --release

# The binary is at target/release/pipeline-runner
# Rename it to something meaningful
cp target/release/pipeline-runner ./my-transcriber

# Verify the embedded pipeline
./my-transcriber --show-pipeline

# Check embedded defaults
./my-transcriber --show-defaults
```

### Use the compiled binary

```bash
# File input/output
./my-transcriber -i input.mp4 -o output.srt

# Pipe support
ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | ./my-transcriber -i - -o -

# Live microphone input with speaker output
./my-transcriber --mic --speaker

# Show embedded pipeline
./my-transcriber --show-pipeline
```

## Build-Time Configuration

You can configure default CLI values at build time, making the resulting binary pre-configured for specific use cases. This is useful for creating "batteries-included" tools that work out of the box for their intended purpose.

### Environment Variables

```bash
# Build a streaming voice assistant with pre-configured defaults
PIPELINE_YAML=$PWD/pipelines/voice-assistant.yaml \
PIPELINE_STREAM=true \
PIPELINE_MIC=true \
PIPELINE_SPEAKER=true \
PIPELINE_SAMPLE_RATE=16000 \
PIPELINE_CHANNELS=1 \
PIPELINE_CHUNK_SIZE=4000 \
cargo build -p pipeline-embed --release

# The resulting binary defaults to streaming mode with mic/speaker enabled
./voice-assistant  # Just run it - no flags needed!
```

#### Available Build Environment Variables

| Variable | Type | Description |
|----------|------|-------------|
| `PIPELINE_YAML` | path | Path to pipeline YAML file (required) |
| `PIPELINE_STREAM` | bool | Enable streaming mode by default |
| `PIPELINE_MIC` | bool | Enable microphone input by default |
| `PIPELINE_SPEAKER` | bool | Enable speaker output by default |
| `PIPELINE_SAMPLE_RATE` | u32 | Default sample rate in Hz |
| `PIPELINE_CHANNELS` | u16 | Default number of channels |
| `PIPELINE_CHUNK_SIZE` | usize | Default chunk size in samples |
| `PIPELINE_TIMEOUT` | u64 | Default timeout in seconds |
| `PIPELINE_INPUT_DEVICE` | string | Default input device name |
| `PIPELINE_OUTPUT_DEVICE` | string | Default output device name |
| `PIPELINE_AUDIO_HOST` | string | Default audio host/backend |
| `PIPELINE_BUFFER_MS` | u32 | Default buffer size in ms |

### Pipeline Metadata Defaults

Alternatively, configure defaults directly in your pipeline YAML:

```yaml
version: v1
metadata:
  name: voice-assistant
  description: Real-time voice assistant
  cli_defaults:
    stream: true
    mic: true
    speaker: true
    sample_rate: 16000
    channels: 1
    chunk_size: 4000
    timeout_secs: 3600
    input_device: "USB Microphone"  # optional
    output_device: "Speakers"       # optional
    audio_host: "pulse"             # optional (Linux: alsa, pulse; macOS: coreaudio)
    buffer_ms: 10

nodes:
  - id: asr
    node_type: WhisperASR
    # ...
```

**Note:** Environment variables take precedence over metadata defaults.

## Audio Device Support

The embedded pipeline runner includes full audio device support with ffmpeg-inspired CLI arguments.

### List Available Devices

```bash
# List all audio input/output devices
./my-transcriber --list-devices
```

### Device Selection (ffmpeg-style)

```bash
# Use specific input device by name
./my-transcriber --mic -D "USB Microphone"
./my-transcriber --mic --input-device "Built-in Microphone"

# Use specific input device by index (0-based)
./my-transcriber --mic -D 0

# Use specific output device
./my-transcriber --speaker -O "DAC"
./my-transcriber --speaker --output-device 1

# ALSA-style device selection (Linux)
./my-transcriber --mic -D hw:0,0 --speaker -O hw:1,0
```

### Audio Configuration

```bash
# Set sample rate (-r or --sample-rate)
./my-transcriber --mic -r 16000

# Set channels (-c or --channels)
./my-transcriber --mic -c 1    # mono
./my-transcriber --mic -c 2    # stereo

# Set buffer size (latency tuning)
./my-transcriber --mic --buffer-ms 10

# Combined example: USB mic at 48kHz stereo, output to DAC
./my-transcriber --mic -D "USB Mic" -r 48000 -c 2 --speaker -O "DAC"
```

### Audio Host Selection

```bash
# Use specific audio host/backend (platform-specific)
# Linux: alsa, pulse, jack
# macOS: coreaudio  
# Windows: wasapi, asio
./my-transcriber --mic --audio-host pulse
./my-transcriber --mic --audio-host alsa -D hw:0
```

### CLI Options

```
Options:
  -i, --input <INPUT>          Input source: file path, named pipe, or `-` for stdin
  -o, --output <OUTPUT>        Output destination [default: -]
  
Audio Input:
      --mic                    Capture audio from input device
  -D, --input-device <DEVICE>  Input device name, index, or 'default'
      --audio-host <HOST>      Audio host/backend (alsa, pulse, coreaudio, wasapi)
  -r, --sample-rate <RATE>     Audio sample rate in Hz [default: from build or 48000]
  -c, --channels <CHANNELS>    Number of audio channels [default: from build or 1]
      --buffer-ms <MS>         Audio buffer size in milliseconds [default: from build or 20]
      --sample-format <FMT>    Sample format: f32, i16, i32 [default: f32]

Audio Output:
      --speaker                Play audio through output device
  -O, --output-device <DEVICE> Output device name, index, or 'default'
      --output-sample-rate     Output sample rate (default: same as input)
      --output-channels        Output channels (default: same as input)

Device Management:
      --list-devices           List available audio input/output devices
      --show-device-info       Show detailed capabilities for selected devices

Streaming:
      --stream                 Run in streaming mode for real-time processing
      --no-stream              Disable streaming (override embedded default)
      --chunk-size <SAMPLES>   Audio chunk size in samples [default: from build or 4000]

General:
      --timeout <TIMEOUT>      Execution timeout [default: from build or 600s]
  -v, --verbose...             Increase verbosity (-v, -vv, -vvv)
  -q, --quiet                  Suppress non-error output
      --show-pipeline          Show the embedded pipeline YAML and exit
      --show-defaults          Show the build-time configured defaults and exit
  -h, --help                   Print help
```

## How It Works

1. **Build time**: The `build.rs` script reads `PIPELINE_YAML` env var and embeds the content
2. **Defaults extraction**: CLI defaults are read from env vars and/or pipeline metadata
3. **Compile**: The YAML and defaults become constants in the binary
4. **Runtime**: No file I/O needed - the pipeline and defaults are already in memory

## Execution Modes

### Unary Mode (Default unless configured otherwise)

Processes a single input file to produce a single output:

```bash
./my-transcriber -i input.wav -o output.json
```

### Streaming Mode

Continuously processes audio from microphone or pipe. Enabled by:
- `--stream` flag
- `--mic` flag
- Build-time `PIPELINE_STREAM=true`
- Pipeline metadata `cli_defaults.stream: true`

```bash
# Live transcription from microphone
./my-transcriber --mic --stream

# Process audio stream from pipe
ffmpeg -i stream.mp3 -f wav -ar 16000 -ac 1 - | ./my-transcriber -i - --stream
```

## Creating Distribution Binaries

### Basic File-Processing Tool

```bash
cd examples

# Build transcription tool for file processing
PIPELINE_YAML=$PWD/cli/pipelines/transcribe-srt.yaml cargo build -p pipeline-embed --release
cp target/release/pipeline-runner dist/transcribe-srt

# Usage
./transcribe-srt -i video.mp4 -o output.srt
```

### Streaming Voice Assistant

```bash
# Build with streaming defaults
PIPELINE_YAML=$PWD/cli/pipelines/voice-assistant.yaml \
PIPELINE_STREAM=true \
PIPELINE_MIC=true \
PIPELINE_SPEAKER=true \
PIPELINE_SAMPLE_RATE=16000 \
cargo build -p pipeline-embed --release

cp target/release/pipeline-runner dist/voice-assistant

# Users can just run it - no flags needed!
./voice-assistant

# Or override defaults
./voice-assistant --no-stream -i recording.wav -o transcript.txt
```

### Low-Latency Audio Tool

```bash
# Build with low-latency audio defaults
PIPELINE_YAML=$PWD/cli/pipelines/audio-effects.yaml \
PIPELINE_STREAM=true \
PIPELINE_MIC=true \
PIPELINE_SPEAKER=true \
PIPELINE_SAMPLE_RATE=48000 \
PIPELINE_BUFFER_MS=5 \
PIPELINE_CHUNK_SIZE=256 \
cargo build -p pipeline-embed --release

cp target/release/pipeline-runner dist/audio-fx
```

**Note:** `PIPELINE_YAML` must be an absolute path since Cargo runs the build script from a different directory.

## Examples

### Real-time Voice Transcription

```bash
# List devices first
./my-transcriber --list-devices

# Use USB microphone for transcription, output to terminal
./my-transcriber --mic -D "USB Audio" -r 16000 -c 1 --stream

# With speaker feedback (echo back processed audio)
./my-transcriber --mic --speaker -r 16000
```

### Processing Multiple Audio Files

```bash
for file in *.wav; do
  ./my-transcriber -i "$file" -o "${file%.wav}.json"
done
```

### Integration with FFmpeg

```bash
# Extract and process audio from video
ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | ./my-transcriber -i - -o transcript.json

# Real-time stream processing
ffmpeg -f pulse -i default -f wav -ar 16000 -ac 1 - | ./my-transcriber -i - --stream
```

## Comparison to transcribe-srt

`transcribe-srt` is a specialized wrapper with:
- Custom CLI args (--model, --language, --threads)
- Template variable substitution in the YAML

`pipeline-embed` is generic:
- Works with any pipeline YAML
- Uses the YAML as-is (no substitution)
- Full audio device support
- Build-time configurable defaults
- Streaming mode

Use `transcribe-srt` for the transcription use case. Use `pipeline-embed` for quick prototyping, live audio processing, or distributing other pipelines with pre-configured defaults.
