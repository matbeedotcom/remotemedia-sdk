#!/usr/bin/env bash
# Build and run WebRTC server with gRPC signaling, multiprocessing, and Docker support
#
# Features enabled:
# - gRPC signaling (grpc-signaling)
# - Multiprocess Python nodes (multiprocess - default in runtime-core)
# - Docker executor support (docker - default in runtime-core)
# - Silero VAD (silero-vad - default in runtime-core)

set -e  # Exit on error

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default configuration
BUILD_MODE="${BUILD_MODE:-release}"
GRPC_ADDRESS="${GRPC_ADDRESS:-0.0.0.0:50051}"
MANIFEST="${MANIFEST:-./examples/tts.json}"
RUST_LOG="${RUST_LOG:-info,remotemedia=info}"
MAX_PEERS="${MAX_PEERS:-10}"
STUN_SERVERS="${STUN_SERVERS:-stun:stun.l.google.com:19302}"

# New configuration options (Phase 8 features)
PRESET="${WEBRTC_PRESET:-}"
VIDEO_CODEC="${WEBRTC_VIDEO_CODEC:-vp9}"
DATA_CHANNEL_MODE="${WEBRTC_DATA_CHANNEL_MODE:-reliable}"
TURN_SERVERS="${WEBRTC_TURN_SERVERS:-}"
TARGET_BITRATE="${WEBRTC_TARGET_BITRATE:-2000}"
MAX_RESOLUTION="${WEBRTC_MAX_RESOLUTION:-720p}"
VIDEO_FRAMERATE="${WEBRTC_VIDEO_FRAMERATE:-30}"
ICE_TIMEOUT="${WEBRTC_ICE_TIMEOUT:-30}"
ADAPTIVE_BITRATE="${WEBRTC_ADAPTIVE_BITRATE:-true}"
MAX_RECONNECT_RETRIES="${WEBRTC_MAX_RECONNECT_RETRIES:-5}"
RECONNECT_BACKOFF_INITIAL="${WEBRTC_RECONNECT_BACKOFF_INITIAL:-1000}"
RECONNECT_BACKOFF_MAX="${WEBRTC_RECONNECT_BACKOFF_MAX:-30000}"
RTCP_INTERVAL="${WEBRTC_RTCP_INTERVAL:-5000}"
ENABLE_METRICS_LOGGING="${WEBRTC_METRICS_LOGGING:-false}"
METRICS_INTERVAL="${WEBRTC_METRICS_INTERVAL:-10}"
JITTER_BUFFER_MS="${WEBRTC_JITTER_BUFFER_MS:-100}"

# Parse command line arguments
SKIP_BUILD=false
SHOW_HELP=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            BUILD_MODE="release"
            shift
            ;;
        --debug)
            BUILD_MODE="debug"
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --grpc-address)
            GRPC_ADDRESS="$2"
            shift 2
            ;;
        --manifest)
            MANIFEST="$2"
            shift 2
            ;;
        --max-peers)
            MAX_PEERS="$2"
            shift 2
            ;;
        --stun-servers)
            STUN_SERVERS="$2"
            shift 2
            ;;
        --log-level)
            RUST_LOG="$2"
            shift 2
            ;;
        # New Phase 8 options
        --preset)
            PRESET="$2"
            shift 2
            ;;
        --video-codec)
            VIDEO_CODEC="$2"
            shift 2
            ;;
        --data-channel-mode)
            DATA_CHANNEL_MODE="$2"
            shift 2
            ;;
        --turn-servers)
            TURN_SERVERS="$2"
            shift 2
            ;;
        --target-bitrate)
            TARGET_BITRATE="$2"
            shift 2
            ;;
        --max-resolution)
            MAX_RESOLUTION="$2"
            shift 2
            ;;
        --video-framerate)
            VIDEO_FRAMERATE="$2"
            shift 2
            ;;
        --ice-timeout)
            ICE_TIMEOUT="$2"
            shift 2
            ;;
        --adaptive-bitrate)
            ADAPTIVE_BITRATE="$2"
            shift 2
            ;;
        --max-reconnect-retries)
            MAX_RECONNECT_RETRIES="$2"
            shift 2
            ;;
        --reconnect-backoff-initial)
            RECONNECT_BACKOFF_INITIAL="$2"
            shift 2
            ;;
        --reconnect-backoff-max)
            RECONNECT_BACKOFF_MAX="$2"
            shift 2
            ;;
        --rtcp-interval)
            RTCP_INTERVAL="$2"
            shift 2
            ;;
        --enable-metrics-logging)
            ENABLE_METRICS_LOGGING=true
            shift
            ;;
        --metrics-interval)
            METRICS_INTERVAL="$2"
            shift 2
            ;;
        --jitter-buffer-ms)
            JITTER_BUFFER_MS="$2"
            shift 2
            ;;
        -h|--help)
            SHOW_HELP=true
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            SHOW_HELP=true
            shift
            ;;
    esac
