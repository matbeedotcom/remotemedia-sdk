# Multi-stage build for RemoteMedia gRPC Server
# Optimized for AWS ECS/Fargate deployment

# Stage 1: Build
FROM rust:1.75-slim-bookworm as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY runtime-core ./runtime-core
COPY transports ./transports
COPY protos ./protos

# Build in release mode with optimizations
RUN cargo build --bin grpc-server --release \
    --features grpc-signaling,docker,multiprocess,silero-vad

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1001 -s /bin/bash remotemedia

# Copy binary from builder
COPY --from=builder /app/target/release/grpc-server /usr/local/bin/grpc-server

# Copy default config (if exists)
# COPY config/default.yaml /etc/remotemedia/config.yaml

# Set ownership
RUN chown -R remotemedia:remotemedia /usr/local/bin/grpc-server

# Switch to non-root user
USER remotemedia

# Expose gRPC port
EXPOSE 50051

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:50051/health || exit 1

# Default command
ENTRYPOINT ["grpc-server"]
CMD ["--grpc-address", "0.0.0.0:50051"]


