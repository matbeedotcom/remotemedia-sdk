#!/bin/bash
# Sign a new evaluation license and optionally bundle with CLI binary
#
# Usage: ./sign-license.sh --customer "Company Name" --expires 2027-01-01 [options]
#
# Required arguments:
#   --customer NAME       Customer name (e.g., "ACME Corp")
#   --expires DATE        Expiration date (YYYY-MM-DD)
#
# Optional arguments:
#   --key-file PATH       Private key file (default: ./keys/private.key)
#   --watermark TEXT      Watermark text (default: EVAL-<CUSTOMER>)
#   --output PATH         Output file (default: license.json)
#   --ingest-schemes LIST Comma-separated schemes (default: file,udp,srt,rtmp)
#   --no-video            Disable video processing
#   --max-session SECS    Maximum session duration in seconds
#   --customer-id UUID    Specific customer ID (auto-generated if not set)
#   --license-id UUID     Specific license ID (auto-generated if not set)
#   --not-before DATE     License valid-from date (YYYY-MM-DD)
#   --bundle              Build and bundle the CLI binary with the license
#   --bundle-dir PATH     Output directory for bundle (default: ./dist/<customer>)
#   --target TARGET       Cross-compile target (e.g., x86_64-unknown-linux-gnu)
#   --pipeline NAME       Pipeline to embed (name like demo_audio_quality_v1, or path)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIGNER_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(cd "$SIGNER_DIR/../.." && pwd)"

# Default values
KEY_FILE="$SIGNER_DIR/keys/private.key"
OUTPUT="license.json"
INGEST_SCHEMES="file,udp,srt,rtmp"
ALLOW_VIDEO="true"
CUSTOMER=""
EXPIRES=""
WATERMARK=""
MAX_SESSION=""
CUSTOMER_ID=""
LICENSE_ID=""
NOT_BEFORE=""
BUNDLE=false
BUNDLE_DIR=""
TARGET=""
PIPELINE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --customer)
            CUSTOMER="$2"
            shift 2
            ;;
        --expires)
            EXPIRES="$2"
            shift 2
            ;;
        --key-file)
            KEY_FILE="$2"
            shift 2
            ;;
        --watermark)
            WATERMARK="$2"
            shift 2
            ;;
        --output)
            OUTPUT="$2"
            shift 2
            ;;
        --ingest-schemes)
            INGEST_SCHEMES="$2"
            shift 2
            ;;
        --no-video)
            ALLOW_VIDEO="false"
            shift
            ;;
        --max-session)
            MAX_SESSION="$2"
            shift 2
            ;;
        --customer-id)
            CUSTOMER_ID="$2"
            shift 2
            ;;
        --license-id)
            LICENSE_ID="$2"
            shift 2
            ;;
        --not-before)
            NOT_BEFORE="$2"
            shift 2
            ;;
        --bundle)
            BUNDLE=true
            shift
            ;;
        --bundle-dir)
            BUNDLE_DIR="$2"
            shift 2
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        --pipeline)
            PIPELINE="$2"
            shift 2
            ;;
        --help|-h)
            head -35 "$0" | tail -33
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Validate required arguments
if [[ -z "$CUSTOMER" ]]; then
    echo "Error: --customer is required"
    echo "Usage: $0 --customer \"Company Name\" --expires 2027-01-01"
    exit 1
fi

if [[ -z "$EXPIRES" ]]; then
    echo "Error: --expires is required"
    echo "Usage: $0 --customer \"Company Name\" --expires 2027-01-01"
    exit 1
fi

# Check key file exists
if [[ ! -f "$KEY_FILE" ]]; then
    echo "Error: Private key not found: $KEY_FILE"
    echo "Run ./generate-keypair.sh first"
    exit 1
fi

# Generate watermark if not provided
if [[ -z "$WATERMARK" ]]; then
    # Create watermark from customer name (uppercase, no spaces)
    WATERMARK="EVAL-$(echo "$CUSTOMER" | tr '[:lower:]' '[:upper:]' | tr ' ' '-' | tr -cd 'A-Z0-9-')"
fi

# Build the signer if needed
if ! command -v license-signer &> /dev/null; then
    echo "Building license-signer..."
    cd "$SIGNER_DIR"
    cargo build --release --quiet
    SIGNER="$SIGNER_DIR/target/release/license-signer"
else
    SIGNER="license-signer"
fi

# Build command
CMD="$SIGNER sign --key-file \"$KEY_FILE\" --customer \"$CUSTOMER\" --expires \"$EXPIRES\" --watermark \"$WATERMARK\" --ingest-schemes \"$INGEST_SCHEMES\" --output \"$OUTPUT\""

# Only add --allow-video flag (clap bool flag defaults to true, pass false to disable)
if [[ "$ALLOW_VIDEO" == "false" ]]; then
    CMD="$CMD --allow-video false"
fi

if [[ -n "$MAX_SESSION" ]]; then
    CMD="$CMD --max-session-secs $MAX_SESSION"
fi

if [[ -n "$CUSTOMER_ID" ]]; then
    CMD="$CMD --customer-id \"$CUSTOMER_ID\""
