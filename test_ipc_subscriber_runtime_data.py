#!/usr/bin/env python3
"""
IPC subscriber that uses RuntimeData objects (not raw bytes).
Tests the full Python node IPC receive path.
"""

import sys
import time
import iceoryx2 as iox2
import ctypes

def main():
    channel_name = sys.argv[1] if len(sys.argv) > 1 else "test_channel"

    print(f"[PY] Starting RuntimeData subscriber for channel: {channel_name}", flush=True)

    # Import RuntimeData
    try:
        from remotemedia.core.multiprocessing.data import RuntimeData
        import numpy as np
        print(f"[PY] RuntimeData imported successfully", flush=True)
    except ImportError as e:
        print(f"[PY] FAIL: Cannot import RuntimeData: {e}", flush=True)
        sys.exit(1)

    # Create iceoryx2 node
    iox2.set_log_level_from_env_or(iox2.LogLevel.Warn)
    node = iox2.NodeBuilder.new().create(iox2.ServiceType.Ipc)

    # Open service and create subscriber
    service_name = iox2.ServiceName.new(channel_name)
    print(f"[PY] Opening service: {channel_name}", flush=True)

    service = (
        node.service_builder(service_name)
        .publish_subscribe(iox2.Slice[ctypes.c_uint8])
        .history_size(0)
        .subscriber_max_buffer_size(100)
        .subscriber_max_borrowed_samples(100)
        .open_or_create()
    )

    subscriber = service.subscriber_builder().create()
    print(f"[PY] Subscriber created, polling for RuntimeData...", flush=True)

    # Poll for messages
    timeout_sec = 10
    start_time = time.time()
    received_count = 0

    while time.time() - start_time < timeout_sec:
        sample = subscriber.receive()

        if sample is not None:
            payload_bytes = bytes(sample.payload())
            print(f"[PY] RECEIVED {len(payload_bytes)} bytes via IPC", flush=True)

            # Parse IPC RuntimeData format
            if len(payload_bytes) < 15:
                print(f"[PY] ERROR: Invalid IPC data - too short", flush=True)
                continue

            pos = 0

            # Data type (1 byte)
            data_type = payload_bytes[pos]
            pos += 1

            # Session ID (2 bytes length + data)
            session_len = int.from_bytes(payload_bytes[pos:pos+2], 'little')
            pos += 2
            session_id = payload_bytes[pos:pos+session_len].decode('utf-8')
            pos += session_len

            # Timestamp (8 bytes)
            timestamp = int.from_bytes(payload_bytes[pos:pos+8], 'little')
            pos += 8

            # Payload length (4 bytes)
            payload_len = int.from_bytes(payload_bytes[pos:pos+4], 'little')
            pos += 4
            payload = payload_bytes[pos:pos+payload_len]

            print(f"[PY] Deserialized: type={data_type}, session={session_id}, payload_len={payload_len}", flush=True)

            # Convert to RuntimeData
            if data_type == 1:  # Audio
                audio_samples = np.frombuffer(payload, dtype=np.float32)
                print(f"[PY] Audio RuntimeData: {len(audio_samples)} samples", flush=True)
                received_count += 1
                break

            elif data_type == 3:  # Text
                text = payload.decode('utf-8')
                print(f"[PY] Text RuntimeData: '{text}'", flush=True)
                runtime_data = RuntimeData.text(text)
                print(f"[PY] Created RuntimeData object: {runtime_data}", flush=True)
                print(f"[PY] is_text(): {runtime_data.is_text()}", flush=True)
                print(f"[PY] as_text(): {runtime_data.as_text()}", flush=True)
                received_count += 1
                break

        time.sleep(0.01)

    if received_count == 0:
        print(f"[PY] FAIL: No messages received in {timeout_sec}s", flush=True)
        sys.exit(1)
    else:
        print(f"[PY] SUCCESS: Received and parsed {received_count} RuntimeData messages", flush=True)
        sys.exit(0)

if __name__ == "__main__":
    main()
