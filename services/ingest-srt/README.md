# SRT Ingest Gateway

Push-based SRT ingest endpoint for real-time stream health monitoring. Users push media via FFmpeg/GStreamer to a private SRT URL; the gateway demuxes, decodes, runs analysis pipelines, and emits alerts via webhooks and SSE.

## Quick Start

### 1. Start the Gateway

```bash
# Build and run
cargo run -p remotemedia-ingest-srt --release

# Or with environment configuration
INGEST_HTTP_PORT=8080 INGEST_SRT_PORT=9000 cargo run -p remotemedia-ingest-srt
```

### 2. Create a Session

```bash
curl -X POST http://localhost:8080/api/ingest/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline": "demo_audio_quality_v1",
    "webhook_url": "https://your-server.com/webhook",
    "audio_enabled": true,
    "video_enabled": false,
    "max_duration_seconds": 300
  }'
```

Response:
```json
{
  "session_id": "sess_abc123def456",
  "srt_url": "srt://localhost:9000?mode=caller&streamid=...",
  "ffmpeg_command_copy": "ffmpeg -re -i \"<YOUR_SOURCE>\" -c copy -f mpegts \"srt://...\"",
  "ffmpeg_command_transcode": "ffmpeg -re -i \"<YOUR_SOURCE>\" -c:v libx264 ... \"srt://...\"",
  "events_url": "/api/ingest/sessions/sess_abc123def456/events",
  "expires_at": "2025-01-01T00:05:00Z"
}
```

### 3. Push Media with FFmpeg

```bash
# Copy mode (lowest CPU, requires compatible source)
ffmpeg -re -i input.mp4 -c copy -f mpegts "srt://localhost:9000?mode=caller&streamid=..."

# Transcode mode (most compatible)
ffmpeg -re -i input.mp4 \
  -c:v libx264 -preset veryfast -tune zerolatency -g 60 -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts "srt://localhost:9000?mode=caller&streamid=..."
```

### 4. Receive Events

#### Via SSE (Server-Sent Events)
```bash
curl -N http://localhost:8080/api/ingest/sessions/sess_abc123def456/events
```

Events are sent as:
```
event: alert
data: {"type":"silence","ts":"...","duration_ms":3500,"rms_db":-60.0}

event: health
data: {"type":"health","ts":"...","score":0.85,"alerts":["SILENCE"]}

event: system
data: {"type":"stream_ended","ts":"...","reason":"client_disconnect"}
```

#### Via Webhook
Webhooks receive POST requests with JSON payload:
```json
{
  "event_type": "silence",
  "session_id": "sess_abc123def456",
  "timestamp": "2025-01-01T00:00:30.000Z",
  "relative_ms": 30000,
  "data": {
    "type": "silence",
    "duration_ms": 3500,
    "rms_db": -60.0
  }
}
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/ingest/sessions` | Create a new ingest session |
| `GET` | `/api/ingest/sessions/:id` | Get session status |
| `DELETE` | `/api/ingest/sessions/:id` | End a session |
| `GET` | `/api/ingest/sessions/:id/events` | SSE event stream |
| `GET` | `/health` | Health check |
| `GET` | `/metrics` | Gateway metrics (JSON) |

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `INGEST_HTTP_PORT` | `8080` | HTTP API port |
| `INGEST_SRT_PORT` | `9000` | SRT listener port |
| `INGEST_HOST` | `0.0.0.0` | Bind address |
| `INGEST_PUBLIC_HOST` | `localhost` | Public hostname for SRT URLs |
| `INGEST_JWT_SECRET` | (default) | JWT signing secret (change in production!) |
| `INGEST_JWT_TTL` | `900` | JWT token TTL in seconds |
| `INGEST_MAX_SESSIONS` | `100` | Maximum concurrent sessions |
| `INGEST_MAX_DURATION` | `3600` | Maximum session duration in seconds |
| `INGEST_PIPELINES_DIR` | `./pipelines` | Pipeline templates directory |

### TOML Configuration

