# SRT Ingest Gateway Troubleshooting Guide

This guide helps diagnose and resolve common issues with the SRT Ingest Gateway.

## Quick Diagnostics

### Gateway Health Check

```bash
# Check if gateway is running
curl http://localhost:8080/health
# Expected: OK

# Check current metrics
curl http://localhost:8080/metrics | jq
```

### Session Status Check

```bash
# Check session status
curl http://localhost:8080/api/ingest/sessions/sess_YOUR_ID | jq
```

## Connection Issues

### "Connection Refused" when creating session

**Symptoms:**
```
curl: (7) Failed to connect to localhost port 8080: Connection refused
```

**Causes & Solutions:**

1. **Gateway not running**
   ```bash
   # Start the gateway
   cargo run -p remotemedia-ingest-srt --release
   ```

2. **Wrong port**
   ```bash
   # Check configured port
   echo $INGEST_HTTP_PORT  # Default: 8080
   ```

3. **Firewall blocking**
   ```bash
   # Linux: Allow port
   sudo ufw allow 8080/tcp
   ```

### "Connection Refused" when streaming with FFmpeg

**Symptoms:**
```
Connection to srt://localhost:9000 failed: Connection refused
```

**Causes & Solutions:**

1. **SRT listener not started**
   - Ensure the gateway started successfully
   - Check logs for SRT listener initialization

2. **Wrong port**
   ```bash
   echo $INGEST_SRT_PORT  # Default: 9000
   ```

3. **Firewall blocking UDP**
   ```bash
   # Linux: Allow UDP port
   sudo ufw allow 9000/udp
   ```

### "Invalid Streamid" rejection

**Symptoms:**
```
SRT connection rejected: invalid or expired streamid
```

**Causes & Solutions:**

1. **Expired JWT token**
   - Create a new session - tokens expire based on `JWT_TTL` (default 15 min)

2. **Malformed streamid**
   - Use the exact URL from session creation response
   - Don't modify the streamid parameter

3. **Session already ended**
   - Create a new session if the previous one timed out

### Timeout during SRT connection

**Symptoms:**
```
SRT: Connection timeout
```

**Causes & Solutions:**

1. **Network latency**
   ```bash
   # Increase latency buffer
   ffmpeg ... "srt://host:9000?...&latency=500"
   ```

2. **Firewall dropping packets**
   - SRT uses UDP - ensure UDP is allowed
   - Check if NAT/router is blocking

3. **Session not created**
   - Verify session exists before streaming

## Streaming Issues

### No events received via SSE

**Symptoms:**
- Connected to `/events` endpoint but no data coming through

**Causes & Solutions:**

1. **No media being pushed**
   ```bash
   # Verify FFmpeg is running
   ps aux | grep ffmpeg
   ```

2. **Wrong session ID**
   - Double-check the session ID in the events URL

3. **Media not triggering alerts**
   - Send problematic audio (silence, clipping) to test
   - Check if pipeline is correctly configured

4. **Browser/client closing connection**
   - Use `curl -N` for persistent SSE:
     ```bash
     curl -N http://localhost:8080/api/ingest/sessions/sess_XXX/events
     ```

### Events received but no webhooks

**Symptoms:**
- SSE events work but webhook endpoint receives nothing

**Causes & Solutions:**

1. **Webhook URL not reachable**
   ```bash
   # Test webhook endpoint directly
   curl -X POST https://your-webhook-url.com/webhook \
     -H "Content-Type: application/json" \
     -d '{"test": true}'
   ```

2. **HTTPS certificate issues**
   - Webhook URL must have valid SSL certificate
   - Self-signed certs may fail

3. **Webhook timing out**
   - Webhook must respond within 10 seconds (configurable)
   - Check webhook server logs

4. **Check metrics for webhook failures**
   ```bash
   curl http://localhost:8080/metrics | jq '.webhook_failures'
   ```

### High latency in events

**Symptoms:**
- Events arrive seconds after the issue occurred

**Causes & Solutions:**

1. **SRT latency buffer too high**
   ```bash
   # Lower latency (may increase packet loss)
   ffmpeg ... "srt://...?latency=50"
   ```

2. **Webhook backlog**
   - Check if webhooks are timing out
   - Monitor `webhook_attempts` vs `webhook_successes` in metrics

3. **Pipeline processing delay**
   - Some analysis (like silence detection) requires buffering

### Audio analysis not detecting issues

**Symptoms:**
- Known audio problems not triggering alerts

**Causes & Solutions:**

1. **Thresholds too strict/lenient**
   - Check pipeline configuration for threshold values
   - Silence threshold: `-40 dB` typical

2. **Wrong sample rate**
   - Ensure FFmpeg outputs 48kHz or matching rate:
     ```bash
     ffmpeg ... -ar 48000 ...
     ```

3. **Audio stream not present**
   - Verify input has audio track:
     ```bash
     ffprobe input.mp4
     ```

## Session Issues

### Session ends unexpectedly

**Symptoms:**
- Stream stops without client disconnecting

**Causes & Solutions:**

1. **Max duration reached**
   - Check `max_duration_seconds` in session config
   - Default is 3600 seconds (1 hour)

2. **Connection timeout (30 seconds without data)**
   - Ensure continuous data flow
   - Check FFmpeg isn't paused

3. **Too many errors (10 consecutive)**
   - Check FFmpeg output for encoding errors
   - Verify source media is valid

