# Protobuf Setup for RemoteMedia Python Client

## Overview

The Python client uses Protocol Buffers (protobuf) to communicate with the Rust gRPC server. This document explains how to generate and maintain the protobuf Python files.

## Requirements

- `protobuf==6.31.1` (specified in `requirements.txt`)
- `grpcio>=1.74.0`
- `grpcio-tools>=1.74.0`

## Generating Protobuf Files

The protobuf Python files are generated from the `.proto` files located in `transports/grpc/protos/`.

### Automatic Generation

Run the generation script:

```bash
cd python-client
python scripts/generate_protos.py
```

This script will:
1. Find all `.proto` files in `../transports/grpc/protos/`
2. Generate Python protobuf files (`*_pb2.py`, `*_pb2_grpc.py`)
3. Fix imports to use relative imports (required for proper package structure)
4. Save files to `remotemedia/protos/`

### Manual Verification

Test that imports work:

```bash
python -c "from remotemedia.protos import execution_pb2, common_pb2, streaming_pb2; print('✅ Success!')"
```

## Version Consistency

**Critical**: The protobuf version must be consistent across all components:

### Python Side
- `python-client/requirements.txt`: `protobuf==6.31.1`
- `python-client/setup.py`: `protobuf==6.31.1`

### Rust Side
- Workspace `Cargo.toml`: `prost = "0.14"`, `prost-types = "0.14"`
- Compatible with protobuf wire format 3.x

## Troubleshooting

### Version Mismatch Error

If you see:
```
google.protobuf.runtime_version.VersionError: Detected mismatched Protobuf Gencode/Runtime major versions
```

**Solution**: Ensure your Python environment has protobuf 6.31.1 installed:

```bash
pip install --upgrade protobuf==6.31.1
```

### Import Errors

If you see `ModuleNotFoundError: No module named 'common_pb2'`:

**Solution**: Regenerate protobuf files with the generation script (it fixes imports automatically).

### Multiple Python Environments

The multiprocess executor spawns Python processes using the `python` executable in PATH. If you have multiple Python environments (e.g., anaconda3 and miniconda3), ensure the correct one is active:

```bash
# Check which Python is being used
which python
python --version

# Verify protobuf version
python -c "import google.protobuf; print(google.protobuf.__version__)"
```

To configure a specific Python interpreter for the Rust runtime, set the `PYTHON` environment variable or pass it via the manifest's multiprocess config.

## CI/CD Integration

For CI/CD pipelines, add a step to generate protobuf files:

```yaml
- name: Generate Protobuf Files
  run: |
    cd python-client
    python scripts/generate_protos.py
```

## Files Generated

From `transports/grpc/protos/`:
- `common.proto` → `common_pb2.py`, `common_pb2_grpc.py`
- `execution.proto` → `execution_pb2.py`, `execution_pb2_grpc.py`
- `streaming.proto` → `streaming_pb2.py`, `streaming_pb2_grpc.py`
- `webrtc_signaling.proto` → `webrtc_signaling_pb2.py`, `webrtc_signaling_pb2_grpc.py`

## Development Workflow

1. **Update `.proto` files** in `transports/grpc/protos/`
2. **Regenerate Python files**: `python scripts/generate_protos.py`
3. **Test imports**: Verify no import errors
4. **Commit generated files**: Include them in version control for reproducibility
5. **Update Rust code**: Rebuild Rust crates to use updated proto definitions

