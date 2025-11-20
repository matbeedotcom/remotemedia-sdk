#!/bin/bash
# Clean Python cache files and socket files that interfere with Docker builds
#
# Usage: ./scripts/clean-python-cache.sh

echo "Cleaning Python cache files and socket files..."

# Remove socket files (cause tar header type 83 errors)
find python-client -type s -delete 2>/dev/null && echo "✓ Removed socket files"

# Remove __pycache__ directories
find python-client -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null && echo "✓ Removed __pycache__"

# Remove .pytest_cache directories
find python-client -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null && echo "✓ Removed .pytest_cache"

# Remove .egg-info directories
find python-client -type d -name "*.egg-info" -exec rm -rf {} + 2>/dev/null && echo "✓ Removed .egg-info"

# Remove .pyc files
find python-client -name "*.pyc" -delete 2>/dev/null && echo "✓ Removed .pyc files"

echo ""
echo "✅ Python cache cleaned!"
echo ""
echo "Now you can run Docker builds without tar errors:"
echo "  docker build -f docker/Dockerfile.remotemedia-node -t remotemedia/python-node:py3.11 ."
echo "  cargo test --test test_docker_e2e_pipeline -- --nocapture"
