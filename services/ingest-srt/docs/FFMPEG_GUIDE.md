# FFmpeg Setup Guide for SRT Ingest Gateway

This guide covers FFmpeg installation, configuration, and common streaming scenarios for the SRT Ingest Gateway.

## Installation

### Ubuntu/Debian

```bash
# Install FFmpeg with SRT support
sudo apt update
sudo apt install ffmpeg

# Verify SRT protocol support
ffmpeg -protocols 2>&1 | grep srt
# Should output: srt
```

### macOS

```bash
# Using Homebrew
brew install ffmpeg

# Verify SRT support
ffmpeg -protocols 2>&1 | grep srt
```

### Windows

1. Download FFmpeg from https://ffmpeg.org/download.html
2. Extract to `C:\ffmpeg`
3. Add `C:\ffmpeg\bin` to your PATH
4. Open Command Prompt and verify:
   ```cmd
   ffmpeg -protocols 2>&1 | findstr srt
   ```

### Docker

```dockerfile
FROM jrottenberg/ffmpeg:4.4-alpine
# SRT support included
```

## Basic Streaming Commands

### Copy Mode (Lowest CPU, Best Quality)

Use when your source is already in a compatible format (H.264/AAC in MPEG-TS):

```bash
ffmpeg -re -i input.mp4 \
  -c copy \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

**Flags explained:**
- `-re` - Read input at native frame rate (real-time playback)
- `-c copy` - Copy streams without re-encoding
- `-f mpegts` - Output format (required for SRT)

### Transcode Mode (Most Compatible)

Use for any input source:

```bash
ffmpeg -re -i input.mp4 \
  -c:v libx264 -preset veryfast -tune zerolatency -g 60 -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

**Video flags:**
- `-c:v libx264` - H.264 encoder
- `-preset veryfast` - Encoding speed/quality tradeoff
- `-tune zerolatency` - Optimize for streaming
- `-g 60` - Keyframe interval (2 seconds at 30fps)
- `-b:v 1500k` - Video bitrate

**Audio flags:**
- `-c:a aac` - AAC encoder
- `-ar 48000` - 48kHz sample rate
- `-b:a 128k` - Audio bitrate

## Common Streaming Scenarios

### From Webcam (Linux)

```bash
# List devices
v4l2-ctl --list-devices

# Stream webcam with audio
ffmpeg -f v4l2 -i /dev/video0 \
  -f alsa -i default \
  -c:v libx264 -preset ultrafast -tune zerolatency -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### From Webcam (macOS)

```bash
# List devices
ffmpeg -f avfoundation -list_devices true -i ""

# Stream (device indices may vary)
ffmpeg -f avfoundation -i "0:0" \
  -c:v libx264 -preset ultrafast -tune zerolatency -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### From Webcam (Windows)

```bash
# List devices
ffmpeg -f dshow -list_devices true -i dummy

# Stream
ffmpeg -f dshow -i video="Your Webcam":audio="Your Microphone" \
  -c:v libx264 -preset ultrafast -tune zerolatency -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### From Screen Capture (Linux)

```bash
# Capture screen with audio
ffmpeg -f x11grab -s 1920x1080 -i :0.0 \
  -f pulse -i default \
  -c:v libx264 -preset ultrafast -tune zerolatency -b:v 3000k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### From RTMP Source

```bash
# Re-stream RTMP to SRT
ffmpeg -i rtmp://source-server/live/stream \
  -c copy \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### From HLS/DASH Source

```bash
# Stream from HLS playlist
ffmpeg -i "https://example.com/live/playlist.m3u8" \
  -c copy \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### Audio-Only Stream

```bash
# From file
ffmpeg -re -i audio.mp3 \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"

# From microphone (Linux)
ffmpeg -f alsa -i default \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### Test Pattern (No Input Required)

```bash
# Generate test video with audio tone
ffmpeg -f lavfi -i "testsrc2=size=1280x720:rate=30" \
  -f lavfi -i "sine=frequency=1000:sample_rate=48000" \
  -c:v libx264 -preset ultrafast -tune zerolatency -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -t 60 \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

## SRT Connection Parameters

### URL Format

```
srt://host:port?mode=caller&streamid=TOKEN&latency=LATENCY_MS
```

### Key Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `mode` | Connection mode (always `caller` for pushing) | - |
| `streamid` | JWT token for authentication | - |
| `latency` | Buffer latency in milliseconds | 120 |
| `maxbw` | Maximum bandwidth in bytes/sec | -1 (unlimited) |
| `pbkeylen` | Encryption key length (0, 16, 24, 32) | 0 (off) |

### Latency Tuning

