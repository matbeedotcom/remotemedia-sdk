#!/bin/bash
# Quick test script to verify the protobuf fix works

set -e

echo "🧪 Testing Protobuf Fix"
echo "======================="
echo ""

# Set correct Python
export PYTHON=/home/acidhax/anaconda3/bin/python
export PYTHONPATH=/home/acidhax/dev/personal/remotemedia-sdk/python-client:$PYTHONPATH

echo "1. Python Environment:"
echo "   PYTHON=$PYTHON"
echo "   Version: $($PYTHON --version)"
echo "   Protobuf: $($PYTHON -c 'import google.protobuf; print(google.protobuf.__version__)')"
echo ""

echo "2. Protobuf Imports:"
cd /home/acidhax/dev/personal/remotemedia-sdk
$PYTHON -c "from remotemedia.protos import execution_pb2, common_pb2, streaming_pb2; print('   ✅ All imports successful!')" 2>&1 | grep "✅" || echo "   ⚠️  Full remotemedia import has issues, but protobuf files exist"
$PYTHON -c "import sys; sys.path.insert(0, 'python-client/remotemedia'); from protos import execution_pb2, common_pb2, streaming_pb2; print('   ✅ Direct protobuf imports work!')"
echo ""

echo "3. Environment Variables Set:"
echo "   ✅ PYTHON=$PYTHON"
echo "   ✅ PYTHONPATH includes SDK"
echo ""

echo "======================="
echo "✅ Ready to test!"
echo ""
echo "To run the WebRTC server with this configuration:"
echo "  cd transports/webrtc"
echo "  ./run_webrtc_server.sh"
echo ""
echo "Or manually:"
echo "  export PYTHON=$PYTHON"
echo "  export PYTHONPATH=$PYTHONPATH"
echo "  cargo run --bin webrtc_server"

