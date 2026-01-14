#!/bin/bash
# Run script for RemoteMedia Remote Execution Service (local development)

set -e

# Configuration
SERVICE_PORT=50051
METRICS_PORT=8080
LOG_LEVEL=DEBUG

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting RemoteMedia Remote Execution Service (Development)${NC}"
echo "=========================================================="

# Check if we're in the right directory
if [ ! -f "src/server.py" ]; then
    echo -e "${RED}Error: server.py not found. Are you in the remote_service directory?${NC}"
    exit 1
fi

# Check Python dependencies
echo -e "${YELLOW}Checking Python dependencies...${NC}"
if ! python -c "import grpc" 2>/dev/null; then
    echo -e "${RED}Error: gRPC not installed. Please install requirements:${NC}"
    echo "  pip install -r requirements.txt"
    exit 1
fi

# Generate gRPC code if needed
if [ ! -f "src/execution_pb2.py" ] || [ "protos/execution.proto" -nt "src/execution_pb2.py" ]; then
    echo -e "${YELLOW}Generating gRPC code...${NC}"
    python -m grpc_tools.protoc \
        --proto_path=protos \
        --python_out=src \
        --grpc_python_out=src \
        protos/*.proto
    echo -e "${GREEN}gRPC code generated${NC}"
fi

# Set environment variables
export GRPC_PORT=$SERVICE_PORT
export METRICS_PORT=$METRICS_PORT
export LOG_LEVEL=$LOG_LEVEL
export SANDBOX_ENABLED=false  # Disable sandboxing for development
export MAX_WORKERS=2

echo -e "${YELLOW}Environment:${NC}"
echo "  GRPC_PORT: $GRPC_PORT"
echo "  METRICS_PORT: $METRICS_PORT"
echo "  LOG_LEVEL: $LOG_LEVEL"
echo "  SANDBOX_ENABLED: $SANDBOX_ENABLED"
echo ""

# Create logs directory
mkdir -p logs

# Start the service
echo -e "${GREEN}Starting service...${NC}"
echo "Press Ctrl+C to stop"
echo ""

cd src
python server.py 