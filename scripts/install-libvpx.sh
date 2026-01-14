#!/bin/bash
# Build libvpx locally for the project (no system install required)
# Required for media-codec-vpx crate (needs vpx >= 1.15.2)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

LIBVPX_VERSION="${LIBVPX_VERSION:-1.15.0}"
VENDOR_DIR="${VENDOR_DIR:-$PROJECT_ROOT/vendor}"
INSTALL_PREFIX="$VENDOR_DIR/libvpx"
JOBS="${JOBS:-$(nproc)}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Check if already built
check_existing() {
    if [ -f "$INSTALL_PREFIX/lib/pkgconfig/vpx.pc" ]; then
        CURRENT_VERSION=$(PKG_CONFIG_PATH="$INSTALL_PREFIX/lib/pkgconfig" pkg-config --modversion vpx 2>/dev/null || echo "unknown")
        info "libvpx $CURRENT_VERSION already built in vendor/"

        read -p "Rebuild? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            print_instructions
            exit 0
        fi
    fi
}

# Check for required build tools
check_dependencies() {
    info "Checking build dependencies..."

    MISSING_DEPS=""

    # Check for assembler (yasm or nasm)
    if ! command -v yasm &>/dev/null && ! command -v nasm &>/dev/null; then
        MISSING_DEPS="$MISSING_DEPS yasm"
    fi

    # Check for basic build tools
    for cmd in git make gcc g++; do
        if ! command -v "$cmd" &>/dev/null; then
            MISSING_DEPS="$MISSING_DEPS $cmd"
        fi
    done

    if [ -n "$MISSING_DEPS" ]; then
        warn "Missing dependencies:$MISSING_DEPS"
        echo ""
        echo "Install them with:"
        echo "  Ubuntu/Debian: sudo apt install build-essential git yasm"
        echo "  Fedora:        sudo dnf install gcc gcc-c++ make git yasm"
        echo "  Arch:          sudo pacman -S base-devel git yasm"
        echo ""
        read -p "Attempt to install missing dependencies? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            if command -v apt &>/dev/null; then
                sudo apt update && sudo apt install -y build-essential git yasm
            elif command -v dnf &>/dev/null; then
                sudo dnf install -y gcc gcc-c++ make git yasm
            elif command -v pacman &>/dev/null; then
                sudo pacman -S --noconfirm base-devel git yasm
            else
                error "Unknown package manager. Please install dependencies manually."
            fi
        else
            error "Missing dependencies. Cannot continue."
        fi
    fi

    info "All dependencies satisfied"
}

# Download and build libvpx
build_libvpx() {
    info "Building libvpx v$LIBVPX_VERSION into vendor/libvpx..."

    # Create vendor directory
    mkdir -p "$VENDOR_DIR"

    # Clean previous build
    rm -rf "$INSTALL_PREFIX"
    rm -rf "$VENDOR_DIR/libvpx-build"

    cd "$VENDOR_DIR"

    # Clone libvpx
    info "Cloning libvpx repository..."
    git clone --depth 1 --branch "v$LIBVPX_VERSION" \
        https://chromium.googlesource.com/webm/libvpx.git libvpx-build

    cd libvpx-build

    # Configure - build both shared and static, install to vendor/libvpx
    info "Configuring libvpx..."
    ./configure \
        --prefix="$INSTALL_PREFIX" \
        --enable-shared \
        --enable-static \
        --enable-vp8 \
        --enable-vp9 \
        --enable-postproc \
        --enable-vp9-postproc \
        --enable-pic \
        --disable-examples \
        --disable-docs \
        --disable-unit-tests

    # Build
    info "Compiling libvpx (using $JOBS jobs)..."
    make -j"$JOBS"

    # Install to vendor/libvpx
    info "Installing to vendor/libvpx..."
    make install

    # Cleanup build directory
    cd "$VENDOR_DIR"
    rm -rf libvpx-build

    info "Build complete!"
}

