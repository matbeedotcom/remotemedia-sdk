#!/bin/bash

# RemoteMedia TypeScript Type Generator
# This script generates TypeScript definitions by connecting to the gRPC service

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 RemoteMedia TypeScript Type Generator${NC}"
echo

# Default values
GRPC_HOST=${GRPC_HOST:-localhost}
GRPC_PORT=${GRPC_PORT:-50052}
OUTPUT_DIR=${OUTPUT_DIR:-./generated-types}

echo -e "${YELLOW}Configuration:${NC}"
echo "  gRPC Host: $GRPC_HOST"
echo "  gRPC Port: $GRPC_PORT"
echo "  Output Dir: $OUTPUT_DIR"
echo

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
    echo -e "${RED}❌ Node.js is not installed. Please install Node.js 14+ first.${NC}"
    exit 1
fi

# Navigate to script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo -e "${YELLOW}📦 Installing dependencies...${NC}"
    npm install
    echo
fi

# Check if gRPC service is running
echo -e "${YELLOW}🔍 Checking gRPC service connection...${NC}"
if ! nc -z "$GRPC_HOST" "$GRPC_PORT" 2>/dev/null; then
    echo -e "${RED}❌ Cannot connect to gRPC service at $GRPC_HOST:$GRPC_PORT${NC}"
    echo -e "${YELLOW}💡 Make sure the RemoteMedia service is running:${NC}"
    echo "   cd remote_service && docker-compose up"
    exit 1
fi

echo -e "${GREEN}✅ gRPC service is running${NC}"
echo

# Generate types
echo -e "${YELLOW}🔧 Generating TypeScript definitions...${NC}"
GRPC_HOST="$GRPC_HOST" GRPC_PORT="$GRPC_PORT" OUTPUT_DIR="$OUTPUT_DIR" node generate-typescript-types.js

echo
echo -e "${GREEN}🎉 TypeScript definitions generated successfully!${NC}"
echo -e "${BLUE}📁 Files created in: $OUTPUT_DIR${NC}"
echo
echo -e "${YELLOW}📋 Usage example:${NC}"
echo "  import { NodeType, RemoteExecutionClient } from '$OUTPUT_DIR';"
echo
echo -e "${YELLOW}📚 Available files:${NC}"
if [ -d "$OUTPUT_DIR" ]; then
    ls -la "$OUTPUT_DIR" | grep -E '\.(ts|d\.ts)$' | awk '{print "  📄 " $9}'
fi