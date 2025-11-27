#!/usr/bin/env python3
"""Test numpy marshaling between Python and Rust."""

import numpy as np
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "python-client"))

# Test 1: Import the runtime
print("Test 1: Importing Rust runtime...")
try:
    import remotemedia_runtime
    print("  [OK] Rust runtime imported")
except ImportError as e:
    print(f"  [FAIL] Could not import: {e}")
    sys.exit(1)

# Test 2: Create a numpy array and marshal it
print("\nTest 2: Creating numpy array...")
audio_data = np.random.randn(1, 32000).astype(np.float32)  # 2 seconds at 16kHz
sample_rate = 16000
print(f"  Created array: shape={audio_data.shape}, dtype={audio_data.dtype}")

# Test 3: Create a simple manifest
print("\nTest 3: Creating manifest...")
manifest = {
    "nodes": [
        {
            "id": "whisper",
            "type": "RustWhisperTranscriber",
            "params": {
                "model_source": "tiny",
                "language": "en",
                "n_threads": 4
            }
        }
    ],
    "edges": []
}
print(f"  Manifest created with {len(manifest['nodes'])} nodes")

# Test 4: Create executor (this will trigger model download)
print("\nTest 4: Creating Rust executor (this may download the model)...")
print("  Note: First run may take a while to download the tiny model (~75MB)")
try:
    executor = remotemedia_runtime.create_executor(manifest)
    print("  [OK] Executor created")
except Exception as e:
    print(f"  [FAIL] Could not create executor: {e}")
    import traceback
    traceback.print_exc()
    sys.exit(1)

# Test 5: Initialize (this triggers model loading)
print("\nTest 5: Initializing executor (loading Whisper model)...")
try:
    import asyncio
    asyncio.run(executor.initialize())
    print("  [OK] Executor initialized, model loaded!")
except Exception as e:
    print(f"  [FAIL] Initialization failed: {e}")
    import traceback
    traceback.print_exc()
    sys.exit(1)

print("\n[SUCCESS] Model initialization completed!")
print("The hang might be during transcription, not initialization.")