# Update .cargo/config.toml
update_cargo_config() {
    local CONFIG_FILE="$PROJECT_ROOT/.cargo/config.toml"

    info "Updating .cargo/config.toml..."

    # Check if config already has our vpx settings
    if [ -f "$CONFIG_FILE" ] && grep -q "PKG_CONFIG_PATH.*vendor/libvpx" "$CONFIG_FILE"; then
        info ".cargo/config.toml already configured for vendor/libvpx"
        return
    fi

    # Create .cargo dir if needed
    mkdir -p "$PROJECT_ROOT/.cargo"

    # Append or create env section
    if [ -f "$CONFIG_FILE" ]; then
        # Check if [env] section exists
        if grep -q '^\[env\]' "$CONFIG_FILE"; then
            # Add to existing [env] section - but we need to be careful
            # For now, just inform the user
            if ! grep -q "PKG_CONFIG_PATH" "$CONFIG_FILE"; then
                warn ".cargo/config.toml has [env] section but no PKG_CONFIG_PATH"
                echo "Add manually:"
                echo ""
                echo 'PKG_CONFIG_PATH = { value = "vendor/libvpx/lib/pkgconfig", relative = true, force = true }'
                echo 'LIBRARY_PATH = { value = "vendor/libvpx/lib", relative = true, force = true }'
                echo ""
            fi
        else
            # Append [env] section
            cat >> "$CONFIG_FILE" << 'EOF'

[env]
PKG_CONFIG_PATH = { value = "vendor/libvpx/lib/pkgconfig", relative = true, force = true }
LIBRARY_PATH = { value = "vendor/libvpx/lib", relative = true, force = true }
EOF
            info "Added [env] section to .cargo/config.toml"
        fi
    else
        # Create new config
        cat > "$CONFIG_FILE" << 'EOF'
[env]
PKG_CONFIG_PATH = { value = "vendor/libvpx/lib/pkgconfig", relative = true, force = true }
LIBRARY_PATH = { value = "vendor/libvpx/lib", relative = true, force = true }
EOF
        info "Created .cargo/config.toml"
    fi
}

# Add vendor to .gitignore
update_gitignore() {
    local GITIGNORE="$PROJECT_ROOT/.gitignore"

    if [ -f "$GITIGNORE" ]; then
        if ! grep -q "^vendor/" "$GITIGNORE" && ! grep -q "^/vendor/" "$GITIGNORE"; then
            echo "" >> "$GITIGNORE"
            echo "# Vendored native libraries" >> "$GITIGNORE"
            echo "vendor/" >> "$GITIGNORE"
            info "Added vendor/ to .gitignore"
        fi
    fi
}

# Print post-install instructions
print_instructions() {
    echo ""
    echo "=============================================="
    echo -e "${GREEN}libvpx v$LIBVPX_VERSION built successfully!${NC}"
    echo "=============================================="
    echo ""
    echo "Location: vendor/libvpx/"
    echo ""

    # Verify
    if PKG_CONFIG_PATH="$INSTALL_PREFIX/lib/pkgconfig" pkg-config --exists 'vpx >= 1.15.0'; then
        NEW_VERSION=$(PKG_CONFIG_PATH="$INSTALL_PREFIX/lib/pkgconfig" pkg-config --modversion vpx)
        echo -e "${GREEN}Verified: libvpx $NEW_VERSION${NC}"
    fi

    echo ""
    echo "Cargo is configured to find it automatically via .cargo/config.toml"
    echo ""
    echo "Run:"
    echo "  cargo check --all-features"
    echo ""
    echo "Or if LD_LIBRARY_PATH is needed at runtime:"
    echo "  LD_LIBRARY_PATH=vendor/libvpx/lib:\$LD_LIBRARY_PATH cargo run ..."
}

# Main
main() {
    echo "=============================================="
    echo "  libvpx Local Build for RemoteMedia SDK"
    echo "=============================================="
    echo ""

    cd "$PROJECT_ROOT"

    check_existing
    check_dependencies
    build_libvpx
    update_cargo_config
    update_gitignore
    print_instructions
}

# Handle Ctrl+C
trap 'echo ""; error "Interrupted by user"' INT

main "$@"
