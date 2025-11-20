# Base Docker image for Python 3.10 nodes with iceoryx2 support
# Spec 009: Docker-based node execution
#
# This image provides:
# - Python 3.10 runtime
# - iceoryx2 Python bindings for zero-copy IPC
# - Common audio processing system libraries
# - Non-root user for security

# ============================================================================
# Stage 1: Builder - Install dependencies and iceoryx2
# ============================================================================
FROM python:3.10-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    gcc \
    g++ \
    git \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Create virtual environment
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install iceoryx2 Python bindings
RUN pip install --no-cache-dir iceoryx2

# ============================================================================
# Stage 2: Runtime - Minimal runtime environment
# ============================================================================
FROM python:3.10-slim as runtime

# Install runtime system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Audio libraries (runtime only, no -dev packages)
    libsndfile1 \
    # Cleanup
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Create non-root user (security best practice - FR-014)
RUN useradd -m -u 1000 remotemedia && \
    mkdir -p /tmp/iceoryx2 && \
    chown remotemedia:remotemedia /tmp/iceoryx2

USER remotemedia
WORKDIR /home/remotemedia/app

# Set Python environment
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1

# Ensure proper signal handling for graceful shutdown
STOPSIGNAL SIGTERM

# Default command (will be overridden by node runner)
CMD ["python", "--version"]
