#!/bin/bash
# Test script for UID configuration

echo "Testing UID/GID Configuration for RemoteMedia Remote Service"
echo "==========================================================="

# Get current user's UID/GID
HOST_UID=$(id -u)
HOST_GID=$(id -g)
echo "Host user UID: $HOST_UID"
echo "Host user GID: $HOST_GID"

# Set environment variables
export USER_UID=$HOST_UID
export USER_GID=$HOST_GID

echo "Building container with UID=$USER_UID, GID=$USER_GID..."

# Build with no cache to ensure UID/GID are applied
docker compose build --no-cache

echo "Testing container user ID..."
docker compose up -d
sleep 5

# Check container user ID
CONTAINER_UID=$(docker exec remotemedia-service id -u)
CONTAINER_GID=$(docker exec remotemedia-service id -g)

echo "Container user UID: $CONTAINER_UID"
echo "Container user GID: $CONTAINER_GID"

if [ "$HOST_UID" = "$CONTAINER_UID" ] && [ "$HOST_GID" = "$CONTAINER_GID" ]; then
    echo "✅ SUCCESS: Container user matches host user!"
else
    echo "❌ FAILED: Container user does not match host user"
fi

# Test file permissions with host directories
echo "Testing file permissions..."
docker exec remotemedia-service touch /home/remotemedia/.cache/huggingface/test_file
ls -la remote_service/cache/huggingface/test_file 2>/dev/null && echo "✅ File created with correct permissions" || echo "❌ File permission test failed"

echo "Cleaning up..."
docker compose down