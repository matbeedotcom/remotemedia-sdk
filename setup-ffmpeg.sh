#!/bin/bash
# Setup script to build FFmpeg from source with STATIC linking for Linux/macOS
# Run this once before building: ./setup-ffmpeg.sh
#
# This builds FFmpeg as static libraries so binaries are self-contained
# (no runtime shared library dependencies).
#
# Prerequisites:
#   Linux: build-essential, nasm, yasm, pkg-config
#   macOS: Xcode Command Line Tools, nasm (via brew)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="$SCRIPT_DIR/vendor"
FFMPEG_DIR="$VENDOR_DIR/ffmpeg"
INCLUDE_DIR="$FFMPEG_DIR/include"
LIB_DIR="$FFMPEG_DIR/lib"

# FFmpeg version to build
FFMPEG_VERSION="7.1"
FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"

# Number of parallel jobs
JOBS=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() { echo -e "${CYAN}$1${NC}"; }
success() { echo -e "${GREEN}$1${NC}"; }
warn() { echo -e "${YELLOW}$1${NC}"; }
error() { echo -e "${RED}$1${NC}"; exit 1; }

# Function to update cargo config for static FFmpeg linking (preserves existing content)
update_cargo_config() {
    local cargo_dir="$SCRIPT_DIR/.cargo"
    mkdir -p "$cargo_dir"

    local config_path="$cargo_dir/config.toml"
    local temp_path="$cargo_dir/config.toml.tmp"

    # FFmpeg config lines to add
    local ffmpeg_lines="FFMPEG_INCLUDE_DIR = \"$INCLUDE_DIR\"
FFMPEG_LIB_DIR = \"$LIB_DIR\"
# Use static linking for FFmpeg (single binary, no shared library dependencies)
FFMPEG_LIBS_MODE = \"static\""

    if [ -f "$config_path" ]; then
        # Remove existing FFmpeg-related lines and rebuild
        grep -v -E "^FFMPEG_INCLUDE_DIR\s*=|^FFMPEG_LIB_DIR\s*=|^FFMPEG_LIBS_MODE\s*=|^#.*FFmpeg.*static|^#.*static linking for FFmpeg" "$config_path" > "$temp_path" || true

        # Check if [env] section exists
        if grep -q "^\[env\]" "$temp_path"; then
            # Insert FFmpeg vars after [env] section
            awk -v ffmpeg="$ffmpeg_lines" '
                /^\[env\]/ { print; print ffmpeg; next }
                { print }
            ' "$temp_path" > "$config_path"
        else
            # Add [env] section at the beginning
            echo "[env]" > "$config_path"
            echo "$ffmpeg_lines" >> "$config_path"
            echo "" >> "$config_path"
            cat "$temp_path" >> "$config_path"
        fi
        rm -f "$temp_path"
    else
        # Create new config
        cat > "$config_path" << EOF
[env]
FFMPEG_INCLUDE_DIR = "$INCLUDE_DIR"
FFMPEG_LIB_DIR = "$LIB_DIR"
# Use static linking for FFmpeg (single binary, no shared library dependencies)
FFMPEG_LIBS_MODE = "static"
EOF
    fi

    echo "  Cargo config updated: $config_path"
}

# Check if already built
if [ -f "$LIB_DIR/libavcodec.a" ]; then
    success "FFmpeg static libraries already built at $FFMPEG_DIR"
    update_cargo_config
    exit 0
fi

info "Building FFmpeg $FFMPEG_VERSION from source (STATIC)..."
echo ""

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux*)  PLATFORM="linux" ;;
    Darwin*) PLATFORM="macos" ;;
    *)       error "Unsupported platform: $OS" ;;
esac

info "Detected platform: $PLATFORM"

