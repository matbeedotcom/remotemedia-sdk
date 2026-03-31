#!/usr/bin/env bash
# Build the embedded UI frontend and recompile the Rust crates that embed it.
#
# Usage:
#   ./scripts/build-ui.sh          # build frontend + remotemedia-ui crate
#   ./scripts/build-ui.sh --cli    # also rebuild the CLI with ui,webrtc features
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRONTEND_DIR="$REPO_ROOT/crates/ui/frontend"
CLI_DIR="$REPO_ROOT/examples/cli/remotemedia-cli"

# 1. Build frontend (Preact + Vite → dist/)
echo "==> Building frontend..."
cd "$FRONTEND_DIR"
npm run build

# 2. Force Rust to re-embed the new dist/ assets
echo "==> Rebuilding remotemedia-ui crate..."
cd "$REPO_ROOT"
cargo clean -p remotemedia-ui 2>/dev/null || true
cargo build -p remotemedia-ui

# 3. Optionally rebuild the CLI
if [[ "${1:-}" == "--cli" ]]; then
    echo "==> Rebuilding CLI (features: ui,webrtc)..."
    cd "$CLI_DIR"
    cargo build --features ui,webrtc
fi

echo "==> Done."
