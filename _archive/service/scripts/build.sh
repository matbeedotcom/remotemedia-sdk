#!/bin/bash
# Build script for RemoteMedia Remote Execution Service

set -e

# Configuration
IMAGE_NAME="remotemedia/execution-service"
IMAGE_TAG="latest"
DOCKERFILE="Dockerfile"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Building RemoteMedia Remote Execution Service${NC}"
echo "=================================================="

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed or not in PATH${NC}"
    exit 1
fi

# Get the script directory and navigate to remote_service
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
cd "$SCRIPT_DIR/.."

# Check if we're in the right directory
if [ ! -f "$DOCKERFILE" ]; then
    echo -e "${RED}Error: Dockerfile not found. Are you in the remote_service directory?${NC}"
    exit 1
fi

# Generate gRPC code first
echo -e "${YELLOW}Generating gRPC code...${NC}"
if [ -d "protos" ]; then
    python -m grpc_tools.protoc \
        --proto_path=protos \
        --python_out=src \
        --grpc_python_out=src \
        protos/*.proto
    echo -e "${GREEN}gRPC code generated successfully${NC}"
else
    echo -e "${YELLOW}Warning: protos directory not found, skipping gRPC generation${NC}"
fi

# Build Docker image from parent directory for proper context
echo -e "${YELLOW}Building Docker image...${NC}"
cd ..
docker build \
    -t "${IMAGE_NAME}:${IMAGE_TAG}" \
    -f "remote_service/$DOCKERFILE" \
    .

if [ $? -eq 0 ]; then
    echo -e "${GREEN}Docker image built successfully: ${IMAGE_NAME}:${IMAGE_TAG}${NC}"
    
    # Show image info
    echo -e "${YELLOW}Image information:${NC}"
    docker images "${IMAGE_NAME}:${IMAGE_TAG}"
    
    # Optional: Tag as latest
    if [ "$IMAGE_TAG" != "latest" ]; then
        docker tag "${IMAGE_NAME}:${IMAGE_TAG}" "${IMAGE_NAME}:latest"
        echo -e "${GREEN}Tagged as latest${NC}"
    fi
    
else
    echo -e "${RED}Docker build failed${NC}"
    exit 1
fi

echo -e "${GREEN}Build completed successfully!${NC}"
echo ""
echo "To run the service:"
echo "  docker run -p 50051:50051 ${IMAGE_NAME}:${IMAGE_TAG}"
echo ""
echo "Or use docker-compose:"
echo "  docker-compose up" 