```toml
[server]
http_port = 8080
srt_port = 9000
host = "0.0.0.0"
public_host = "ingest.example.com"

[jwt]
secret = "your-production-secret"
token_ttl_seconds = 900

[limits]
max_sessions = 100
max_session_duration_seconds = 3600
max_bitrate_bps = 10000000
audio_queue_ms = 500
video_queue_frames = 5

[webhooks]
timeout_seconds = 10
max_retries = 3
retry_backoff_ms = 1000
```

## Event Types

### Alert Events
| Type | Description | Fields |
|------|-------------|--------|
| `silence` | Audio silence detected | `duration_ms`, `rms_db` |
| `low_volume` | Low audio volume | `rms_db`, `peak_db` |
| `clipping` | Audio clipping/distortion | `saturation_ratio`, `crest_factor_db` |
| `channel_imbalance` | One-sided audio | `imbalance_db`, `dead_channel` |
| `dropouts` | Intermittent audio dropouts | `dropout_count` |
| `drift` | A/V drift detected | `lead_ms`, `threshold_ms` |
| `freeze` | Video freeze detected | `duration_ms` |

### System Events
| Type | Description | Fields |
|------|-------------|--------|
| `stream_started` | Stream began receiving data | `session_id` |
| `stream_ended` | Stream ended | `reason`, `session_id` |

### Health Events
| Type | Description | Fields |
|------|-------------|--------|
| `health` | Periodic health score | `score` (0.0-1.0), `alerts` |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        remotemedia-ingest-srt                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌──────────────────┐                                                   │
│  │  HTTP API Server │ ← POST /api/ingest/sessions                      │
│  │  (axum)          │ ← GET  /api/ingest/sessions/:id/events (SSE)     │
│  └────────┬─────────┘                                                   │
│           │ creates                                                     │
│           ▼                                                             │
│  ┌──────────────────┐   ┌──────────────────┐   ┌────────────────────┐  │
│  │  Session Manager │──▶│  JWT Validator   │──▶│  Pipeline Registry │  │
│  │  (sessions map)  │   │  (jsonwebtoken)  │   │  (templates)       │  │
│  └────────┬─────────┘   └──────────────────┘   └────────────────────┘  │
│           │ spawns                                                      │
│           ▼                                                             │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  SRT Listener (srt-tokio)                                         │  │
│  │  - Streamid-based multi-session routing                           │  │
│  │  - JWT validation per connection                                  │  │
│  │  - Timeout and error handling                                     │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  ┌──────────────────┐   ┌──────────────────┐   ┌────────────────────┐  │
│  │  Bounded Queues  │──▶│  Pipeline Runner │──▶│  Webhook Worker    │  │
│  │  (drop-oldest)   │   │  (analysis)      │   │  (retry + backoff) │  │
│  └──────────────────┘   └──────────────────┘   └────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

## Session Lifecycle

1. **Created**: Session created via API, waiting for SRT connection
2. **Connected**: SRT connection established, waiting for media
3. **Streaming**: Receiving and analyzing media
4. **Ended**: Session completed (disconnect, timeout, error, or deleted)

Sessions automatically expire when:
- `max_duration_seconds` is exceeded
- Connection times out (30s no data)
- Too many consecutive errors (10)
- Client disconnects
- Explicitly deleted via API

## Robustness Features

- **Bounded queues**: Audio (500ms) and video (5 frames) with drop-oldest policy
- **Connection timeout**: 30 seconds without data
- **Error tolerance**: Up to 10 consecutive errors before termination
- **Graceful shutdown**: SIGTERM/SIGINT handling
- **Auto-cleanup**: Expired sessions removed every 10 seconds
- **Webhook retry**: Exponential backoff with configurable max retries

## Metrics

The `/metrics` endpoint returns:
```json
{
  "sessions_created": 150,
  "sessions_ended": 148,
  "active_sessions": 2,
  "events_emitted": 5420,
  "bytes_received": 1073741824,
  "packets_received": 50000,
  "webhook_attempts": 500,
  "webhook_successes": 495,
  "webhook_failures": 5,
  "uptime_secs": 86400
}
```

## Development

```bash
# Build
cargo build -p remotemedia-ingest-srt

# Run tests
cargo test -p remotemedia-ingest-srt

# Run with debug logging
RUST_LOG=debug cargo run -p remotemedia-ingest-srt
```

## License

See repository root for license information.
