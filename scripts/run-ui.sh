#!/usr/bin/env bash
# Start the UI + WebRTC signaling server for local development.
#
# Usage:
#   ./scripts/run-ui.sh                              # defaults
#   ./scripts/run-ui.sh --manifest path/to/pipe.json # custom pipeline
#   UI_PORT=4000 SIGNAL_PORT=19000 ./scripts/run-ui.sh
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLI_DIR="$REPO_ROOT/examples/cli/remotemedia-cli"
DEFAULT_MANIFEST="$REPO_ROOT/crates/ui/e2e/fixtures/passthrough.json"

UI_PORT="${UI_PORT:-3001}"
SIGNAL_PORT="${SIGNAL_PORT:-18091}"
TRANSPORT_PORT="${TRANSPORT_PORT:-18080}"
MANIFEST="${MANIFEST:-$DEFAULT_MANIFEST}"

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --manifest) MANIFEST="$2"; shift 2 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

echo "==> Starting RemoteMedia UI"
echo "    UI:        http://0.0.0.0:${UI_PORT}"
echo "    Signaling: ws://0.0.0.0:${SIGNAL_PORT}/ws"
echo "    Transport: 0.0.0.0:${TRANSPORT_PORT}"
echo "    Manifest:  ${MANIFEST}"
echo ""

cd "$CLI_DIR"
exec cargo run --features ui,webrtc -- serve "$MANIFEST" \
    --transport webrtc \
    --port "$TRANSPORT_PORT" \
    --signal-port "$SIGNAL_PORT" \
    --signal-type websocket \
    --ui \
    --ui-port "$UI_PORT"
