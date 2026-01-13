# Stream Health Monitor Demo

Real-time stream health monitoring demo binary for evaluating RemoteMedia SDK capabilities.

## Features

- **Stream Health Monitoring**: Detect drift, freeze, cadence issues, and A/V skew
- **Audio Analysis**: Silence detection, clipping, low volume, channel imbalance
- **Multiple Input Sources**: Files, stdin, named pipes, RTMP/RTSP/UDP streams
- **JSONL Output**: Machine-readable event stream for integration
- **Demo Mode**: Time-limited sessions for evaluation (15 min/session, 3 sessions/day)
- **License Activation**: Unlock unlimited usage with a license key

## Installation

```bash
cd examples/cli/stream-health-demo

# Basic build
cargo build --release

# With RTMP/RTSP/UDP support
cargo build --release --features rtmp
```

## Quick Start

### File Input

```bash
# Analyze a local file
./remotemedia-demo -i test.wav

# With JSONL output
./remotemedia-demo -i test.wav --json
```

### Stdin Pipe (FFmpeg)

```bash
# Pipe audio from FFmpeg
ffmpeg -i input.mp4 -f f32le -ar 16000 -ac 1 - | ./remotemedia-demo -i -

# With quiet mode (JSON only)
ffmpeg -i input.mp4 -f f32le -ar 16000 -ac 1 - 2>/dev/null | ./remotemedia-demo -i - --json -q
```

### RTMP/RTSP/UDP Ingest

With the `rtmp` feature enabled:

```bash
# RTMP stream
./remotemedia-demo --ingest rtmp://server/live/stream --json

# RTSP stream
./remotemedia-demo --ingest rtsp://server:8554/stream --json

# UDP listener (for FFmpeg tee output)
./remotemedia-demo --ingest udp://127.0.0.1:5004 --json

# SRT stream
./remotemedia-demo --ingest srt://server:9000 --json
```

### FFmpeg Tee Pattern (Production)

For non-blocking side-car monitoring of live streams:

```bash
# Primary stream continues uninterrupted
# Health monitor receives UDP side-car copy
ffmpeg -hide_banner -loglevel warning \
  -i rtmp://source/live/stream \
  -map 0:a -map 0:v \
  -c copy \
  -f tee \
  "[f=flv]rtmp://primary/live/stream|[f=mpegts]udp://127.0.0.1:5004?pkt_size=1316"

# Health monitor (separate terminal)
./remotemedia-demo --ingest udp://127.0.0.1:5004 --json
```

## Output Events

### Health Events (JSONL)

```json
{"type":"health","ts":"2025-01-01T00:00:00Z","score":1.0,"alerts":[]}
{"type":"drift","ts":"2025-01-01T00:00:01Z","lead_ms":5,"threshold_ms":50,"stream_id":"audio:0"}
{"type":"low_volume","ts":"2025-01-01T00:00:02Z","rms_db":-25.5,"peak_db":-15.0,"stream_id":"audio:0"}
{"type":"silence","ts":"2025-01-01T00:00:03Z","duration_ms":500,"threshold_db":-40.0,"stream_id":"audio:0"}
{"type":"clipping","ts":"2025-01-01T00:00:04Z","severity":0.8,"clipped_samples":150,"stream_id":"audio:0"}
```

### Event Types

| Event | Description |
|-------|-------------|
| `health` | Periodic health summary (score 0-1, alerts list) |
| `drift` | Stream timing drift detected |
| `freeze` | Video freeze detected |
| `cadence` | Irregular frame cadence |
| `av_skew` | Audio/video desynchronization |
| `low_volume` | Audio below volume threshold |
| `silence` | Sustained silence detected |
| `clipping` | Audio clipping/distortion |
| `channel_imbalance` | Left/right imbalance detected |
| `dropouts` | Intermittent audio dropouts |

## Demo Mode

In demo mode (no license), the following limits apply:

- **Session Duration**: 15 minutes maximum
- **Daily Sessions**: 3 sessions per day (resets at UTC midnight)
- **Startup Banner**: Shows demo limits

```bash
# Check demo limits
./remotemedia-demo --show-limits

# Bypass limits for development (env var)
REMOTEMEDIA_DEMO_UNLIMITED=1 ./remotemedia-demo --ingest rtsp://...
```

## License Activation

Evaluation licenses unlock unlimited usage and remove demo watermarks from outputs.

### Activating a License

```bash
# Activate license from a license file
./remotemedia-demo activate --file license.json

# Check current license status
./remotemedia-demo --license-status
```

### Using a License for a Session

```bash
# Use a specific license file for this session only
./remotemedia-demo --ingest rtsp://server/stream --license-file license.json

# Normal usage (uses installed license from ~/.config/remotemedia/license.json)
./remotemedia-demo --ingest rtsp://server/stream
```

### License Status

```bash
# View license details
./remotemedia-demo --license-status

# Example output (licensed):
#   License Status: VALID
#   Customer: ACME Corp
#   License ID: 940a5af3-...
#   Expires: 2027-01-01T23:59:59+00:00
#   Entitlements:
#     - Ingest schemes: file, udp, srt, rtmp
#     - Video processing: allowed
#
# Example output (unlicensed):
#   No license activated
#   Running in demo mode (15 min sessions, 3 per day)
```

### Entitlements

Licenses control access to features:

| Entitlement | Description |
|-------------|-------------|
| `allow_ingest_schemes` | Permitted input sources (file, udp, srt, rtmp) |
| `allow_video` | Video processing enabled |
| `max_session_duration_secs` | Maximum session length (unlimited if not set) |

### Watermarked Output

All JSONL events include an `_rm` metadata block:

```json
{"_rm":{"demo":false,"customer_id":"c9f4e3d2-...","watermark":"EVAL-ACME"},"type":"health","ts":"...","score":1.0}
```

In demo mode:
```json
{"_rm":{"demo":true,"watermark":"DEMO-UNLICENSED"},"type":"health","ts":"...","score":1.0}
```

## Command Line Options

```
USAGE:
    remotemedia-demo [OPTIONS] [COMMAND]

COMMANDS:
    activate              Activate a license from a file

OPTIONS:
    -i, --input <FILE>       Input file (WAV/audio) or - for stdin
        --ingest <URI>       Ingest from URI (rtmp://, rtsp://, udp://, file://)
        --json               Output events as JSONL
    -q, --quiet              Suppress banner and status messages
        --show-limits        Show demo mode limits
        --license-status     Display current license status
        --license-file <F>   Use specific license file for this session
    -h, --help               Print help
    -V, --version            Print version

ACTIVATE OPTIONS:
    --file <PATH>            Path to license.json file (required)
```

## Integration Examples

### Python Event Consumer

```python
import json
import subprocess

proc = subprocess.Popen(
    ["./remotemedia-demo", "--ingest", "rtsp://server/stream", "--json", "-q"],
    stdout=subprocess.PIPE,
    text=True
)

for line in proc.stdout:
    event = json.loads(line)
    if event["type"] == "silence":
        print(f"Silence detected: {event['duration_ms']}ms")
    elif event["type"] == "health" and event["score"] < 0.8:
        print(f"Health degraded: {event['score']}")
```

### Webhook Integration

```bash
# Pipe to webhook sender
./remotemedia-demo --ingest rtsp://server/stream --json -q | while read line; do
    curl -X POST -H "Content-Type: application/json" -d "$line" https://webhook.site/...
done
```

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug ./remotemedia-demo -i test.wav

# Build with all features
cargo build --features rtmp
```

## See Also

- [RemoteMedia SDK](../../../README.md)
- [Ingestion Framework](../../../runtime-core/README.md#ingestion-framework-spec-028)
- [Spec 027: Demo Binary](../../../specs/027-stream-health-demo-binary/)