4. **Check session end reason**
   ```bash
   curl http://localhost:8080/api/ingest/sessions/sess_XXX | jq '.end_reason'
   ```

### "Maximum sessions reached" error

**Symptoms:**
```json
{"error": "Maximum sessions reached"}
```

**Causes & Solutions:**

1. **Too many concurrent sessions**
   ```bash
   # Increase limit
   INGEST_MAX_SESSIONS=200 cargo run -p remotemedia-ingest-srt
   ```

2. **Zombie sessions**
   - Sessions should auto-cleanup, but verify:
     ```bash
     curl http://localhost:8080/metrics | jq '.active_sessions'
     ```

3. **Delete unused sessions**
   ```bash
   curl -X DELETE http://localhost:8080/api/ingest/sessions/sess_XXX
   ```

### Session status shows "created" but never "connected"

**Symptoms:**
- Session created but FFmpeg appears to be streaming

**Causes & Solutions:**

1. **FFmpeg not actually streaming**
   ```bash
   # Verify FFmpeg process
   ps aux | grep ffmpeg
   ```

2. **Connecting to wrong port/host**
   - Use exact URL from session response
   - Check `INGEST_PUBLIC_HOST` configuration

3. **Streamid encoding issues**
   - Special characters in URL may need encoding
   - Use quotes around the SRT URL in FFmpeg

## Performance Issues

### High CPU usage

**Symptoms:**
- Gateway consuming excessive CPU

**Causes & Solutions:**

1. **Too many concurrent sessions**
   - Reduce `MAX_SESSIONS`
   - Scale horizontally

2. **Video processing enabled**
   - Disable video if only audio analysis needed:
     ```json
     {"video_enabled": false}
     ```

3. **Check for busy-loop issues**
   ```bash
   # Monitor CPU per thread
   top -H -p $(pgrep remotemedia)
   ```

### High memory usage

**Symptoms:**
- Memory growing over time

**Causes & Solutions:**

1. **Queue overflow**
   - Check queue configuration
   - Monitor dropped samples in logs

2. **Session leak**
   - Sessions should auto-cleanup
   - Manually delete old sessions

3. **Check per-session memory**
   ```bash
   curl http://localhost:8080/metrics | jq
   ```

### Dropped samples/frames

**Symptoms:**
- Logs show "Audio queue overflow" or "Video queue overflow"

**Causes & Solutions:**

1. **Pipeline too slow**
   - Use simpler analysis pipeline
   - Disable video processing if not needed

2. **System overloaded**
   - Reduce concurrent sessions
   - Increase queue sizes (may increase latency)

3. **Check queue stats in logs**
   - Monitor `dropped_samples` and `dropped_frames`

## Logging and Debugging

### Enable debug logging

```bash
RUST_LOG=debug cargo run -p remotemedia-ingest-srt
```

### Enable trace logging (very verbose)

```bash
RUST_LOG=trace cargo run -p remotemedia-ingest-srt
```

### Filter logs by module

```bash
# Only SRT listener logs
RUST_LOG=remotemedia_ingest_srt::listener=debug cargo run -p remotemedia-ingest-srt

# Only session logs
RUST_LOG=remotemedia_ingest_srt::session=debug cargo run -p remotemedia-ingest-srt

# Only webhook logs
RUST_LOG=remotemedia_ingest_srt::webhook=debug cargo run -p remotemedia-ingest-srt
```

### FFmpeg verbose logging

```bash
ffmpeg -loglevel debug -i input.mp4 ...
```

## Network Diagnostics

### Test UDP connectivity

```bash
# Server side
nc -u -l 9000

# Client side
echo "test" | nc -u localhost 9000
```

### Check if port is in use

```bash
# Linux
ss -tuln | grep 9000

# macOS
lsof -i :9000
```

### Monitor network traffic

```bash
# Capture SRT traffic
sudo tcpdump -i any port 9000 -n

# With Wireshark (has SRT dissector)
wireshark -i any -f "port 9000"
```

## Recovery Procedures

### Restart gateway gracefully

```bash
# Send SIGTERM for graceful shutdown
kill -TERM $(pgrep remotemedia-ingest)

# Wait for cleanup
sleep 5

# Restart
cargo run -p remotemedia-ingest-srt --release
```

### Clear all sessions

```bash
# Get all active sessions (requires custom endpoint or DB access)
# Then delete each:
curl -X DELETE http://localhost:8080/api/ingest/sessions/sess_XXX
```

### Reset metrics

Metrics reset on gateway restart. There is no runtime reset endpoint.

## Common Error Messages

| Error | Meaning | Solution |
|-------|---------|----------|
| `Connection refused` | Gateway not running or wrong port | Start gateway, check ports |
| `Invalid streamid` | JWT expired or malformed | Create new session |
| `Session not found` | Session expired or deleted | Create new session |
| `Maximum sessions reached` | Concurrent limit hit | Increase limit or delete old sessions |
| `Webhook delivery failed` | Webhook URL unreachable | Check webhook server |
| `Connection timeout` | No data for 30 seconds | Check FFmpeg, increase latency |
| `Queue overflow` | Processing too slow | Reduce load, increase queue size |

## Getting Help

If issues persist:

1. Collect logs with `RUST_LOG=debug`
2. Gather metrics from `/metrics` endpoint
3. Note the session ID and error messages
4. Check the GitHub issues for similar problems
5. Open a new issue with all collected information
