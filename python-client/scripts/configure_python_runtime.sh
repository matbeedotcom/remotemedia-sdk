#!/bin/bash
# Configure Python runtime for RemoteMedia SDK
#
# This script helps ensure the Rust runtime uses the correct Python interpreter
# for multiprocess execution, avoiding protobuf version mismatches.

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
CLIENT_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"
SDK_ROOT="$( cd "$CLIENT_DIR/.." && pwd )"

echo "🔧 Configuring Python Runtime for RemoteMedia SDK"
echo "=================================================="
echo ""

# Get the current Python interpreter
PYTHON_PATH=$(which python)
PYTHON_VERSION=$(python --version 2>&1)
PROTOBUF_VERSION=$(python -c "import google.protobuf; print(google.protobuf.__version__)" 2>&1 || echo "NOT INSTALLED")

echo "Current Python Configuration:"
echo "  Python: $PYTHON_PATH"
echo "  Version: $PYTHON_VERSION"
echo "  Protobuf: $PROTOBUF_VERSION"
echo ""

# Check if protobuf version matches requirements
REQUIRED_PROTOBUF="6.31.1"
if [ "$PROTOBUF_VERSION" != "$REQUIRED_PROTOBUF" ]; then
    echo "⚠️  WARNING: Protobuf version mismatch!"
    echo "   Required: $REQUIRED_PROTOBUF"
    echo "   Installed: $PROTOBUF_VERSION"
    echo ""
    echo "   Fix this by running:"
    echo "   pip install --upgrade protobuf==$REQUIRED_PROTOBUF"
    echo ""
fi

# Save Python path for reference
echo "$PYTHON_PATH" > "$CLIENT_DIR/.python-path"
echo "✅ Saved Python path to: $CLIENT_DIR/.python-path"
echo ""

# Generate protobuf files
echo "📦 Generating protobuf files..."
cd "$CLIENT_DIR"
python scripts/generate_protos.py
echo ""

# Verify protobuf imports (test directly without full remotemedia package to avoid ML dependencies)
echo "🔍 Verifying protobuf imports..."
cd "$CLIENT_DIR"
if python -c "import sys; sys.path.insert(0, 'remotemedia'); from protos import execution_pb2, common_pb2, streaming_pb2; print('✅ Protobuf imports working!')" 2>&1; then
    echo ""
else
    echo ""
    echo "❌ ERROR: Protobuf imports failed!"
    echo "   Try reinstalling the package:"
    echo "   cd $CLIENT_DIR"
    echo "   pip install -e ."
    exit 1
fi

echo "=================================================="
echo "✅ Configuration complete!"
echo ""
echo "To use this Python interpreter in Rust runtime,"
echo "set the PYTHON environment variable:"
echo ""
echo "  export PYTHON=$PYTHON_PATH"
echo ""
echo "Or configure it in your manifest's multiprocess config."

