# RemoteMedia Remote Execution Service
# Multi-stage build for security and efficiency

# Build arguments for user configuration
ARG USER_UID=996
ARG USER_GID=996

# Build stage - compile gRPC and dependencies
FROM ubuntu:22.04 as builder

# Build arguments
ARG USER_UID=996
ARG USER_GID=996

# Set non-interactive mode to prevent prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    python3 \
    python3-pip \
    python3-dev \
    && rm -rf /var/lib/apt/lists/*

# Create build user
RUN groupadd -g ${USER_GID} builduser && \
    useradd -l -u ${USER_UID} -g ${USER_GID} -m -d /home/builduser builduser

# Switch to build user
USER builduser
WORKDIR /home/builduser

# Copy requirements and install Python dependencies
COPY --chown=builduser:builduser remote_service/requirements.txt .
RUN pip3 install --no-cache-dir --user -r requirements.txt

# Copy remotemedia package source for installation with proper ownership
COPY --chown=builduser:builduser remotemedia /home/builduser/remotemedia
COPY --chown=builduser:builduser setup.py /home/builduser/setup.py
COPY --chown=builduser:builduser pyproject.toml /home/builduser/pyproject.toml
COPY --chown=builduser:builduser README.md /home/builduser/README.md
COPY --chown=builduser:builduser requirements-ml.txt /home/builduser/requirements-ml.txt

# Install remotemedia package with ML extras (non-editable for proper deployment)
RUN cd /home/builduser && pip3 install --no-cache-dir --user ".[ml]"

# Copy proto files and generate gRPC code
COPY --chown=builduser:builduser remote_service/protos/ ./protos/
RUN python3 -m grpc_tools.protoc \
    --proto_path=protos \
    --python_out=. \
    --grpc_python_out=. \
    protos/*.proto

# Runtime stage - minimal runtime environment
FROM ubuntu:22.04 as runtime

# Re-declare build arguments for runtime stage
ARG USER_UID=996  
ARG USER_GID=996

# Set non-interactive mode to prevent prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install runtime dependencies and security tools
RUN apt-get update && apt-get install -y \
    # Python runtime
    python3 \
    python3-pip \
    # Security and sandboxing tools
    bubblewrap \
    firejail \
    # System monitoring tools
    procps \
    psmisc \
    # Cleanup
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user for service with configurable UID/GID
RUN groupadd -g ${USER_GID} remotemedia && \
    useradd -l -u ${USER_UID} -g ${USER_GID} -m -d /home/remotemedia -s /bin/bash remotemedia

# Create application directories with proper ownership
RUN mkdir -p /app/src /app/sandbox /app/logs /app/config /app/cache/models && \
    mkdir -p /home/remotemedia/.cache/huggingface && \
    mkdir -p /home/remotemedia/.cache/torch && \
    chown -R remotemedia:remotemedia /app /home/remotemedia

WORKDIR /app

# Copy Python dependencies from builder with proper ownership
COPY --from=builder --chown=remotemedia:remotemedia /home/builduser/.local /home/remotemedia/.local

# Copy generated gRPC code with proper ownership
COPY --from=builder --chown=remotemedia:remotemedia /home/builduser/*.py /app/src/

# Copy application source code with proper ownership
COPY --chown=remotemedia:remotemedia remote_service/src/ /app/src/
COPY --chown=remotemedia:remotemedia remote_service/config/ /app/config/

# Set up Python path
ENV PYTHONPATH=/app/src:/home/remotemedia/.local/lib/python3.10/site-packages
ENV PATH=/home/remotemedia/.local/bin:$PATH

# Configure security settings
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1
ENV PYTHONHASHSEED=random

# Service configuration
ENV GRPC_PORT=50051
ENV LOG_LEVEL=INFO
ENV SANDBOX_ENABLED=true
ENV MAX_WORKERS=4
ENV SANDBOX_TYPE=bubblewrap

# ML model cache configuration
ENV HF_HOME=/home/remotemedia/.cache/huggingface
ENV TRANSFORMERS_CACHE=/home/remotemedia/.cache/huggingface
ENV TORCH_HOME=/home/remotemedia/.cache/torch
ENV TORCH_CACHE=/home/remotemedia/.cache/torch
ENV REMOTEMEDIA_CACHE_DIR=/app/cache/models

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD python3 /app/src/health_check.py || exit 1


# Expose gRPC port
EXPOSE 50051

# Switch to non-root user
USER remotemedia

# Start the service
CMD ["python3", "/app/src/server.py"] 