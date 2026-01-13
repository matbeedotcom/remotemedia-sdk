#!/bin/bash
# Verify a license file signature
#
# Usage: ./verify-license.sh license.json [--key-file private.key]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIGNER_DIR="$(dirname "$SCRIPT_DIR")"

# Default values
KEY_FILE="$SIGNER_DIR/keys/private.key"
LICENSE_FILE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --key-file)
            KEY_FILE="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 license.json [--key-file private.key]"
            exit 0
            ;;
        *)
            LICENSE_FILE="$1"
            shift
            ;;
    esac
done

if [[ -z "$LICENSE_FILE" ]]; then
    echo "Error: License file required"
    echo "Usage: $0 license.json [--key-file private.key]"
    exit 1
fi

if [[ ! -f "$LICENSE_FILE" ]]; then
    echo "Error: License file not found: $LICENSE_FILE"
    exit 1
fi

if [[ ! -f "$KEY_FILE" ]]; then
    echo "Error: Private key not found: $KEY_FILE"
    exit 1
fi

# Build the signer if needed
if ! command -v license-signer &> /dev/null; then
    cd "$SIGNER_DIR"
    cargo build --release --quiet
    SIGNER="$SIGNER_DIR/target/release/license-signer"
else
    SIGNER="license-signer"
fi

# Verify the license
$SIGNER verify --license-file "$LICENSE_FILE" --key-file "$KEY_FILE"