```bash
# Lower latency (may increase packet loss)
ffmpeg ... "srt://localhost:9000?mode=caller&streamid=TOKEN&latency=50"

# Higher latency (more reliable over bad networks)
ffmpeg ... "srt://localhost:9000?mode=caller&streamid=TOKEN&latency=500"
```

## Bitrate Recommendations

### Audio Quality

| Use Case | Sample Rate | Bitrate |
|----------|-------------|---------|
| Speech/Podcast | 48000 Hz | 64-96 kbps |
| Music | 48000 Hz | 128-256 kbps |
| High Quality | 48000 Hz | 320 kbps |

### Video Quality

| Resolution | Frame Rate | Bitrate (Low Motion) | Bitrate (High Motion) |
|------------|------------|----------------------|-----------------------|
| 720p | 30 fps | 1.5-2.5 Mbps | 2.5-4 Mbps |
| 1080p | 30 fps | 3-4.5 Mbps | 4.5-6 Mbps |
| 1080p | 60 fps | 4.5-6 Mbps | 6-9 Mbps |
| 4K | 30 fps | 13-18 Mbps | 18-25 Mbps |

## Troubleshooting

### Connection Refused

```
Connection refused
```

**Solution:** Verify the gateway is running and the port is correct:
```bash
curl http://localhost:8080/health
```

### Invalid Streamid

```
Connection rejected: invalid streamid
```

**Solution:** Ensure you're using the exact streamid from the session creation response.

### Timeout During Connection

```
Connection timeout
```

**Solutions:**
- Check firewall allows UDP on port 9000
- Verify network connectivity
- Try increasing latency: `latency=500`

### High Packet Loss

```
SRT connection: retransmissions high
```

**Solutions:**
- Increase latency buffer
- Reduce bitrate
- Check network quality

### Audio/Video Sync Issues

**Solutions:**
- Add `-async 1` for audio sync
- Use `-copyts` to preserve timestamps
- Ensure constant frame rate with `-vsync cfr`

```bash
ffmpeg -re -i input.mp4 \
  -async 1 -vsync cfr \
  -c:v libx264 -preset veryfast -tune zerolatency -g 60 -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

### FFmpeg Not Finding SRT Protocol

```
Protocol 'srt' not on whitelist
```

**Solution:** Build FFmpeg with SRT support or install a package that includes it:
```bash
# Ubuntu/Debian
sudo apt install libsrt1-openssl ffmpeg
```

## Performance Tips

### Reduce CPU Usage

1. Use hardware encoding if available:
   ```bash
   # NVIDIA NVENC
   -c:v h264_nvenc -preset fast

   # Intel Quick Sync
   -c:v h264_qsv -preset veryfast

   # AMD VCE
   -c:v h264_amf -quality speed
   ```

2. Use faster presets:
   ```bash
   -preset ultrafast  # Fastest, lowest quality
   -preset superfast
   -preset veryfast   # Good balance (recommended)
   ```

3. Reduce resolution/framerate:
   ```bash
   -vf scale=1280:720 -r 24
   ```

### Reduce Latency

```bash
ffmpeg -fflags nobuffer -flags low_delay \
  -i input \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -x264-params "bframes=0:force-cfr=1" \
  -c:a aac \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=TOKEN&latency=50"
```

### Improve Reliability

```bash
# Add reconnect for HLS/HTTP sources
ffmpeg -reconnect 1 -reconnect_streamed 1 -reconnect_delay_max 5 \
  -i "https://source.com/stream.m3u8" \
  ...
```

## Scripted Streaming

### Bash Script with Retry

```bash
#!/bin/bash
SRT_URL="srt://localhost:9000?mode=caller&streamid=$STREAM_ID"
INPUT="input.mp4"

while true; do
  echo "Starting stream..."
  ffmpeg -re -i "$INPUT" \
    -c:v libx264 -preset veryfast -tune zerolatency -g 60 -b:v 1500k \
    -c:a aac -ar 48000 -b:a 128k \
    -f mpegts "$SRT_URL"

  echo "Stream ended, retrying in 5 seconds..."
  sleep 5
done
```

### Loop File Indefinitely

```bash
ffmpeg -stream_loop -1 -re -i input.mp4 \
  -c:v libx264 -preset veryfast -tune zerolatency -g 60 -b:v 1500k \
  -c:a aac -ar 48000 -b:a 128k \
  -f mpegts \
  "srt://localhost:9000?mode=caller&streamid=YOUR_STREAM_ID"
```

## See Also

- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
- [SRT Protocol](https://github.com/Haivision/srt)
- [SRT Cookbook](https://srtlab.github.io/srt-cookbook/)