done

# Show help if requested
if [ "$SHOW_HELP" = true ]; then
    cat << EOF
${CYAN}WebRTC Server with gRPC Signaling${NC}

Build and run the RemoteMedia WebRTC server with full feature support:
- gRPC bidirectional streaming signaling
- Multiprocess Python node execution (iceoryx2 IPC)
- Docker container executor
- Native Rust audio nodes (Silero VAD)

${YELLOW}Usage:${NC}
  ./run_grpc_server.sh [OPTIONS]

${YELLOW}Options:${NC}
  --release              Build in release mode (optimized, slower compile)
  --debug                Build in debug mode (default, faster compile)
  --skip-build           Skip the build step (use existing binary)
  --grpc-address ADDR    gRPC server address (default: 0.0.0.0:50051)
  --manifest PATH        Pipeline manifest path (default: ./examples/tts.json)
  --max-peers N          Maximum concurrent WebRTC peers (default: 10)
  --stun-servers URLS    STUN server URLs, comma-separated (default: stun:stun.l.google.com:19302)
  --log-level LEVEL      Rust log level (default: info,remotemedia=info)
  -h, --help             Show this help message

${YELLOW}Advanced Options (Phase 8):${NC}
  --preset PRESET        Config preset: low-latency, high-quality, mobile-network
  --video-codec CODEC    Video codec: vp8, vp9, h264 (default: vp9)
  --data-channel-mode M  Data channel mode: reliable, unreliable (default: reliable)
  --turn-servers URLS    TURN servers (format: turn:host:port:user:pass, comma-separated)
  --target-bitrate KBPS  Target bitrate in kbps (default: 2000)
  --max-resolution RES   Max video resolution: 480p, 720p, 1080p (default: 720p)
  --video-framerate FPS  Video framerate (default: 30)
  --ice-timeout SECS     ICE connection timeout in seconds (default: 30)
  --adaptive-bitrate B   Enable adaptive bitrate: true/false (default: true)
  --max-reconnect-retries N  Max reconnection attempts (default: 5)
  --reconnect-backoff-initial MS  Initial backoff delay in ms (default: 1000)
  --reconnect-backoff-max MS      Max backoff delay in ms (default: 30000)
  --rtcp-interval MS     RTCP report interval in ms (default: 5000)
  --jitter-buffer-ms MS  Jitter buffer size in ms (default: 100)
  --enable-metrics-logging  Enable quality metrics logging
  --metrics-interval SECS   Metrics logging interval (default: 10)

${YELLOW}Environment Variables:${NC}
  BUILD_MODE             Build mode: debug or release
  GRPC_ADDRESS           gRPC server address
  MANIFEST               Pipeline manifest file path
  RUST_LOG               Logging level configuration
  MAX_PEERS              Maximum concurrent peers
  STUN_SERVERS           STUN/TURN server configuration
  WEBRTC_PRESET          Config preset (low-latency, high-quality, mobile-network)
  WEBRTC_VIDEO_CODEC     Video codec (vp8, vp9, h264)
  WEBRTC_DATA_CHANNEL_MODE  Data channel mode (reliable, unreliable)
  WEBRTC_TURN_SERVERS    TURN servers
  WEBRTC_TARGET_BITRATE  Target bitrate in kbps
  WEBRTC_MAX_RESOLUTION  Max video resolution
  WEBRTC_VIDEO_FRAMERATE Video framerate
  WEBRTC_ICE_TIMEOUT     ICE timeout in seconds
  WEBRTC_ADAPTIVE_BITRATE Enable adaptive bitrate
  WEBRTC_MAX_RECONNECT_RETRIES  Max reconnection attempts
  WEBRTC_RECONNECT_BACKOFF_INITIAL  Initial backoff ms
  WEBRTC_RECONNECT_BACKOFF_MAX      Max backoff ms
  WEBRTC_RTCP_INTERVAL   RTCP interval in ms
  WEBRTC_JITTER_BUFFER_MS Jitter buffer size in ms
  WEBRTC_METRICS_LOGGING Enable metrics logging
  WEBRTC_METRICS_INTERVAL Metrics logging interval

