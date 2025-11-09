#!/usr/bin/env python3
"""
Test script to verify audio nodes with runtime_hint parameter.
"""

import sys
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "python-client"))

from remotemedia.nodes.audio import AudioResampleNode, VADNode, FormatConverterNode


def test_audio_resample_node():
    """Test AudioResampleNode with different runtime hints."""
    print("Testing AudioResampleNode...")
    
    # Test with auto (should work regardless of Rust availability)
    node = AudioResampleNode(target_sample_rate=16000, runtime_hint="auto", name="resample_auto")
    print(f"  - Created node with runtime_hint='auto'")
    
    # Create test audio data (2 channels, 44100 Hz)
    audio_data = np.random.randn(2, 44100).astype(np.float32)
    sample_rate = 44100
    
    result = node.process((audio_data, sample_rate))
    
    if isinstance(result, tuple) and len(result) == 2:
        resampled_audio, output_sr = result
        print(f"  - Processed: {audio_data.shape} @ {sample_rate}Hz -> {resampled_audio.shape} @ {output_sr}Hz")
        print(f"  ✓ AudioResampleNode works!")
    else:
        print(f"  ✗ Unexpected result: {result}")
        return False
    
    # Test with explicit python
    node_py = AudioResampleNode(target_sample_rate=16000, runtime_hint="python", name="resample_python")
    result_py = node_py.process((audio_data, sample_rate))
    print(f"  - Python runtime works: {isinstance(result_py, tuple)}")
    
    # Test with explicit rust (may warn if not available)
    node_rust = AudioResampleNode(target_sample_rate=16000, runtime_hint="rust", name="resample_rust")
    result_rust = node_rust.process((audio_data, sample_rate))
    print(f"  - Rust runtime hint accepted: {isinstance(result_rust, tuple)}")
    
    return True


def test_vad_node():
    """Test VADNode with different runtime hints."""
    print("\nTesting VADNode...")
    
    # Test with auto
    node = VADNode(frame_duration_ms=30, energy_threshold=0.02, runtime_hint="auto", name="vad_auto")
    print(f"  - Created node with runtime_hint='auto'")
    
    # Create test audio data
    audio_data = np.random.randn(1, 16000).astype(np.float32) * 0.1
    sample_rate = 16000
    
    result = node.process((audio_data, sample_rate))
    
    if isinstance(result, tuple) and len(result) == 3:
        output_audio, output_sr, vad_results = result
        print(f"  - VAD results: {vad_results}")
        print(f"  ✓ VADNode works!")
    else:
        print(f"  ✗ Unexpected result: {result}")
        return False
    
    # Test with python
    node_py = VADNode(runtime_hint="python", name="vad_python")
    result_py = node_py.process((audio_data, sample_rate))
    print(f"  - Python runtime works: {isinstance(result_py, tuple)}")
    
    return True


def test_format_converter_node():
    """Test FormatConverterNode with different runtime hints."""
    print("\nTesting FormatConverterNode...")
    
    # Test f32 to i16
    node = FormatConverterNode(target_format="i16", runtime_hint="auto", name="converter_auto")
    print(f"  - Created node with runtime_hint='auto'")
    
    # Create test audio data (f32)
    audio_data = np.random.randn(2, 1000).astype(np.float32) * 0.5
    sample_rate = 16000
    
    result = node.process((audio_data, sample_rate))
    
    if isinstance(result, tuple) and len(result) == 2:
        converted_audio, output_sr = result
        print(f"  - Converted: {audio_data.dtype} -> {converted_audio.dtype}")
        print(f"  ✓ FormatConverterNode works!")
    else:
        print(f"  ✗ Unexpected result: {result}")
        return False
    
    # Test with python
    node_py = FormatConverterNode(target_format="f32", runtime_hint="python", name="converter_python")
    i16_data = np.random.randint(-32768, 32767, size=(2, 1000), dtype=np.int16)
    result_py = node_py.process((i16_data, sample_rate))
    print(f"  - Python runtime works: {isinstance(result_py, tuple)}")
    
    return True


def main():
    """Run all tests."""
    print("=" * 70)
    print("Testing Audio Nodes with runtime_hint parameter")
    print("=" * 70)
    print()
    
    # Check Rust availability
    try:
        import remotemedia_runtime
        print(f"[INFO] Rust runtime available: version {remotemedia_runtime.__version__}")
        print(f"[INFO] Nodes will use Rust acceleration when runtime_hint='auto' or 'rust'")
    except ImportError:
        print("[INFO] Rust runtime not available")
        print("[INFO] Nodes will use Python fallback")
    print()
    
    all_passed = True
    
    all_passed &= test_audio_resample_node()
    all_passed &= test_vad_node()
    all_passed &= test_format_converter_node()
    
    print()
    print("=" * 70)
    if all_passed:
        print("✓ All tests passed!")
    else:
        print("✗ Some tests failed")
    print("=" * 70)
    
    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
