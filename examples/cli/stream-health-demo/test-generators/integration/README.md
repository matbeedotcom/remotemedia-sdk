# Integration Tests

Tests the full streaming path with real-time audio delivery.

## What This Tests

Unlike the WAV unit tests, integration tests validate:

1. **Real-time streaming** - FFmpeg pushes audio at native framerate
2. **Named pipe input** - Audio flows through a FIFO (simulating ingest)
3. **Streaming mode** - Demo processes chunks as they arrive

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   FFmpeg    │────▶│ Named Pipe  │────▶│   Demo      │
│  (realtime) │     │   (FIFO)    │     │  --stream   │
└─────────────┘     └─────────────┘     └─────────────┘
                                              │
                                              ▼
                                        JSON events
```

## Prerequisites

```bash
# Ubuntu/Debian
apt install ffmpeg curl python3

# macOS
brew install ffmpeg
```

## Usage

```bash
# Run all integration tests
./run_integration_tests.sh

# Manual test
ffmpeg -re -i ../test_suite/fault_clipping.wav -f wav pipe:1 | \
    ../../remotemedia-demo -i - --stream --json
```

## Tests

| Test | Input | Expected |
|------|-------|----------|
| silence | fault_silence.wav | `alert.silence` events |
| clipping | fault_clipping.wav | `alert.clipping` events |
| low_volume | fault_low_volume.wav | `alert.low_volume` events |

## Future: Webhook Integration

The full wedge path with webhooks:

```
FFmpeg → RTMP/SRT → Ingest Server → Pipeline → Webhook Receiver
```

```bash
# Start webhook receiver
python webhook_receiver.py --port 8765 &

# Start ingest server with webhook config
./start_ingest.sh --webhook http://localhost:8765/webhook

# Push test stream
ffmpeg -re -i fault_clipping.wav -f flv rtmp://localhost:1935/live/test

# Check webhooks received
curl http://localhost:8765/events
```

## Timing-Based Tests

These tests CAN validate drift/jitter (unlike WAV tests):

```bash
# Create stream with artificial delay
ffmpeg -re -i clean.wav -f wav -ar 16000 pipe:1 | \
    (while read -r -n 4000 chunk; do
        sleep 0.05  # Add 50ms jitter
        echo -n "$chunk"
    done) | \
    ./remotemedia-demo -i - --stream --json
```

## Troubleshooting

### FFmpeg not found
```bash
apt install ffmpeg  # or brew install ffmpeg
```

### Permission denied on FIFO
```bash
rm -f /tmp/health_test_*.fifo
```

### Demo hangs
The demo may hang if FFmpeg doesn't properly close the pipe. Use Ctrl+C and the cleanup handler will terminate both processes.
