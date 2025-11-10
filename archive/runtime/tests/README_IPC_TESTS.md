# IPC Integration Tests

Automated tests for verifying Rust-to-Python IPC communication via iceoryx2.

## Quick Start

```bash
# Run all IPC tests
cargo test --test test_ipc_communication --features multiprocess -- --nocapture

# Run specific test
cargo test --test test_ipc_communication test_ipc_channel_creation --features multiprocess -- --nocapture
```

## Test Overview

### 1. `test_ipc_channel_creation` ✅
**What it tests:**
- iceoryx2 channel creation
- Rust publisher → Rust subscriber communication
- Basic publish/receive functionality

**Status:** PASSING

**Runtime:** ~0.1s

---

### 2. `test_ipc_roundtrip_text` 🚧
**What it tests:**
- Full Rust → Python → Rust roundtrip
- Python EchoNode receives data via IPC
- Python sends response back via IPC
- Text data serialization/deserialization

**Status:** IN PROGRESS (Python IPC receive debugging)

**Runtime:** ~15s (includes Python process spawn)

**Expected behavior:**
1. Rust creates IPC channels
2. Rust spawns Python EchoNode process
3. Rust publishes text to `test_echo_input` channel
4. Python receives via subscriber
5. Python echoes back to `test_echo_output` channel
6. Rust receives echoed response

---

### 3. `test_ipc_roundtrip_audio` 🚧
**What it tests:**
- Audio data roundtrip through Python
- F32 audio sample serialization
- Large payload handling (1000+ samples)

**Status:** IN PROGRESS

---

## Current Issues Being Debugged

### Issue: Python `has_samples()` returns False

**Symptoms:**
- Rust successfully publishes data (logs: "Successfully sent input to IPC channel")
- Python subscriber connects (logs: "Connected iceoryx2 IPC channels")
- Python process loop runs (logs: "STARTING PROCESS LOOP")
- But `has_samples()` always returns `False`

**Investigation:**
- Both Rust and Python use `ServiceType.Ipc` ✅
- Both use `Slice[c_uint8]` type ✅
- Both use matching `history_size(0)`, `subscriber_max_buffer_size(100)` ✅
- Both use `.open_or_create()` with same config ✅

**Logs to check:**
```bash
grep "Creating publisher\|Successfully sent\|has_samples\|RECEIVED IPC" log.log
```

## Running Manual Tests

For debugging the Python side with full logs:

```bash
# 1. Start gRPC server
cd runtime
cargo run --bin grpc_server --features grpc-transport,multiprocess

# 2. In another terminal, send a test request
# (use your existing test client)

# 3. Check logs
grep "has_samples\|RECEIVED IPC MESSAGE" log.log
```

## Test Node: EchoNode

Location: `python-client/remotemedia/nodes/test_echo.py`

Simple test node that:
- Receives data via IPC
- Logs what it received
- Echoes it back with a counter

Registered automatically in `remotemedia.core.multiprocessing.__init__.py`

## Troubleshooting

### Test won't compile
```bash
# Clean build
cd runtime
cargo clean
cargo test --test test_ipc_communication --features multiprocess
```

### Test hangs
- Default timeout: 15 seconds
- If Python process doesn't respond, test will timeout and fail
- Check process logs in stderr output

### "Access is denied" error
- grpc_server.exe is locked by running server
- Stop the server first, or ignore the warning (tests still compile)
