#!/usr/bin/env python3
"""Debug script to list all iceoryx2 services."""

import iceoryx2 as iox2
import ctypes

print("Listing all iceoryx2 IPC services...")
print("=" * 60)

# Create node
iox2.set_log_level_from_env_or(iox2.LogLevel.Info)
node = iox2.NodeBuilder.new().create(iox2.ServiceType.Ipc)

# List all services
print(f"\nNode created: {node}")
print(f"Node methods: {[m for m in dir(node) if not m.startswith('_')]}")

# Try to find a method to list services
if hasattr(node, 'list'):
    try:
        services = list(node.list(iox2.Config.global_config(), iox2.ServiceType.Ipc))
        print(f"\nFound {len(services)} services:")
        for svc in services:
            print(f"  - Service: {svc}")
    except Exception as e:
        print(f"\nError listing services via node.list(): {e}")
elif hasattr(iox2, 'list_services'):
    services = iox2.list_services(iox2.ServiceType.Ipc)
    print(f"\nFound {len(services)} services:")
    for svc in services:
        print(f"  - {svc}")
else:
    print("\nNo list_services() method found in iceoryx2")

# Try to open known service names continuously
test_names = ["control/lfm2_audio", "lfm2_audio_input", "lfm2_audio_output", "control/vibevoice_tts", "vibevoice_tts_input", "vibevoice_tts_output"]

print(f"\nPolling for services every 2 seconds (Ctrl+C to stop)...")
print("Waiting for pipeline to start...")
print("=" * 60)

import time
try:
    while True:
        found_any = False
        for name in test_names:
            try:
                svc_name = iox2.ServiceName.new(name)
                svc = node.service_builder(svc_name).publish_subscribe(iox2.Slice[ctypes.c_uint8]).open()
                if not found_any:
                    print(f"\n✅ Found services:")
                    found_any = True
                print(f"  ✓ {name}: EXISTS")
                print(f"    - Service ID: {svc.service_id()}")
                stat = svc.static_config()
                print(f"    - Max publishers: {stat.max_publishers()}")
                print(f"    - Max subscribers: {stat.max_subscribers()}")
            except Exception:
                pass  # Service doesn't exist yet
        
        if not found_any:
            print(".", end="", flush=True)
        
        time.sleep(2)
except KeyboardInterrupt:
    print("\n\nStopped polling.")
    print("=" * 60)
