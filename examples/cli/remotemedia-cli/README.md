# RemoteMedia CLI

A command-line tool for running AI/ML pipelines locally or remotely.

## Installation

### From Source

```bash
cd examples/cli/remotemedia-cli
cargo build --release
```

The binary will be at `target/release/remotemedia`.

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
remotemedia run pipelines/transcribe.yaml --input audio.wav

# Generate speech from text
remotemedia run pipelines/tts.yaml --input "Hello, world!" --output speech.wav

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
remotemedia run <manifest> --input <file> [--output <file>]

Options:
  --input, -i    Input source: file path, named pipe (FIFO), or '-' for stdin
  --output, -o   Output destination: file path, named pipe (FIFO), or '-' for stdout
  --timeout      Execution timeout in seconds
```

### `stream` - Streaming Execution

Run a pipeline in streaming mode.

```bash
remotemedia stream <manifest> [options]

Options:
  --mic          Use microphone input
  --speaker      Use speaker output
  --input, -i    Input source: file path, named pipe (FIFO), or '-' for stdin
  --output, -o   Output destination: file path, named pipe (FIFO), or '-' for stdout
```

### `validate` - Validate Manifest

Check a pipeline manifest for errors.

```bash
remotemedia validate <manifest>

Checks:
  - YAML syntax
  - Node type existence
  - Connection validity
  - Cycle detection
  - Duplicate ID detection
```

### `serve` - Start Pipeline Server

Start a local server for pipeline execution.

```bash
remotemedia serve [options]

Options:
  --port, -p     Server port (default: 8080)
  --host         Bind address (default: 0.0.0.0)
  --manifest     Default pipeline manifest
```

### `nodes` - Node Management

```bash
# List all available nodes
remotemedia nodes list [--format json|table]

# Get detailed info about a node
remotemedia nodes info <node_type>
```

### `remote` - Remote Execution

Execute pipelines on a remote server.

```bash
# Run on remote server
remotemedia remote run --server grpc://host:50051 <manifest> --input <file>

# Stream on remote server
remotemedia remote stream --server ws://host:8080 <manifest> --mic --speaker
```

### `servers` - Server Management

Manage saved server configurations.

```bash
# List saved servers
remotemedia servers list

# Add a server
remotemedia servers add <name> <url> [--default]

# Remove a server
remotemedia servers remove <name>
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
-v, --verbose    Increase verbosity (-v, -vv, -vvv)
-q, --quiet      Suppress non-error output
-c, --config     Config file path
-o, --output-format    Output format (text, json, table)
```

## Examples

### Transcribe Audio File

```bash
remotemedia run pipelines/transcribe.yaml \
  --input recording.wav \
  --output transcript.txt
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
  --input large-file.wav
```

### JSON Output

```bash
remotemedia nodes list --output-format json | jq '.nodes[].name'
```

## Unix Pipes and Named Pipes (FIFOs)

The CLI supports Unix-style pipeline integration through:

### Standard Input/Output (`-` shorthand)

Use `-` as a shorthand for stdin (input) or stdout (output):

```bash
# Read from stdin
cat audio.wav | remotemedia run pipeline.yaml --input -

# Write to stdout
remotemedia run pipeline.yaml --input audio.wav --output -

# Filter mode (read from stdin, write to stdout)
cat audio.wav | remotemedia run pipeline.yaml --input - --output - > processed.wav
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
remotemedia stream pipeline.yaml --input /tmp/audio_in --output /tmp/audio_out
```

### Pipeline Composition with FFmpeg

```bash
# Convert and transcribe in one pipeline
ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | \
  remotemedia run transcribe.yaml --input - --output transcript.txt

# Real-time audio processing
ffmpeg -f pulse -i default -f wav - | \
  remotemedia stream voice-assistant.yaml --input - --speaker
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
