#!/usr/bin/env python3
"""
Standalone IPC subscriber for integration testing.
This script subscribes to an iceoryx2 channel and prints received messages.
"""

import sys
import time
import iceoryx2 as iox2
import ctypes

def main():
    channel_name = sys.argv[1] if len(sys.argv) > 1 else "test_channel"

    print(f"[PY] Starting subscriber for channel: {channel_name}", flush=True)

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
    print(f"[PY] Subscriber created, waiting for messages...", flush=True)

    # Poll for messages
    timeout_sec = 10
    start_time = time.time()
    received_count = 0

    while time.time() - start_time < timeout_sec:
        sample = subscriber.receive()

        if sample is not None:
            payload_bytes = bytes(sample.payload())
            print(f"[PY] RECEIVED {len(payload_bytes)} bytes", flush=True)

            # Parse simple format: type (1) | session_len (2) | session | timestamp (8) | payload_len (4) | payload
            if len(payload_bytes) >= 15:
                data_type = payload_bytes[0]
                pos = 1
                session_len = int.from_bytes(payload_bytes[pos:pos+2], 'little')
                pos += 2
                session_id = payload_bytes[pos:pos+session_len].decode('utf-8')
                pos += session_len + 8  # skip timestamp
                payload_len = int.from_bytes(payload_bytes[pos:pos+4], 'little')
                pos += 4
                payload = payload_bytes[pos:pos+payload_len]

                if data_type == 3:  # Text
                    text = payload.decode('utf-8')
                    print(f"[PY] Text message: {text}", flush=True)
                elif data_type == 1:  # Audio
                    print(f"[PY] Audio message: {payload_len} bytes", flush=True)

                received_count += 1
                break

        time.sleep(0.01)

    if received_count == 0:
        print(f"[PY] FAIL: No messages received in {timeout_sec}s", flush=True)
        sys.exit(1)
    else:
        print(f"[PY] SUCCESS: Test passed - received {received_count} messages", flush=True)
        sys.exit(0)

if __name__ == "__main__":
    main()
