#!/usr/bin/env python3
"""
Python helper for cross-language IPC tests.

This script provides Python-side publishers and subscribers for testing
the Node.js native bindings against the Python iceoryx2 bindings.

Usage:
    python python_helper.py publisher <service_name> <num_messages>
    python python_helper.py subscriber <service_name> <num_messages>
"""

import sys
import time
import struct
import argparse
import ctypes

try:
    import iceoryx2 as iox2
except ImportError:
    print("ERROR: iceoryx2 Python package not installed. Install with: pip install iceoryx2", file=sys.stderr)
    sys.exit(1)


def create_test_audio_payload(sample_num: int, num_samples: int = 480) -> bytes:
    """
    Create a test audio payload in RuntimeData wire format.

    Wire format:
    - type (1 byte): 1 = Audio
    - session_len (2 bytes LE): length of session_id
    - session_id (session_len bytes): session ID string
    - timestamp (8 bytes LE): timestamp in nanoseconds
    - sample_rate (4 bytes LE): audio sample rate
    - channels (2 bytes LE): number of channels
    - num_samples (8 bytes LE): number of samples
    - samples (num_samples * 4 bytes): f32 audio samples
    """
    session_id = b"test_session_01"
    timestamp_ns = int(time.time() * 1e9)
    sample_rate = 16000
    channels = 1

    # Generate simple sine wave samples
    import math
    samples = []
    for i in range(num_samples):
        t = (sample_num * num_samples + i) / sample_rate
        sample = math.sin(2 * math.pi * 440 * t) * 0.5  # 440 Hz tone at 50% amplitude
        samples.append(sample)

    # Build header
    header = bytearray()
    header.append(1)  # type = Audio
    header.extend(struct.pack('<H', len(session_id)))  # session_len
    header.extend(session_id)  # session_id
    header.extend(struct.pack('<Q', timestamp_ns))  # timestamp_ns
    header.extend(struct.pack('<I', sample_rate))  # sample_rate
    header.extend(struct.pack('<H', channels))  # channels
    header.extend(struct.pack('<Q', num_samples))  # num_samples

    # Add samples
    for sample in samples:
        header.extend(struct.pack('<f', sample))

    return bytes(header)


def run_publisher(service_name: str, num_messages: int, delay_ms: int = 10):
    """Run a Python publisher that sends test audio data."""
    print(f"Starting Python publisher on service '{service_name}'", file=sys.stderr)

    # Set log level
    iox2.set_log_level_from_env_or(iox2.LogLevel.Info)

    # Create iceoryx2 node
    node = iox2.NodeBuilder.new().create(iox2.ServiceType.Ipc)

    # Create service using Slice[c_uint8] (same as runner.py)
    service = node.service_builder(
        iox2.ServiceName.new(service_name)
    ).publish_subscribe(iox2.Slice[ctypes.c_uint8]).open_or_create()

    # Create publisher with initial slice len and allocation strategy
    publisher = (
        service.publisher_builder()
        .initial_max_slice_len(1048576)
        .allocation_strategy(iox2.AllocationStrategy.PowerOfTwo)
        .create()
    )

    print(f"Publisher ready, sending {num_messages} messages", file=sys.stderr)

    sent_count = 0
    for i in range(num_messages):
        payload = create_test_audio_payload(i)

        # Loan, write, send
        sample = publisher.loan_slice_uninit(len(payload))
        for j, byte_val in enumerate(payload):
            sample.payload()[j] = byte_val
        sample = sample.assume_init()
        sample.send()

        sent_count += 1
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)

    print(f"DONE: Sent {sent_count} messages", file=sys.stderr)
    print(f"SENT:{sent_count}")  # Machine-readable output
    sys.stdout.flush()


def run_subscriber(service_name: str, num_messages: int, timeout_sec: float = 10.0):
    """Run a Python subscriber that receives test data."""
    print(f"Starting Python subscriber on service '{service_name}'", file=sys.stderr)

    # Set log level
    iox2.set_log_level_from_env_or(iox2.LogLevel.Info)

    # Create iceoryx2 node
    node = iox2.NodeBuilder.new().create(iox2.ServiceType.Ipc)

    # Open existing service (created by Node.js publisher) using Slice[c_uint8]
    service = node.service_builder(
        iox2.ServiceName.new(service_name)
    ).publish_subscribe(iox2.Slice[ctypes.c_uint8]).open_or_create()

    # Create subscriber - use default buffer to avoid exceeding service limits
    subscriber = service.subscriber_builder().create()

    print(f"Subscriber ready, waiting for {num_messages} messages", file=sys.stderr)

    received_count = 0
    start_time = time.time()

    while received_count < num_messages:
        if time.time() - start_time > timeout_sec:
            print(f"TIMEOUT: Received {received_count}/{num_messages} messages", file=sys.stderr)
            break

        sample = subscriber.receive()
        if sample is not None:
            payload = bytes(sample.payload())
            received_count += 1

            # Validate it's audio (type byte = 1)
            if len(payload) > 0 and payload[0] == 1:
                print(f"Received audio message {received_count}: {len(payload)} bytes", file=sys.stderr)
            else:
                print(f"Received message {received_count}: {len(payload)} bytes (type={payload[0] if payload else 'empty'})", file=sys.stderr)
        else:
            time.sleep(0.001)  # Small sleep to avoid busy-waiting

    print(f"DONE: Received {received_count} messages", file=sys.stderr)
    print(f"RECEIVED:{received_count}")  # Machine-readable output
    sys.stdout.flush()


def main():
    parser = argparse.ArgumentParser(description='Python helper for cross-language IPC tests')
    parser.add_argument('mode', choices=['publisher', 'subscriber'], help='Run as publisher or subscriber')
    parser.add_argument('service_name', help='iceoryx2 service name')
    parser.add_argument('num_messages', type=int, help='Number of messages to send/receive')
    parser.add_argument('--delay-ms', type=int, default=10, help='Delay between publishes (ms)')
    parser.add_argument('--timeout-sec', type=float, default=10.0, help='Subscriber timeout (seconds)')

    args = parser.parse_args()

    try:
        if args.mode == 'publisher':
            run_publisher(args.service_name, args.num_messages, args.delay_ms)
        else:
            run_subscriber(args.service_name, args.num_messages, args.timeout_sec)
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    main()
