#!/bin/bash
# Setup script to download/configure FFmpeg for builds
# Run this once before building: ./setup-ffmpeg.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="$SCRIPT_DIR/vendor"
FFMPEG_DIR="$VENDOR_DIR/ffmpeg"
INCLUDE_DIR="$FFMPEG_DIR/include"
LIB_DIR="$FFMPEG_DIR/lib"

# Function to create cargo config
create_cargo_config() {
    local CARGO_DIR="$SCRIPT_DIR/.cargo"
    mkdir -p "$CARGO_DIR"

    local CONFIG_PATH="$CARGO_DIR/config.toml"
    cat > "$CONFIG_PATH" << EOF
[env]
FFMPEG_INCLUDE_DIR = "$INCLUDE_DIR"
FFMPEG_LIB_DIR = "$LIB_DIR"
EOF
    echo "  Cargo config: $CONFIG_PATH"
}

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux*)     PLATFORM="linux";;
    Darwin*)    PLATFORM="macos";;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows";;
    *)          echo "Unsupported platform: $OS"; exit 1;;
esac

echo "Detected platform: $PLATFORM"

# Check if already installed
if [ -d "$INCLUDE_DIR/libavcodec" ] && [ -d "$LIB_DIR" ]; then
    echo "FFmpeg already installed at $FFMPEG_DIR"
    create_cargo_config
    exit 0
fi

# Platform-specific installation
case "$PLATFORM" in
    linux)
        echo "Checking for FFmpeg development packages..."

        # Try pkg-config first
        if command -v pkg-config &> /dev/null; then
            if pkg-config --exists libavcodec libavformat libavutil 2>/dev/null; then
                PKG_INCLUDE=$(pkg-config --variable=includedir libavcodec 2>/dev/null || echo "")
                PKG_LIB=$(pkg-config --variable=libdir libavcodec 2>/dev/null || echo "")

                if [ -n "$PKG_INCLUDE" ] && [ -n "$PKG_LIB" ]; then
                    echo "Found system FFmpeg via pkg-config"
                    INCLUDE_DIR="$PKG_INCLUDE"
                    LIB_DIR="$PKG_LIB"
                    create_cargo_config
                    echo ""
                    echo "FFmpeg configured successfully!"
                    echo "  Include: $INCLUDE_DIR"
                    echo "  Lib: $LIB_DIR"
                    exit 0
                fi
            fi
        fi

        echo ""
        echo "FFmpeg development packages not found. Please install:"
        echo "  Ubuntu/Debian: sudo apt-get install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev"
        echo "  Fedora/RHEL:   sudo dnf install ffmpeg-devel"
        echo "  Arch:          sudo pacman -S ffmpeg"
        echo ""
        echo "Then run this script again."
        exit 1
        ;;

    macos)
        echo "Checking for FFmpeg via Homebrew..."

        if command -v brew &> /dev/null; then
            BREW_PREFIX=$(brew --prefix ffmpeg 2>/dev/null || echo "")

            if [ -n "$BREW_PREFIX" ] && [ -d "$BREW_PREFIX/include" ]; then
                echo "Found FFmpeg via Homebrew at $BREW_PREFIX"
                INCLUDE_DIR="$BREW_PREFIX/include"
                LIB_DIR="$BREW_PREFIX/lib"
                create_cargo_config
                echo ""
                echo "FFmpeg configured successfully!"
                echo "  Include: $INCLUDE_DIR"
                echo "  Lib: $LIB_DIR"
                exit 0
            fi
        fi

        echo ""
        echo "FFmpeg not found. Please install via Homebrew:"
        echo "  brew install ffmpeg"
        echo ""
        echo "Then run this script again."
        exit 1
        ;;

    windows)
        echo "Downloading FFmpeg for Windows..."

        mkdir -p "$VENDOR_DIR"

        # Try to download full shared build
        DOWNLOAD_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip"
        ARCHIVE_PATH="$VENDOR_DIR/ffmpeg.zip"
        EXTRACT_DIR="$VENDOR_DIR/ffmpeg-extract"

        echo "Downloading from $DOWNLOAD_URL..."
        if command -v curl &> /dev/null; then
            curl -L -o "$ARCHIVE_PATH" "$DOWNLOAD_URL"
        elif command -v wget &> /dev/null; then
            wget -O "$ARCHIVE_PATH" "$DOWNLOAD_URL"
        else
            echo "Error: curl or wget required to download FFmpeg"
            exit 1
        fi

        echo "Extracting..."
        rm -rf "$EXTRACT_DIR"
        mkdir -p "$EXTRACT_DIR"
        unzip -q "$ARCHIVE_PATH" -d "$EXTRACT_DIR"

        # Find extracted folder
        EXTRACTED_FOLDER=$(find "$EXTRACT_DIR" -maxdepth 1 -type d -name "ffmpeg-*" | head -1)

        if [ -z "$EXTRACTED_FOLDER" ]; then
            echo "Error: Could not find extracted FFmpeg folder"
            exit 1
        fi

        # Create target directory
        rm -rf "$FFMPEG_DIR"
        mkdir -p "$FFMPEG_DIR"

        # Copy directories
        echo "Copying include and lib directories..."
        cp -r "$EXTRACTED_FOLDER/include" "$FFMPEG_DIR/"
        cp -r "$EXTRACTED_FOLDER/lib" "$FFMPEG_DIR/"

        if [ -d "$EXTRACTED_FOLDER/bin" ]; then
            cp -r "$EXTRACTED_FOLDER/bin" "$FFMPEG_DIR/"
            echo "Note: FFmpeg DLLs copied to $FFMPEG_DIR/bin"
        fi

        # Cleanup
        echo "Cleaning up..."
        rm -f "$ARCHIVE_PATH"
        rm -rf "$EXTRACT_DIR"

        create_cargo_config

        echo ""
        echo "FFmpeg installed successfully!"
        echo "  Include: $INCLUDE_DIR"
        echo "  Lib: $LIB_DIR"
        ;;
esac

echo ""
echo "You can now build with: cargo build"
