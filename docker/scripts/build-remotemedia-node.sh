#!/bin/bash
# Build RemoteMedia Docker node image with full Python package support
#
# This script pre-builds a Docker image that includes:
# - Python 3.10 runtime
# - iceoryx2 for IPC
# - remotemedia package from local source
# - Common dependencies
#
# Usage:
#   ./build-remotemedia-node.sh [python_version]
#
# Examples:
#   ./build-remotemedia-node.sh 3.10
#   ./build-remotemedia-node.sh 3.11

set -e

# Configuration
PYTHON_VERSION="${1:-3.10}"
IMAGE_NAME="remotemedia/python-node"
IMAGE_TAG="${IMAGE_NAME}:py${PYTHON_VERSION}"

echo "============================================"
echo "Building RemoteMedia Docker Node Image"
echo "============================================"
echo "Python version: ${PYTHON_VERSION}"
echo "Image tag: ${IMAGE_TAG}"
echo ""

# Navigate to workspace root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${WORKSPACE_ROOT}"

echo "Workspace root: ${WORKSPACE_ROOT}"
echo ""

# Verify python-client exists
if [ ! -d "python-client/remotemedia" ]; then
    echo "ERROR: python-client/remotemedia directory not found"
    echo "Expected at: ${WORKSPACE_ROOT}/python-client/remotemedia"
    exit 1
fi

echo "✓ Found python-client directory"
echo ""

# Create Dockerfile
DOCKERFILE=$(cat <<EOF
# ===========================================================================
# Stage 1: Builder - Install dependencies with build tools
# ===========================================================================
FROM python:${PYTHON_VERSION}-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \\
    build-essential \\
    gcc \\
    g++ \\
    git \\
    && rm -rf /var/lib/apt/lists/*

# Create virtual environment
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Copy remotemedia package source
COPY python-client/remotemedia /tmp/remotemedia-src/remotemedia
COPY python-client/setup.py /tmp/remotemedia-src/
COPY python-client/README.md /tmp/remotemedia-src/
COPY python-client/requirements.txt /tmp/remotemedia-src/

# Install remotemedia package
RUN cd /tmp/remotemedia-src && pip install --no-cache-dir .

# Install iceoryx2 (required for IPC)
RUN pip install --no-cache-dir iceoryx2

# ===========================================================================
# Stage 2: Runtime - Minimal runtime environment
# ===========================================================================
FROM python:${PYTHON_VERSION}-slim as runtime

# Install only runtime system dependencies (no build tools)
RUN apt-get update && apt-get install -y --no-install-recommends \\
    libsndfile1 \\
    ffmpeg \\
    && rm -rf /var/lib/apt/lists/*

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Create non-root user
RUN useradd -m -u 1000 remotemedia && \\
    mkdir -p /tmp/iceoryx2 && \\
    chown remotemedia:remotemedia /tmp/iceoryx2

# Create diagnostic script
RUN cat > /tmp/test_remotemedia.py << 'EOFDIAGO'
#!/usr/bin/env python3
import sys
import os

print("=== RemoteMedia Docker Container Diagnostics ===")
print(f"Python: {sys.version}")
print()

# Test imports
print("Testing imports...")
try:
    import remotemedia
    print(f"✓ remotemedia: {remotemedia.__version__ if hasattr(remotemedia, '__version__') else 'imported'}")
    print(f"  Location: {remotemedia.__file__}")
except ImportError as e:
    print(f"✗ remotemedia: FAILED - {e}")

try:
    import iceoryx2
    print(f"✓ iceoryx2: imported")
    print(f"  Location: {iceoryx2.__file__}")
except ImportError as e:
    print(f"✗ iceoryx2: FAILED - {e}")

try:
    from remotemedia.core.multiprocessing.runner import main as runner_main
    print("✓ remotemedia.core.multiprocessing.runner: available")
except ImportError as e:
    print(f"✗ runner: FAILED - {e}")

print()
print("Filesystem check...")
print(f"  /tmp/iceoryx2: exists={os.path.exists('/tmp/iceoryx2')}, writable={os.access('/tmp/iceoryx2', os.W_OK) if os.path.exists('/tmp/iceoryx2') else False}")
print(f"  /dev/shm: exists={os.path.exists('/dev/shm')}")
print()
print("Container ready for node execution!")
EOFDIAGO

RUN chmod +x /tmp/test_remotemedia.py

USER remotemedia
WORKDIR /home/remotemedia/app

ENV PYTHONUNBUFFERED=1
ENV PYTHONDONTWRITEBYTECODE=1

STOPSIGNAL SIGTERM

# Default: run diagnostic script
CMD ["python", "/tmp/test_remotemedia.py"]
EOF
)

echo "Creating Dockerfile..."
echo "${DOCKERFILE}" > /tmp/remotemedia-node.Dockerfile

echo "✓ Dockerfile created"
echo ""

# Build image
echo "Building Docker image (this may take a few minutes)..."
echo ""

docker build \\
    -f /tmp/remotemedia-node.Dockerfile \\
    -t "${IMAGE_TAG}" \\
    --progress=plain \\
    .

BUILD_EXIT_CODE=$?

if [ ${BUILD_EXIT_CODE} -eq 0 ]; then
    echo ""
    echo "============================================"
    echo "✓ Build successful!"
    echo "============================================"
    echo "Image: ${IMAGE_TAG}"
    echo ""
    echo "Test the image:"
    echo "  docker run --rm ${IMAGE_TAG}"
    echo ""
    echo "Run with iceoryx2 mounts:"
    echo "  docker run --rm \\"
    echo "    -v /tmp/iceoryx2:/tmp/iceoryx2 \\"
    echo "    -v /dev/shm:/dev/shm \\"
    echo "    --shm-size=2g \\"
    echo "    ${IMAGE_TAG}"
    echo ""
    echo "Interactive shell:"
    echo "  docker run --rm -it ${IMAGE_TAG} /bin/bash"
    echo ""
else
    echo ""
    echo "============================================"
    echo "✗ Build failed (exit code: ${BUILD_EXIT_CODE})"
    echo "============================================"
    exit ${BUILD_EXIT_CODE}
fi
