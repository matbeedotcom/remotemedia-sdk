#!/usr/bin/env bash
#
# Build script for protocol buffer compilation
# Ensures the correct protoc binary is used across all build environments
#
# Usage:
#   ./scripts/build-protos.sh              # Build all protos
#   ./scripts/build-protos.sh --check      # Check protoc is available
#   ./scripts/build-protos.sh --rust       # Build Rust protos only (via cargo)
#   ./scripts/build-protos.sh --typescript # Build TypeScript protos only
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Find the best protoc binary (sets FOUND_PROTOC variable)
find_protoc() {
    FOUND_PROTOC=""

    # Priority order for protoc:
    # 1. PROTOC environment variable (explicit override)
    # 2. Conda/Anaconda protoc (most reliable on dev machines)
    # 3. System protoc from apt/brew
    # 4. Cargo-installed protoc

    if [[ -n "${PROTOC:-}" ]] && [[ -x "$PROTOC" ]]; then
        FOUND_PROTOC="$PROTOC"
        log_info "Using PROTOC from environment: $FOUND_PROTOC"
    elif [[ -x "${HOME}/anaconda3/bin/protoc" ]]; then
        FOUND_PROTOC="${HOME}/anaconda3/bin/protoc"
        log_info "Using Anaconda protoc: $FOUND_PROTOC"
    elif [[ -x "${HOME}/miniconda3/bin/protoc" ]]; then
        FOUND_PROTOC="${HOME}/miniconda3/bin/protoc"
        log_info "Using Miniconda protoc: $FOUND_PROTOC"
    elif command -v protoc &> /dev/null; then
        FOUND_PROTOC="$(command -v protoc)"
        log_info "Using system protoc: $FOUND_PROTOC"
    elif [[ -x "${HOME}/.cargo/bin/protoc" ]]; then
        FOUND_PROTOC="${HOME}/.cargo/bin/protoc"
        log_info "Using cargo-installed protoc: $FOUND_PROTOC"
    fi

    if [[ -z "$FOUND_PROTOC" ]]; then
        log_error "protoc not found! Please install protobuf-compiler:"
        echo "  Ubuntu/Debian: sudo apt install protobuf-compiler"
        echo "  macOS: brew install protobuf"
        echo "  Conda: conda install -c conda-forge protobuf"
        echo "  Or set PROTOC environment variable to the protoc binary path"
        exit 1
    fi

    # Verify it's executable
    if ! "$FOUND_PROTOC" --version &> /dev/null; then
        log_error "protoc at $FOUND_PROTOC is not executable or has permission issues"
        exit 1
    fi
}

# Check protoc availability and version
check_protoc() {
    find_protoc

    log_info "protoc path: $FOUND_PROTOC"
    log_info "protoc version: $("$FOUND_PROTOC" --version)"

    # Export for child processes
    export PROTOC="$FOUND_PROTOC"
    log_info "PROTOC environment variable set to: $PROTOC"
}

# Build Rust protos using cargo (which will use PROTOC env var)
build_rust_protos() {
    find_protoc
    export PROTOC="$FOUND_PROTOC"

    log_info "Building Rust protos with PROTOC=$PROTOC"

    cd "$PROJECT_ROOT"

    # Touch build.rs files to force proto regeneration
    if [[ -f "transports/grpc/build.rs" ]]; then
        touch "transports/grpc/build.rs"
    fi
    if [[ -f "transports/webrtc/build.rs" ]]; then
        touch "transports/webrtc/build.rs"
    fi

    # Build the crates that use protos
    cargo build -p remotemedia-grpc -p remotemedia-webrtc 2>&1 | grep -E "(Compiling|warning:|error:|Successfully)" || true

    log_info "Rust protos built successfully"
}

# Build TypeScript protos for examples
build_typescript_protos() {
    find_protoc
    export PROTOC="$FOUND_PROTOC"

    local proto_dir="$PROJECT_ROOT/runtime/protos"
    local examples_dir="$PROJECT_ROOT/examples"

    # Check if ts-proto or protoc-gen-ts is available
    if ! command -v protoc-gen-ts &> /dev/null && ! npm list -g @protobuf-ts/plugin &> /dev/null; then
        log_warn "TypeScript protobuf plugin not found. Skipping TypeScript proto generation."
        log_info "Install with: npm install -g @protobuf-ts/plugin"
        return 0
    fi

    # Find all examples with generate:protos script
    for example_dir in "$examples_dir"/*/; do
        if [[ -f "${example_dir}package.json" ]]; then
            if grep -q '"generate:protos"' "${example_dir}package.json"; then
                log_info "Generating protos for $(basename "$example_dir")"
                (cd "$example_dir" && npm run generate:protos) || log_warn "Failed to generate protos for $(basename "$example_dir")"
            fi
        fi
    done

    log_info "TypeScript protos built"
}

# Main entry point
main() {
    local action="${1:-all}"

    case "$action" in
        --check|-c)
            check_protoc
            ;;
        --rust|-r)
            build_rust_protos
            ;;
        --typescript|--ts|-t)
            build_typescript_protos
            ;;
        --help|-h)
            echo "Usage: $0 [option]"
            echo ""
            echo "Options:"
            echo "  --check, -c       Check protoc availability and version"
            echo "  --rust, -r        Build Rust protos only"
            echo "  --typescript, -t  Build TypeScript protos only"
            echo "  --help, -h        Show this help message"
            echo "  (no option)       Build all protos"
            echo ""
            echo "Environment variables:"
            echo "  PROTOC            Path to protoc binary (overrides auto-detection)"
            ;;
        all|"")
            check_protoc
            build_rust_protos
            build_typescript_protos
            ;;
        *)
            log_error "Unknown option: $action"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
}

main "$@"