${YELLOW}Examples:${NC}
  # Quick development build with default settings
  ./run_grpc_server.sh

  # Production release build with custom manifest
  ./run_grpc_server.sh --release --manifest ./my-pipeline.yaml

  # Skip rebuild and run with custom gRPC port
  ./run_grpc_server.sh --skip-build --grpc-address 0.0.0.0:9090

  # Debug logging for specific modules
  ./run_grpc_server.sh --log-level debug,remotemedia_webrtc=trace,remotemedia_runtime_core=debug

  # Low-latency preset for real-time applications
  ./run_grpc_server.sh --preset low-latency

  # High-quality preset with custom bitrate
  ./run_grpc_server.sh --preset high-quality --target-bitrate 6000 --max-resolution 1080p

  # Mobile network preset with TURN server
  ./run_grpc_server.sh --preset mobile-network --turn-servers "turn:turn.example.com:3478:user:password"

  # Full custom configuration for production
  ./run_grpc_server.sh --release \\
    --video-codec h264 \\
    --data-channel-mode unreliable \\
    --jitter-buffer-ms 75 \\
    --max-reconnect-retries 10 \\
    --enable-metrics-logging \\
    --metrics-interval 5

${YELLOW}Features Enabled:${NC}
  ✓ grpc-signaling       gRPC bidirectional streaming (instead of WebSocket)
  ✓ multiprocess         Process-isolated Python nodes with iceoryx2 IPC
  ✓ docker      Execute nodes in Docker containers
  ✓ silero-vad           Native Rust voice activity detection

${YELLOW}Dependencies:${NC}
  - Rust toolchain (1.87+)
  - Docker daemon (for docker feature)
  - iceoryx2 system dependencies (for multiprocess feature)

${YELLOW}Pipeline Manifest Format:${NC}
  The manifest file can be JSON or YAML. Example:
  
  {
    "nodes": [
      {
        "id": "docker_node",
        "node_type": "CustomDockerNode",
        "executor": "docker",
        "config": {
          "image": "my-node:latest"
        }
      }
    ],
    "edges": []
  }

${YELLOW}Connecting Clients:${NC}
  Once running, clients can connect via gRPC at: ${GRPC_ADDRESS}
  
  Example with Python client:
    from remotemedia.webrtc import WebRTCClient
    client = WebRTCClient(grpc_address="${GRPC_ADDRESS}")
    await client.connect()

EOF
    exit 0
fi

# Print configuration
echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}WebRTC Server Configuration${NC}"
echo -e "${CYAN}========================================${NC}"
echo -e "Build Mode:      ${GREEN}${BUILD_MODE}${NC}"
echo -e "gRPC Address:    ${GREEN}${GRPC_ADDRESS}${NC}"
echo -e "Manifest:        ${GREEN}${MANIFEST}${NC}"
echo -e "Max Peers:       ${GREEN}${MAX_PEERS}${NC}"
echo -e "STUN Servers:    ${GREEN}${STUN_SERVERS}${NC}"
echo -e "Log Level:       ${GREEN}${RUST_LOG}${NC}"
if [ -n "$PRESET" ]; then
echo -e "Preset:          ${GREEN}${PRESET}${NC}"
fi
echo -e "${CYAN}── Media Settings ──${NC}"
echo -e "Video Codec:     ${GREEN}${VIDEO_CODEC}${NC}"
echo -e "Max Resolution:  ${GREEN}${MAX_RESOLUTION}${NC}"
echo -e "Framerate:       ${GREEN}${VIDEO_FRAMERATE} fps${NC}"
echo -e "Target Bitrate:  ${GREEN}${TARGET_BITRATE} kbps${NC}"
echo -e "Adaptive:        ${GREEN}${ADAPTIVE_BITRATE}${NC}"
echo -e "${CYAN}── Connection Settings ──${NC}"
echo -e "Data Channel:    ${GREEN}${DATA_CHANNEL_MODE}${NC}"
echo -e "Jitter Buffer:   ${GREEN}${JITTER_BUFFER_MS} ms${NC}"
echo -e "RTCP Interval:   ${GREEN}${RTCP_INTERVAL} ms${NC}"
echo -e "ICE Timeout:     ${GREEN}${ICE_TIMEOUT} s${NC}"
if [ -n "$TURN_SERVERS" ]; then
echo -e "TURN Servers:    ${GREEN}configured${NC}"
fi
echo -e "${CYAN}── Reconnection Settings ──${NC}"
echo -e "Max Retries:     ${GREEN}${MAX_RECONNECT_RETRIES}${NC}"
echo -e "Backoff Initial: ${GREEN}${RECONNECT_BACKOFF_INITIAL} ms${NC}"
echo -e "Backoff Max:     ${GREEN}${RECONNECT_BACKOFF_MAX} ms${NC}"
if [ "$ENABLE_METRICS_LOGGING" = true ]; then
echo -e "${CYAN}── Metrics ──${NC}"
echo -e "Logging:         ${GREEN}enabled (${METRICS_INTERVAL}s interval)${NC}"
fi
echo -e "${CYAN}========================================${NC}"
echo ""

