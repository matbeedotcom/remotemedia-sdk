#!/bin/bash
#
# Build script for Node.js native bindings with napi-webrtc feature
#
# This script builds the FFI library and copies it to all the locations
# where Node.js may look for it, avoiding stale .node file issues.
#
# Usage:
#   ./build-napi.sh           # Debug build
#   ./build-napi.sh --release # Release build
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Parse arguments
BUILD_TYPE="debug"
CARGO_FLAGS=""

if [[ "$1" == "--release" ]]; then
    BUILD_TYPE="release"
    CARGO_FLAGS="--release"
fi

echo "=========================================="
echo "Building Node.js native bindings"
echo "Build type: $BUILD_TYPE"
echo "=========================================="

# Source cargo environment if needed
if command -v cargo &> /dev/null; then
    : # cargo is available
else
    if [[ -f "$HOME/.cargo/env" ]]; then
        source "$HOME/.cargo/env"
    else
        echo "Error: cargo not found. Please install Rust first."
        exit 1
    fi
fi

# Build the library
echo ""
echo "Building remotemedia-ffi with napi-webrtc feature..."
cd "$SCRIPT_DIR"
cargo build --features napi-webrtc $CARGO_FLAGS

# Determine source file and target locations
if [[ "$BUILD_TYPE" == "release" ]]; then
    SOURCE_LIB="$PROJECT_ROOT/target/release/libremotemedia_ffi.so"
else
    SOURCE_LIB="$PROJECT_ROOT/target/debug/libremotemedia_ffi.so"
fi

if [[ ! -f "$SOURCE_LIB" ]]; then
    echo "Error: Build output not found at $SOURCE_LIB"
    exit 1
fi

echo ""
echo "Copying native library to all locations..."

# Target locations where Node.js looks for the native binding
TARGETS=(
    "$PROJECT_ROOT/target/$BUILD_TYPE/remotemedia_native.node"
    "$SCRIPT_DIR/nodejs/remotemedia-native.x86_64-unknown-linux-gnu.node"
    "$SCRIPT_DIR/tests/nodejs/remotemedia-native.linux-x64-gnu.node"
)

for TARGET in "${TARGETS[@]}"; do
    echo "  -> $TARGET"
    cp "$SOURCE_LIB" "$TARGET"
done

echo ""
echo "Build complete!"
echo ""
echo "Library size: $(du -h "$SOURCE_LIB" | cut -f1)"
echo "Modified: $(stat -c %y "$SOURCE_LIB" 2>/dev/null || stat -f "%Sm" "$SOURCE_LIB" 2>/dev/null)"
echo ""
echo "To test, run:"
echo "  cd $SCRIPT_DIR/tests/nodejs"
echo "  npx ts-node demo/server.ts"
