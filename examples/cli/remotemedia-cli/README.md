# RemoteMedia CLI

A command-line tool for running AI/ML pipelines locally or remotely.

## Installation

### From Source

```bash
cd examples/cli/remotemedia-cli
cargo build --release
```

The binary will be at `target/release/remotemedia`.

### Transport Features

The CLI uses Cargo features to control which transport backends are compiled in. Only enable what you need to keep compile times fast.

| Feature  | Transport | Default |
|----------|-----------|---------|
| `grpc`   | gRPC      | Yes     |
| `http`   | HTTP/REST + SSE | No |
| `webrtc` | WebRTC    | No      |
| `ui`     | Web UI    | No      |
| `candle` | Candle ML nodes | No |

```bash
# Default build (gRPC only)
cargo build --release

# With all transports
cargo build --release --features grpc,http,webrtc

# HTTP only (no gRPC)
cargo build --release --no-default-features --features http

# With Candle ML nodes
cargo build --release --features candle

# With embedded web UI
cargo build --release --features ui
```

### Add to PATH

```bash
# Linux/macOS
export PATH="$PATH:$(pwd)/target/release"

# Or install globally
cargo install --path .
```

## Quick Start

### Run a Pipeline

```bash
# Transcribe an audio file
remotemedia run pipelines/transcribe.yaml -i audio.wav

# Generate speech from text
remotemedia run pipelines/tts.yaml -i "Hello, world!" -O speech.wav

# Stream with microphone and speaker
remotemedia stream pipelines/voice-assistant.yaml --mic --speaker
```

### Validate a Pipeline

```bash
remotemedia validate pipeline.yaml
```

### List Available Nodes

```bash
remotemedia nodes list
remotemedia nodes info WhisperSTT
```

## Commands

### `run` - Execute a Pipeline

Run a pipeline with file input/output.

```bash
remotemedia run <manifest> [options]

Options:
  -i, --input          Input source: file path, named pipe (FIFO), or '-' for stdin
  -O, --output         Output destination: file path, named pipe (FIFO), or '-' for stdout
  --params             Override node parameters (JSON string)
  --timeout            Execution timeout (default: 300s)
  --input-format       Input format hint: auto, wav, text, json, raw (default: auto)
```

### `stream` - Streaming Execution

Run a pipeline in streaming mode.

```bash
remotemedia stream <manifest> [options]

Options:
  --mic                Use microphone input
  --speaker            Play audio output to speaker
  -i, --input          Input source: file path, named pipe (FIFO), or '-' for stdin
  -O, --output         Output destination: file path, named pipe (FIFO), or '-' for stdout
  --sample-rate        Audio sample rate (default: 16000)
  --channels           Audio channels, 1 or 2 (default: 1)
  --chunk-size         Chunk size in samples for streaming (default: 4000)
```

### `validate` - Validate Manifest

Check a pipeline manifest for errors.

```bash
remotemedia validate <manifest> [options]

Options:
  --check-nodes        Verify that node types are registered
  --server             Check nodes against a remote server

Checks:
  - YAML syntax
  - Node type existence
  - Connection validity
  - Cycle detection
  - Duplicate ID detection
```

### `serve` - Start Pipeline Server

Start a local server for pipeline execution. Requires the corresponding transport feature to be enabled at build time.

```bash
remotemedia serve <manifest> [options]

Options:
  --port               Server port (default: 8080)
  --host               Bind address (default: 0.0.0.0)
  --transport          Transport type: grpc, http, webrtc (default: grpc)
  --auth-token         Require this token for authentication
  --max-sessions       Maximum concurrent sessions (default: 100)
  --ui                 Enable embedded web UI (requires --features ui)
  --ui-port            Web UI port when --ui is enabled (default: 3001)
  --signal-port        Start a signaling server for browser WebRTC connections
  --signal-type        Signaling protocol: websocket or grpc (default: websocket)
```

```bash
# Start a gRPC server (default)
remotemedia serve pipeline.yaml --port 50051

# Start an HTTP server (requires --features http)
remotemedia serve pipeline.yaml --transport http --port 8080

# Start with authentication
remotemedia serve pipeline.yaml --transport grpc --auth-token my-secret

# Start with WebRTC and browser signaling
remotemedia serve pipeline.yaml --transport webrtc --signal-port 18091

# Start with embedded web UI
remotemedia serve pipeline.yaml --transport webrtc --signal-port 18091 --ui
```

If you select a transport that wasn't compiled in, the CLI will print a message telling you which feature flag to enable.

### `nodes` - Node Management

```bash
# List all available nodes
remotemedia nodes list

# Filter nodes by name pattern
remotemedia nodes list --filter "audio"

# List nodes from a remote server
remotemedia nodes list --server grpc://localhost:50051

# Get detailed info about a node
remotemedia nodes info <node_type>
```

### `remote` - Remote Execution

Execute pipelines on a remote server.

