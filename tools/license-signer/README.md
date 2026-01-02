# License Signer

Internal tool for generating signed evaluation license files for the RemoteMedia SDK.

## Security

**IMPORTANT**: The private signing key must be kept secure and never committed to version control.

- Private keys are stored in `keys/private.key` (gitignored)
- Only authorized personnel should have access to the private key
- Back up the private key securely - if lost, all existing licenses become unverifiable

## Quick Start

```bash
# 1. Generate a keypair (first time only)
./scripts/generate-keypair.sh

# 2. Update the public key in the demo binary
cargo run -- print-public-key --key-file keys/private.key
# Copy the output to examples/cli/stream-health-demo/src/license.rs

# 3. Sign a license for a customer
./scripts/sign-license.sh \
    --customer "ACME Corp" \
    --expires 2027-01-01 \
    --output acme-license.json

# 4. Verify the license
./scripts/verify-license.sh acme-license.json
```

## Building Licensed Binaries

Create a CLI binary with the license embedded - no separate license file needed:

```bash
# Build licensed binary for a customer
./scripts/sign-license.sh \
    --customer "ACME Corp" \
    --expires 2027-01-01 \
    --bundle

# Cross-compile for a specific target
./scripts/sign-license.sh \
    --customer "ACME Corp" \
    --expires 2027-01-01 \
    --bundle \
    --target x86_64-unknown-linux-gnu

# Custom output directory
./scripts/sign-license.sh \
    --customer "ACME Corp" \
    --expires 2027-01-01 \
    --bundle \
    --bundle-dir ./releases/acme
```

The bundle creates:
- `dist/<customer>/remotemedia-demo` - The CLI binary (license embedded)
- `dist/<customer>/README.txt` - Quick start guide

The customer receives a single binary that just works - no activation step required.

## Commands

### generate-keypair

Generate a new Ed25519 keypair for signing licenses.

```bash
license-signer generate-keypair --output keys/
```

Creates:
- `keys/private.key` - Keep secure, never commit
- `keys/public.key` - Can be shared, embedded in binaries

### sign

Sign a license file for distribution.

```bash
license-signer sign \
    --key-file keys/private.key \
    --customer "ACME Corp" \
    --customer-id "c9f4e3d2-b1a0-4f8e-9d6c-5b4a3e2f1d0c" \
    --expires "2027-01-01" \
    --watermark "EVAL-ACME-CORP" \
    --output license.json
```

Options:
- `--key-file` - Path to private key (required)
- `--customer` - Customer display name (required)
- `--customer-id` - UUID, auto-generated if not provided
- `--license-id` - UUID, auto-generated if not provided
- `--expires` - Expiration date YYYY-MM-DD (required)
- `--not-before` - Valid-from date YYYY-MM-DD (optional)
- `--watermark` - Watermark text for output events (required)
- `--ingest-schemes` - Comma-separated list (default: file,udp,srt,rtmp)
- `--allow-video` - Enable video processing (default: true)
- `--max-session-secs` - Maximum session duration (optional, unlimited if not set)
- `--output` - Output file path (default: license.json)

### print-public-key

Output the public key as a Rust const for embedding in binaries.

```bash
license-signer print-public-key --key-file keys/private.key
```

Output:
```rust
const EVAL_PUBLIC_KEY: [u8; 32] = [
    0xea, 0xcf, 0x23, 0x7c, 0xca, 0x67, 0xbc, 0x95,
    ...
];
```

### verify

Verify a license file signature.

```bash
license-signer verify \
    --license-file license.json \
    --key-file keys/private.key
```

## License File Format

```json
{
  "version": 1,
  "customer_id": "c9f4e3d2-b1a0-4f8e-9d6c-5b4a3e2f1d0c",
  "license_id": "940a5af3-a49a-4188-bb1b-ff6569f9ecf9",
  "issued_at": "2026-01-02T06:09:01.363045178+00:00",
  "expires_at": "2027-01-01T23:59:59+00:00",
  "entitlements": {
    "allow_ingest_schemes": ["file", "udp", "srt", "rtmp"],
    "allow_video": true
  },
  "watermark": "EVAL-ACME-CORP",
  "signature": "base64-encoded-ed25519-signature..."
}
```

## Helper Scripts

- `scripts/generate-keypair.sh` - Generate new keypair
- `scripts/sign-license.sh` - Sign a license with common defaults
- `scripts/verify-license.sh` - Verify a license signature

## Building

```bash
# From this directory
cargo build --release

# Or from repo root
make cli-license-signer
```

## Cryptographic Details

- Algorithm: Ed25519 (RFC 8032)
- Signature verification: `verify_strict()` (no malleability)
- Payload canonicalization: RFC 8785 (JSON Canonicalization Scheme)
- Encoding: Base64 for signatures, raw bytes for embedded keys
