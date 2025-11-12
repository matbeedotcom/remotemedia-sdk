# Build RemoteMedia Docker node image with full Python package support
#
# This script pre-builds a Docker image that includes:
# - Python 3.10 runtime
# - iceoryx2 for IPC
# - remotemedia package from local source
# - Common dependencies
#
# Usage:
#   .\build-remotemedia-node.ps1 [-PythonVersion "3.10"]
#
# Examples:
#   .\build-remotemedia-node.ps1
#   .\build-remotemedia-node.ps1 -PythonVersion "3.11"

param(
    [string]$PythonVersion = "3.10"
)

$ImageName = "remotemedia/python-node"
$ImageTag = "${ImageName}:py${PythonVersion}"

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "Building RemoteMedia Docker Node Image" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan
Write-Host "Python version: $PythonVersion"
Write-Host "Image tag: $ImageTag"
Write-Host ""

# Navigate to workspace root
$ScriptDir = Split-Path -Parent $PSCommandPath
$WorkspaceRoot = Resolve-Path (Join-Path $ScriptDir "..\..") | Select-Object -ExpandProperty Path
Set-Location $WorkspaceRoot

Write-Host "Workspace root: $WorkspaceRoot"
Write-Host ""

# Verify python-client exists
$PythonClientPath = Join-Path $WorkspaceRoot "python-client\remotemedia"
if (-not (Test-Path $PythonClientPath)) {
    Write-Host "ERROR: python-client/remotemedia directory not found" -ForegroundColor Red
    Write-Host "Expected at: $PythonClientPath" -ForegroundColor Red
    exit 1
}

Write-Host "✓ Found python-client directory" -ForegroundColor Green
Write-Host ""

# Create Dockerfile content
$DockerfileContent = @"
# ===========================================================================
# Stage 1: Builder - Install dependencies with build tools
# ===========================================================================
FROM python:${PythonVersion}-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    gcc \
    g++ \
    git \
    && rm -rf /var/lib/apt/lists/*

# Create virtual environment
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:`$PATH"

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
FROM python:${PythonVersion}-slim as runtime

# Install only runtime system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    libsndfile1 \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:`$PATH"

# Create non-root user
RUN useradd -m -u 1000 remotemedia && \
    mkdir -p /tmp/iceoryx2 && \
    chown remotemedia:remotemedia /tmp/iceoryx2

# Create diagnostic script
RUN echo '#!/usr/bin/env python3' > /tmp/test_remotemedia.py && \
    echo 'import sys, os' >> /tmp/test_remotemedia.py && \
    echo 'print("=== RemoteMedia Docker Container Diagnostics ===")' >> /tmp/test_remotemedia.py && \
    echo 'print(f"Python: {sys.version}")' >> /tmp/test_remotemedia.py && \
    echo 'print()' >> /tmp/test_remotemedia.py && \
    echo 'try:' >> /tmp/test_remotemedia.py && \
    echo '    import remotemedia' >> /tmp/test_remotemedia.py && \
    echo '    print(f"✓ remotemedia imported from {remotemedia.__file__}")' >> /tmp/test_remotemedia.py && \
    echo 'except ImportError as e:' >> /tmp/test_remotemedia.py && \
    echo '    print(f"✗ remotemedia import failed: {e}")' >> /tmp/test_remotemedia.py && \
    echo 'try:' >> /tmp/test_remotemedia.py && \
    echo '    import iceoryx2' >> /tmp/test_remotemedia.py && \
    echo '    print(f"✓ iceoryx2 imported from {iceoryx2.__file__}")' >> /tmp/test_remotemedia.py && \
    echo 'except ImportError as e:' >> /tmp/test_remotemedia.py && \
    echo '    print(f"✗ iceoryx2 import failed: {e}")' >> /tmp/test_remotemedia.py && \
    echo 'print(f"✓ /tmp/iceoryx2: exists={os.path.exists(\"/tmp/iceoryx2\")}")' >> /tmp/test_remotemedia.py && \
    echo 'print("Container ready!")' >> /tmp/test_remotemedia.py && \
    chmod +x /tmp/test_remotemedia.py

USER remotemedia
WORKDIR /home/remotemedia/app

ENV PYTHONUNBUFFERED=1
ENV PYTHONDONTWRITEBYTECODE=1

STOPSIGNAL SIGTERM

CMD ["python", "/tmp/test_remotemedia.py"]
"@

# Write Dockerfile
$TempDockerfile = Join-Path $env:TEMP "remotemedia-node.Dockerfile"
$DockerfileContent | Out-File -FilePath $TempDockerfile -Encoding ASCII

Write-Host "✓ Dockerfile created at: $TempDockerfile" -ForegroundColor Green
Write-Host ""

# Build image
Write-Host "Building Docker image (this may take a few minutes)..." -ForegroundColor Yellow
Write-Host ""

docker build `
    -f $TempDockerfile `
    -t $ImageTag `
    --progress=plain `
    .

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "============================================" -ForegroundColor Green
    Write-Host "✓ Build successful!" -ForegroundColor Green
    Write-Host "============================================" -ForegroundColor Green
    Write-Host "Image: $ImageTag"
    Write-Host ""
    Write-Host "Test the image:"
    Write-Host "  docker run --rm $ImageTag" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Run with iceoryx2 mounts:"
    Write-Host "  docker run --rm ```" -ForegroundColor Cyan
    Write-Host "    -v /tmp/iceoryx2:/tmp/iceoryx2 ```" -ForegroundColor Cyan
    Write-Host "    -v /dev/shm:/dev/shm ```" -ForegroundColor Cyan
    Write-Host "    --shm-size=2g ```" -ForegroundColor Cyan
    Write-Host "    $ImageTag" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Interactive shell:"
    Write-Host "  docker run --rm -it $ImageTag /bin/bash" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "List installed Python packages:"
    Write-Host "  docker run --rm $ImageTag pip list" -ForegroundColor Cyan
    Write-Host ""
} else {
    Write-Host ""
    Write-Host "============================================" -ForegroundColor Red
    Write-Host "✗ Build failed (exit code: $LASTEXITCODE)" -ForegroundColor Red
    Write-Host "============================================" -ForegroundColor Red
    exit $LASTEXITCODE
}