# Install build dependencies
install_deps() {
    if [ "$PLATFORM" = "linux" ]; then
        if command -v apt-get &> /dev/null; then
            info "Installing build dependencies (apt)..."
            sudo apt-get update
            sudo apt-get install -y build-essential nasm yasm pkg-config curl xz-utils
        elif command -v dnf &> /dev/null; then
            info "Installing build dependencies (dnf)..."
            sudo dnf install -y gcc gcc-c++ make nasm yasm pkgconfig curl xz
        elif command -v pacman &> /dev/null; then
            info "Installing build dependencies (pacman)..."
            sudo pacman -S --needed --noconfirm base-devel nasm yasm pkg-config curl xz
        elif command -v apk &> /dev/null; then
            info "Installing build dependencies (apk)..."
            apk add --no-cache build-base nasm yasm pkgconfig curl xz
        else
            warn "Unknown package manager. Ensure build-essential, nasm, yasm, pkg-config are installed."
        fi
    elif [ "$PLATFORM" = "macos" ]; then
        if ! command -v brew &> /dev/null; then
            error "Homebrew is required on macOS. Install from https://brew.sh"
        fi
        info "Installing build dependencies (brew)..."
        brew install nasm yasm pkg-config || true
    fi
}

# Check for required tools
check_requirements() {
    local missing=""

    for cmd in make gcc nasm curl; do
        if ! command -v $cmd &> /dev/null; then
            missing="$missing $cmd"
        fi
    done

    if [ -n "$missing" ]; then
        warn "Missing required tools:$missing"
        install_deps
    fi
}

check_requirements

# Create directories
mkdir -p "$VENDOR_DIR"
mkdir -p "$FFMPEG_DIR"

# Download FFmpeg source
ARCHIVE_PATH="$VENDOR_DIR/ffmpeg-${FFMPEG_VERSION}.tar.xz"
if [ ! -f "$ARCHIVE_PATH" ]; then
    info "Downloading FFmpeg $FFMPEG_VERSION source..."
    curl -L -o "$ARCHIVE_PATH" "$FFMPEG_URL"
fi

# Extract source
if [ ! -d "$VENDOR_DIR/ffmpeg-${FFMPEG_VERSION}" ]; then
    info "Extracting FFmpeg source..."
    cd "$VENDOR_DIR"
    tar -xf "ffmpeg-${FFMPEG_VERSION}.tar.xz"
fi

# Build FFmpeg
cd "$VENDOR_DIR/ffmpeg-${FFMPEG_VERSION}"

info "Configuring FFmpeg for static build..."

# Platform-specific configure options
EXTRA_CFLAGS=""
EXTRA_LDFLAGS=""

if [ "$PLATFORM" = "macos" ]; then
    # macOS-specific flags for compatibility
    EXTRA_CFLAGS="-mmacosx-version-min=10.15"
    EXTRA_LDFLAGS="-mmacosx-version-min=10.15"
fi

# Configure with all built-in codecs enabled
./configure \
    --prefix="$FFMPEG_DIR" \
    --enable-static \
    --disable-shared \
    --disable-programs \
    --disable-doc \
    --disable-debug \
    --enable-gpl \
    --enable-version3 \
    --enable-nonfree \
    --enable-pic \
    --extra-cflags="$EXTRA_CFLAGS" \
    --extra-ldflags="$EXTRA_LDFLAGS"

# Note: This enables all built-in codecs including:
# Video: H.264, H.265/HEVC, VP8, VP9, AV1, MPEG-1/2/4, ProRes, DNxHD, etc.
# Audio: AAC, MP3, Opus, Vorbis, FLAC, AC3, DTS, PCM variants, etc.
# Containers: MP4, MKV, WebM, AVI, MOV, FLV, etc.

info "Building FFmpeg (this may take 10-20 minutes)..."
make -j"$JOBS"

info "Installing to prefix..."
make install

# Verify build
if [ ! -f "$LIB_DIR/libavcodec.a" ]; then
    error "Build completed but libavcodec.a not found. Check build output."
fi

# Update cargo config (preserves existing settings)
update_cargo_config

echo ""
success "FFmpeg $FFMPEG_VERSION built successfully (STATIC)!"
echo "  Include: $INCLUDE_DIR"
echo "  Lib: $LIB_DIR"
echo ""
info "Static libraries created - no runtime shared library dependencies needed!"
