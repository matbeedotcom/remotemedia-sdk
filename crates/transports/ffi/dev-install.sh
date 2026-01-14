#!/bin/bash
# Development installation script for remotemedia-ffi
#
# This script builds the Rust extension module and creates a symlink
# in the python-client package for editable installs.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_CLIENT_DIR="$SCRIPT_DIR/../../../clients/python/remotemedia"
FFI_SO="$SCRIPT_DIR/python/remotemedia/runtime.abi3.so"
TARGET_LINK="$PYTHON_CLIENT_DIR/runtime.abi3.so"

echo "🔨 Building remotemedia-ffi with maturin..."
maturin develop --release --features extension-module

if [ ! -f "$FFI_SO" ]; then
    echo "❌ Error: Build failed - $FFI_SO not found"
    exit 1
fi

echo "✓ Build successful"

# Create symlink if it doesn't exist or is not a symlink
if [ -L "$TARGET_LINK" ]; then
    echo "✓ Symlink already exists: $TARGET_LINK"
elif [ -f "$TARGET_LINK" ]; then
    echo "⚠️  Regular file exists at $TARGET_LINK - removing it"
    rm "$TARGET_LINK"
    ln -sf "$FFI_SO" "$TARGET_LINK"
    echo "✓ Created symlink: $TARGET_LINK -> $FFI_SO"
else
    ln -sf "$FFI_SO" "$TARGET_LINK"
    echo "✓ Created symlink: $TARGET_LINK -> $FFI_SO"
fi

echo ""
echo "✅ Development setup complete!"
echo ""
echo "Test with:"
echo "  python -c 'from remotemedia.runtime import get_runtime_version; print(get_runtime_version())'"
