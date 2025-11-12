#!/bin/bash
# Build all standard base images for Docker executor
# Spec 009: Docker-based node execution

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_IMAGES_DIR="$(dirname "$SCRIPT_DIR")/base-images"

echo "Building standard base images for RemoteMedia Docker executor..."

# Python 3.9
echo ""
echo "=== Building Python 3.9 base image ==="
docker build \
    -t remotemedia/python-node:3.9 \
    -t remotemedia/python-node:3.9-latest \
    -f "$BASE_IMAGES_DIR/python39.Dockerfile" \
    "$BASE_IMAGES_DIR"

# Python 3.10
echo ""
echo "=== Building Python 3.10 base image ==="
docker build \
    -t remotemedia/python-node:3.10 \
    -t remotemedia/python-node:3.10-latest \
    -t remotemedia/python-node:latest \
    -f "$BASE_IMAGES_DIR/python310.Dockerfile" \
    "$BASE_IMAGES_DIR"

# Python 3.11
echo ""
echo "=== Building Python 3.11 base image ==="
docker build \
    -t remotemedia/python-node:3.11 \
    -t remotemedia/python-node:3.11-latest \
    -f "$BASE_IMAGES_DIR/python311.Dockerfile" \
    "$BASE_IMAGES_DIR"

echo ""
echo "✓ All base images built successfully!"
echo ""
echo "Available images:"
docker images | grep remotemedia/python-node