```bash
# Run a local manifest on a remote server
remotemedia remote run --server grpc://host:50051 manifest.yaml -i audio.wav

# Run a named pipeline on a remote server
remotemedia remote run --server grpc://host:50051 --pipeline transcribe -i audio.wav

# Stream a named pipeline on a remote server
remotemedia remote stream --server ws://host:8080 --pipeline voice-assistant --mic --speaker
```

### `servers` - Server Management

Manage saved server configurations.

```bash
# List saved servers
remotemedia servers list

# Add a server
remotemedia servers add <name> <url> [--default] [--auth-token <token>]

# Remove a server
remotemedia servers remove <name>
```

### `models` - Model Management

Manage Candle ML model cache (requires `--features candle`).

```bash
# List cached models
remotemedia models list

# Show cache statistics
remotemedia models stats

# Download a model for offline use
remotemedia models download "openai/whisper-base"

# Download a specific file
remotemedia models download "openai/whisper-base" -f config.json

# Remove a model from cache
remotemedia models remove "openai/whisper-base"
```

### `pack` - Package a Pipeline

Pack a pipeline into a self-contained Python wheel.

```bash
remotemedia pack <pipeline.yaml> [options]

Options:
  -O, --output         Output directory (default: ./dist)
  -n, --name           Override package name (default: from manifest)
  --pkg-version        Package version (default: 0.1.0)
  --build              Build the wheel after generating
  --release            Build in release mode (requires --build)
  --test               Run tests after building (requires --build)
```

## Configuration

Configuration is stored in `~/.remotemedia/`:

### config.toml

```toml
[default]
output_format = "text"  # text, json, table

[audio]
sample_rate = 48000
channels = 1
input_device = "default"
output_device = "default"

[execution]
timeout = 300
```

### servers.toml

```toml
[servers.local]
url = "grpc://localhost:50051"
default = true

[servers.cloud]
url = "grpc://api.example.com:50051"
auth_token = "..."
```

## Global Options

```bash
-v, --verbose          Increase verbosity (-v, -vv, -vvv)
-q, --quiet            Suppress non-error output
-c, --config           Config file path
-o, --output-format    Output format (text, json, table)
```

## Examples

### Transcribe Audio File

```bash
remotemedia run pipelines/transcribe.yaml \
  -i recording.wav \
  -O transcript.txt
```

### Voice Assistant Session

```bash
remotemedia stream pipelines/voice-assistant.yaml \
  --mic --speaker
```

### Remote Execution

```bash
# Add a remote server
remotemedia servers add cloud grpc://ml.example.com:50051

# Run transcription remotely
remotemedia remote run --server cloud \
  pipelines/transcribe.yaml \
  -i large-file.wav
```

### JSON Output

```bash
remotemedia nodes list -o json | jq '.nodes[].name'
```

## Unix Pipes and Named Pipes (FIFOs)

The CLI supports Unix-style pipeline integration through:

### Standard Input/Output (`-` shorthand)

Use `-` as a shorthand for stdin (input) or stdout (output):

```bash
# Read from stdin
cat audio.wav | remotemedia run pipeline.yaml -i -

# Write to stdout
remotemedia run pipeline.yaml -i audio.wav -O -

# Filter mode (read from stdin, write to stdout)
cat audio.wav | remotemedia run pipeline.yaml -i - -O - > processed.wav
```

### Named Pipes (FIFOs)

Named pipes enable integration with other processes for continuous streaming:

```bash
# Create named pipes
mkfifo /tmp/audio_in
mkfifo /tmp/audio_out

# In terminal 1: Feed audio to the pipeline
ffmpeg -i input.mp3 -f wav - > /tmp/audio_in

# In terminal 2: Consume processed output
cat /tmp/audio_out > processed.wav

# In terminal 3: Run the pipeline
remotemedia stream pipeline.yaml -i /tmp/audio_in -O /tmp/audio_out
```

### Pipeline Composition with FFmpeg

```bash
# Convert and transcribe in one pipeline
ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | \
  remotemedia run transcribe.yaml -i - -O transcript.txt

# Real-time audio processing
ffmpeg -f pulse -i default -f wav - | \
  remotemedia stream voice-assistant.yaml -i - --speaker
```

### Notes on Named Pipes

- **Blocking behavior**: Opening a named pipe will block until both a reader and writer are connected (standard Unix behavior)
- **SIGPIPE handling**: If the output reader closes, the CLI will exit cleanly with code 141 (128 + SIGPIPE)
- **Platform support**: Named pipes are supported on Linux and macOS. Windows named pipes use a different mechanism and are not currently supported.

## Environment Variables

- `REMOTEMEDIA_CONFIG` - Config file path
- `REMOTEMEDIA_DEFAULT_SERVER` - Default server URL
- `REMOTEMEDIA_LOG` - Log level (error, warn, info, debug, trace)

## Model Setup

See [MODELS.md](../../MODELS.md) for instructions on downloading required models.

## License

Apache-2.0
