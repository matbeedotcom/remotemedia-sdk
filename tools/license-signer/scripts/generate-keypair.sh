#!/bin/bash
# Generate a new Ed25519 keypair for license signing
#
# Usage: ./generate-keypair.sh [output-dir]
#
# Default output directory: ./keys/
#
# SECURITY: The private.key file must be kept secure!
# Never commit private.key to version control.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIGNER_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${1:-$SIGNER_DIR/keys}"

echo "License Keypair Generator"
echo "========================="
echo ""

# Build the signer if needed
if ! command -v license-signer &> /dev/null; then
    echo "Building license-signer..."
    cd "$SIGNER_DIR"
    cargo build --release --quiet
    SIGNER="$SIGNER_DIR/target/release/license-signer"
else
    SIGNER="license-signer"
fi

# Generate keypair
echo "Generating new Ed25519 keypair..."
$SIGNER generate-keypair --output "$OUTPUT_DIR"

echo ""
echo "To embed the public key in the demo binary, run:"
echo "  $SIGNER print-public-key --key-file $OUTPUT_DIR/private.key"
echo ""
echo "SECURITY REMINDER:"
echo "  - Keep private.key secure and never commit to git"
echo "  - Add '$OUTPUT_DIR/private.key' to .gitignore"
echo "  - Back up private.key securely"
