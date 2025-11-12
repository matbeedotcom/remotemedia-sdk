# RemoteMedia Python 3.9 Node - Single Stage Build
#
# Uses pytorch/pytorch base image with CUDA support
# Includes: Python 3.9, PyTorch, RemoteMedia SDK, iceoryx2 IPC, CUDA libraries
#
# Build:
#   docker build -f docker/base-images/python39.Dockerfile -t remotemedia/python39-node:latest .
#
# Test:
#   docker run --rm remotemedia/python39-node:latest python -c "import torch, remotemedia, iceoryx2; print('All OK')"

ARG PYTORCH_VERSION=1.13.1

# Use PyTorch base image with Python 3.9 (older PyTorch version for Python 3.9 compatibility)
FROM pytorch/pytorch:${PYTORCH_VERSION}-cuda11.6-cudnn8-runtime

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    gcc \
    g++ \
    git \
    libsndfile1 \
    libsndfile1-dev \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*

# Install remotemedia shared package (protobuf definitions)
COPY remotemedia /tmp/remotemedia-shared/remotemedia
COPY setup.py /tmp/remotemedia-shared/
COPY README.md /tmp/remotemedia-shared/

WORKDIR /tmp/remotemedia-shared
RUN pip install --no-cache-dir .

# Install remotemedia-client with dependencies
COPY python-client /tmp/remotemedia-client

WORKDIR /tmp/remotemedia-client

# Install base dependencies
RUN pip install --no-cache-dir -r requirements.txt

# Install ML dependencies if needed
RUN pip install --no-cache-dir -r requirements-ml.txt || true

# Install iceoryx2 for IPC
RUN pip install --no-cache-dir iceoryx2

# Install the package
RUN pip install --no-cache-dir --no-deps .

# Setup for RemoteMedia execution
RUN mkdir -p /tmp/iceoryx2 && chmod 777 /tmp/iceoryx2

WORKDIR /app

ENV PYTHONUNBUFFERED=1
ENV PYTHONDONTWRITEBYTECODE=1

STOPSIGNAL SIGTERM

# Keep container alive for docker exec
CMD ["tail", "-f", "/dev/null"]
