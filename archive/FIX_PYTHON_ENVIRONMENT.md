# Fix: Python Environment for Multiprocess Nodes

## Problem
Multiprocess Python nodes were failing with protobuf version mismatch because the spawned subprocess was using a different Python environment than expected.

**Error:**
```
google.protobuf.runtime_version.VersionError: Detected mismatched Protobuf Gencode/Runtime 
major versions when loading execution.proto: gencode 6.31.1 runtime 5.29.5
```

**Root cause:** The subprocess loaded:
- Python from `/home/acidhax/miniconda3/` (with old protobuf 5.29.5)
- remotemedia from `/home/acidhax/dev/remote_media_processing/` (old installation)

Instead of:
- Python from `/home/acidhax/anaconda3/` (with protobuf 6.31.1)
- remotemedia from `/home/acidhax/dev/personal/remotemedia-sdk/python-client/` (current SDK)

## Solution

### 1. Updated Rust Runtime to Respect PYTHON Environment Variable

**File:** `runtime-core/src/python/multiprocess/multiprocess_executor.rs`

Changed `default_python_executable()` to check the `PYTHON` environment variable first:

```rust
fn default_python_executable() -> std::path::PathBuf {
    // Check PYTHON environment variable first (standard convention)
    // This allows users to specify which Python interpreter to use
    std::env::var("PYTHON")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("python"))
}
```

This follows the standard Python convention (used by tools like `tox`, `nox`, `pre-commit`, etc.).

### 2. Created WebRTC Server Launch Script

**File:** `transports/webrtc/run_webrtc_server.sh`

Features:
- Sets `PYTHON=/home/acidhax/anaconda3/bin/python`
- Verifies protobuf version matches 6.31.1
- Adds SDK `python-client` to `PYTHONPATH`
- Runs the WebRTC server with correct environment

## Usage

### Option 1: Use the Launch Script (Easiest)

```bash
cd transports/webrtc
./run_webrtc_server.sh
```

### Option 2: Set Environment Manually

```bash
# Set Python interpreter
export PYTHON=/home/acidhax/anaconda3/bin/python

# Add SDK python-client to path
export PYTHONPATH=/home/acidhax/dev/personal/remotemedia-sdk/python-client:$PYTHONPATH

# Run your server
cargo run --bin webrtc_server
```

### Option 3: Configure in Manifest

You can also specify the Python executable in your pipeline manifest:

```yaml
manifest:
  multiprocess_config:
    python_executable: /home/acidhax/anaconda3/bin/python
```

## Verification

After setting `PYTHON`, verify it's correct:

```bash
$PYTHON --version
# Should show: Python 3.12.7

$PYTHON -c "import google.protobuf; print(google.protobuf.__version__)"
# Should show: 6.31.1

$PYTHON -c "from remotemedia.protos import execution_pb2; print('✅ Imports work!')"
# Should show: ✅ Imports work!
```

## Troubleshooting

### Still Getting Version Mismatch?

1. **Check which Python is actually being used:**
   ```bash
   which python
   $PYTHON --version
   ```

2. **Verify protobuf version in that environment:**
   ```bash
   $PYTHON -m pip show protobuf
   ```

3. **Upgrade protobuf if needed:**
   ```bash
   $PYTHON -m pip install --upgrade protobuf==6.31.1
   ```

### Old remotemedia Installation Interfering?

If you have an old installation at `/home/acidhax/dev/remote_media_processing/`:

1. **Remove it or update it:**
   ```bash
   # Option A: Remove
   rm -rf /home/acidhax/dev/remote_media_processing/
   
   # Option B: Update it
   cd /home/acidhax/dev/remote_media_processing/
   pip install --upgrade protobuf==6.31.1
   python scripts/generate_protos.py  # if it exists
   ```

2. **Or ensure PYTHONPATH prioritizes SDK:**
   ```bash
   export PYTHONPATH=/home/acidhax/dev/personal/remotemedia-sdk/python-client:$PYTHONPATH
   ```

## What Changed

1. **Rust runtime now respects `PYTHON` env var** - Standard practice for Python tooling
2. **Launch script sets correct environment** - Ensures consistency
3. **Documentation added** - Clear instructions for different scenarios

## Testing

Run a test pipeline with multiprocess nodes:

```bash
cd transports/webrtc
./run_webrtc_server.sh

# In another terminal, test with a Python node that uses protobuf
# (e.g., kokoro_tts, whisper, etc.)
```

The error should no longer occur!