fi

if [[ -n "$LICENSE_ID" ]]; then
    CMD="$CMD --license-id \"$LICENSE_ID\""
fi

if [[ -n "$NOT_BEFORE" ]]; then
    CMD="$CMD --not-before \"$NOT_BEFORE\""
fi

# Remember original directory for output file
ORIGINAL_DIR="$(pwd)"

# Sign the license
echo "Signing license for: $CUSTOMER"
eval $CMD

# Make OUTPUT an absolute path for later use
if [[ ! "$OUTPUT" = /* ]]; then
    OUTPUT="$ORIGINAL_DIR/$OUTPUT"
fi

echo ""
echo "License saved to: $OUTPUT"

# Bundle with CLI binary if requested
if [[ "$BUNDLE" == "true" ]]; then
    echo ""
    echo "Building CLI binary with embedded license..."

    # Create sanitized customer name for directory
    CUSTOMER_SLUG="$(echo "$CUSTOMER" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-')"

    # Set bundle directory
    if [[ -z "$BUNDLE_DIR" ]]; then
        BUNDLE_DIR="$SIGNER_DIR/dist/$CUSTOMER_SLUG"
    fi

    # Build the demo CLI with embedded license and public key
    DEMO_DIR="$REPO_ROOT/examples/cli/stream-health-demo"
    EXAMPLES_TARGET="$REPO_ROOT/examples/target"
    cd "$DEMO_DIR"

    # Set the license and public key paths for build.rs to embed
    export REMOTEMEDIA_LICENSE="$OUTPUT"

    # Derive public key path from private key path
    KEY_DIR="$(dirname "$KEY_FILE")"
    PUBLIC_KEY_FILE="$KEY_DIR/public.key"
    if [[ ! -f "$PUBLIC_KEY_FILE" ]]; then
        echo "Error: Public key not found: $PUBLIC_KEY_FILE"
        echo "Run ./generate-keypair.sh first"
        exit 1
    fi
    export REMOTEMEDIA_PUBLIC_KEY="$PUBLIC_KEY_FILE"

    # Set pipeline if specified
    if [[ -n "$PIPELINE" ]]; then
        export REMOTEMEDIA_PIPELINE="$PIPELINE"
        echo "Using pipeline: $PIPELINE"
    fi

    CARGO_BUILD_CMD="cargo build --release"
    if [[ -n "$TARGET" ]]; then
        CARGO_BUILD_CMD="$CARGO_BUILD_CMD --target $TARGET"
        BINARY_PATH="$EXAMPLES_TARGET/$TARGET/release/remotemedia-demo"
    else
        BINARY_PATH="$EXAMPLES_TARGET/release/remotemedia-demo"
    fi

    echo "Running: REMOTEMEDIA_LICENSE=$OUTPUT REMOTEMEDIA_PUBLIC_KEY=$PUBLIC_KEY_FILE${PIPELINE:+ REMOTEMEDIA_PIPELINE=$PIPELINE} $CARGO_BUILD_CMD"
    eval $CARGO_BUILD_CMD

    # Create bundle directory
    mkdir -p "$BUNDLE_DIR"

    # Determine binary name based on target
    BINARY_NAME="remotemedia-demo"
    if [[ "$TARGET" == *"windows"* ]]; then
        BINARY_NAME="remotemedia-demo.exe"
        BINARY_PATH="${BINARY_PATH}.exe"
    fi

    # Copy the binary (license is embedded, no separate file needed)
    cp "$BINARY_PATH" "$BUNDLE_DIR/$BINARY_NAME"

    # Create README for the customer
    cat > "$BUNDLE_DIR/README.txt" << EOF
RemoteMedia Stream Health Demo
==============================

Licensed to: $CUSTOMER
Expires: $EXPIRES
Watermark: $WATERMARK

This binary has your license embedded - no activation required!

Quick Start
-----------

1. Run the demo:
   ./$BINARY_NAME -i audio.wav
   ./$BINARY_NAME --ingest rtmp://server/live/stream

2. Check license status:
   ./$BINARY_NAME --license-status

For help:
   ./$BINARY_NAME --help

Support: https://remotemedia.dev/support
EOF

    echo ""
    echo "============================================"
    echo "Licensed binary created successfully!"
    echo "============================================"
    echo ""
    echo "  Binary:    $BUNDLE_DIR/$BINARY_NAME"
    echo "  Customer:  $CUSTOMER"
    echo "  Expires:   $EXPIRES"
    echo "  Watermark: $WATERMARK"
    echo ""
    echo "The license is embedded in the binary - no separate license file needed."
    echo "Send '$BINARY_NAME' to the customer."
else
    echo ""
    echo "To verify the license:"
    echo "  $SIGNER verify --license-file \"$OUTPUT\" --key-file \"$KEY_FILE\""
    echo ""
    echo "To activate in the demo:"
    echo "  remotemedia-demo activate --file \"$OUTPUT\""
    echo ""
    echo "To bundle with CLI binary, add --bundle flag"
fi
