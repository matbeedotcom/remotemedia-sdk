# pipeline-embed

Compile any pipeline YAML into a standalone executable at build time.

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
```

### Use the compiled binary

```bash
# File input/output
./my-transcriber -i input.mp4 -o output.srt

# Pipe support
ffmpeg -i video.mp4 -f wav -ar 16000 -ac 1 - | ./my-transcriber -i - -o -

# Show embedded pipeline
./my-transcriber --show-pipeline
```

### CLI Options

```
Options:
  -i, --input <INPUT>      Input source: file path, named pipe, or `-` for stdin
  -o, --output <OUTPUT>    Output destination [default: -]
      --timeout <TIMEOUT>  Execution timeout [default: 600s]
  -v, --verbose...         Increase verbosity (-v, -vv, -vvv)
  -q, --quiet              Suppress non-error output
      --show-pipeline      Show the embedded pipeline YAML and exit
  -h, --help               Print help
```

## How It Works

1. **Build time**: The `build.rs` script reads `PIPELINE_YAML` env var and embeds the content
2. **Compile**: The YAML becomes a `const &str` in the binary
3. **Runtime**: No file I/O needed - the pipeline is already in memory

## Creating Distribution Binaries

```bash
cd examples

# Build transcription tool
PIPELINE_YAML=$PWD/cli/pipelines/transcribe-srt.yaml cargo build -p pipeline-embed --release
cp target/release/pipeline-runner dist/transcribe-srt

# Build another pipeline  
PIPELINE_YAML=/absolute/path/to/my-pipeline.yaml cargo build -p pipeline-embed --release
cp target/release/pipeline-runner dist/my-tool
```

**Note:** `PIPELINE_YAML` must be an absolute path since Cargo runs the build script from a different directory.

## Comparison to transcribe-srt

`transcribe-srt` is a specialized wrapper with:
- Custom CLI args (--model, --language, --threads)
- Template variable substitution in the YAML

`pipeline-embed` is generic:
- Works with any pipeline YAML
- Uses the YAML as-is (no substitution)
- Simpler CLI

Use `transcribe-srt` for the transcription use case. Use `pipeline-embed` for quick prototyping or distributing other pipelines.