# Navigate to the webrtc transport directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Build the server
if [ "$SKIP_BUILD" = false ]; then
    echo -e "${CYAN}Building WebRTC server...${NC}"
    echo -e "${YELLOW}Features: grpc-signaling + runtime defaults (multiprocess, docker, silero-vad)${NC}"
    echo ""
    
    BUILD_CMD="cargo build --bin webrtc_server --features grpc-signaling"
    
    if [ "$BUILD_MODE" = "release" ]; then
        BUILD_CMD="$BUILD_CMD --release"
        echo -e "${YELLOW}⚙️  Release build (optimized, takes longer)...${NC}"
    else
        echo -e "${YELLOW}⚙️  Debug build (faster compilation)...${NC}"
    fi
    
    if ! eval "$BUILD_CMD"; then
        echo -e "${RED}❌ Build failed!${NC}"
        exit 1
    fi
    
    echo ""
    echo -e "${GREEN}✅ Build succeeded!${NC}"
    echo ""
else
    echo -e "${YELLOW}⏭️  Skipping build step${NC}"
    echo ""

fi

# Check if manifest file exists
if [ ! -f "$MANIFEST" ]; then
    echo -e "${YELLOW}⚠️  Warning: Manifest file not found: ${MANIFEST}${NC}"
    echo -e "${YELLOW}   Server will start but may fail if manifest is required${NC}"
    echo ""
fi

# Export environment variables for the server
export RUST_LOG
export WEBRTC_ENABLE_GRPC_SIGNALING=true
export GRPC_SIGNALING_ADDRESS="$GRPC_ADDRESS"
export WEBRTC_PIPELINE_MANIFEST="$MANIFEST"
export WEBRTC_MAX_PEERS="$MAX_PEERS"
export WEBRTC_STUN_SERVERS="$STUN_SERVERS"

# Determine binary path
if [ "$BUILD_MODE" = "release" ]; then
    BINARY="../../target/release/webrtc_server"
else
    BINARY="../../target/debug/webrtc_server"
fi

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo -e "${RED}❌ Binary not found: ${BINARY}${NC}"
    echo -e "${RED}   Try running without --skip-build${NC}"
    exit 1
fi

# Print startup information
echo -e "${GREEN}🚀 Starting WebRTC Server with gRPC Signaling${NC}"
echo -e "${CYAN}────────────────────────────────────────${NC}"
echo -e "gRPC Endpoint:   ${GREEN}${GRPC_ADDRESS}${NC}"
echo -e "Pipeline:        ${GREEN}${MANIFEST}${NC}"
echo -e "Binary:          ${GREEN}${BINARY}${NC}"
echo -e "${CYAN}────────────────────────────────────────${NC}"
echo -e "${YELLOW}Press Ctrl+C to stop${NC}"
echo -e "${CYAN}────────────────────────────────────────${NC}"
echo ""

# Build command-line arguments array
CMD_ARGS=(
    --mode grpc
    --grpc-address "$GRPC_ADDRESS"
    --manifest "$MANIFEST"
    --max-peers "$MAX_PEERS"
    --stun-servers "$STUN_SERVERS"
    --video-codec "$VIDEO_CODEC"
    --data-channel-mode "$DATA_CHANNEL_MODE"
    --target-bitrate-kbps "$TARGET_BITRATE"
    --max-resolution "$MAX_RESOLUTION"
    --video-framerate-fps "$VIDEO_FRAMERATE"
    --ice-timeout-secs "$ICE_TIMEOUT"
    --adaptive-bitrate "$ADAPTIVE_BITRATE"
    --max-reconnect-retries "$MAX_RECONNECT_RETRIES"
    --reconnect-backoff-initial-ms "$RECONNECT_BACKOFF_INITIAL"
    --reconnect-backoff-max-ms "$RECONNECT_BACKOFF_MAX"
    --rtcp-interval-ms "$RTCP_INTERVAL"
    --jitter-buffer-ms "$JITTER_BUFFER_MS"
    --metrics-interval-secs "$METRICS_INTERVAL"
)

# Add optional arguments
if [ -n "$PRESET" ]; then
    CMD_ARGS+=(--preset "$PRESET")
fi

if [ -n "$TURN_SERVERS" ]; then
    CMD_ARGS+=(--turn-servers "$TURN_SERVERS")
fi

if [ "$ENABLE_METRICS_LOGGING" = true ]; then
    CMD_ARGS+=(--enable-metrics-logging)
fi

# Run the server with command-line arguments
exec "$BINARY" "${CMD_ARGS[@]}"

