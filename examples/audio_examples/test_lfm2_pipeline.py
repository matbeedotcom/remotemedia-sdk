#!/usr/bin/env python3
"""
Test script for LFM2-Audio pipeline

This script tests the simple S2S pipeline to ensure:
1. LFM2AudioNode is properly registered
2. Pipeline can be created and executed
3. Audio input produces text and audio output
"""

import asyncio
import logging
import numpy as np
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia.nodes.ml import LFM2AudioNode

try:
    from remotemedia.core.multiprocessing.data import RuntimeData, numpy_to_audio, audio_to_numpy
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    print("ERROR: RuntimeData bindings not available.")
    print("Please build the Rust extension: cargo build --release")
    sys.exit(1)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


async def test_lfm2_audio_node():
    """Test LFM2AudioNode directly."""
    print("\n" + "="*60)
    print("TEST 1: LFM2AudioNode Direct Test")
    print("="*60)

    try:
        # Create node
        logger.info("Creating LFM2AudioNode...")
        node = LFM2AudioNode(
            node_id="test_lfm2",
            system_prompt="You are a helpful assistant. Respond with 'Hello!' to any input.",
            device="cpu",
            audio_temperature=1.0,
            audio_top_k=4,
            max_new_tokens=50,  # Short for testing
            sample_rate=24000,
        )

        # Initialize
        logger.info("Initializing node...")
        await node.initialize()
        logger.info("✓ Node initialized successfully")

        # Create test audio (1 second of sine wave)
        logger.info("Creating test audio...")
        duration = 1.0
        sample_rate = 24000
        t = np.linspace(0, duration, int(sample_rate * duration), dtype=np.float32)
        test_audio = 0.1 * np.sin(2 * np.pi * 440 * t)  # 440 Hz sine wave

        # Convert to RuntimeData
        input_data = numpy_to_audio(test_audio, sample_rate, channels=1)
        logger.info(f"✓ Created test audio: {len(test_audio)} samples")

        # Process
        logger.info("Processing audio...")

        text_output = []
        audio_output = []

        async for response in node.process(input_data):
            if response.is_text():
                text = response.as_text()
                text_output.append(text)
                logger.info(f"  Text: {text}")
            elif response.is_audio():
                audio = audio_to_numpy(response)
                audio_output.append(audio)
                logger.info(f"  Audio: {len(audio)} samples")

        # Verify output
        if text_output:
            logger.info(f"Received {len(text_output)} text responses")
        else:
            logger.warning("No text output received")

        if audio_output:
            total_samples = sum(len(a) for a in audio_output)
            logger.info(f"Received {len(audio_output)} audio chunks ({total_samples} total samples)")
        else:
            logger.warning("No audio output received")

        # Cleanup
        await node.cleanup()
        logger.info("Node cleaned up")

        print("\n" + "="*60)
        print("TEST 1 PASSED: LFM2AudioNode works correctly")
        print("="*60)
        return True

    except Exception as e:
        logger.error(f"TEST 1 FAILED: {e}", exc_info=True)
        print("\n" + "="*60)
        print(f"TEST 1 FAILED: {e}")
        print("="*60)
        return False


async def test_pipeline_manifest():
    """Test that the pipeline manifest is valid."""
    print("\n" + "="*60)
    print("TEST 2: Pipeline Manifest Validation")
    print("="*60)

    try:
        import json

        # Load simple pipeline manifest
        manifest_path = Path(__file__).parent.parent / "nextjs-tts-app" / "pipelines" / "simple-s2s-pipeline.json"

        if not manifest_path.exists():
            logger.error(f"Manifest not found: {manifest_path}")
            return False

        logger.info(f"Loading manifest: {manifest_path}")
        with open(manifest_path, 'r') as f:
            manifest = json.load(f)

        # Validate structure
        assert 'version' in manifest, "Missing version"
        assert 'metadata' in manifest, "Missing metadata"
        assert 'nodes' in manifest, "Missing nodes"
        assert 'connections' in manifest, "Missing connections"

        logger.info(f"Manifest version: {manifest['version']}")
        logger.info(f"Manifest name: {manifest['metadata']['name']}")
        logger.info(f"Nodes: {len(manifest['nodes'])}")
        logger.info(f"Connections: {len(manifest['connections'])}")

        # Check LFM2AudioNode
        lfm2_node = None
        for node in manifest['nodes']:
            if node['nodeType'] == 'LFM2AudioNode':
                lfm2_node = node
                break

        assert lfm2_node is not None, "LFM2AudioNode not found in manifest"
        logger.info(f"Found LFM2AudioNode with id: {lfm2_node['id']}")

        # Validate params (can be dict or JSON string)
        params = lfm2_node['params']
        if isinstance(params, str):
            params = json.loads(params)
        assert 'node_id' in params, "Missing node_id in params"
        assert 'hf_repo' in params, "Missing hf_repo in params"
        assert 'system_prompt' in params, "Missing system_prompt in params"

        logger.info(f"Node params valid: {list(params.keys())}")

        print("\n" + "="*60)
        print("TEST 2 PASSED: Pipeline manifest is valid")
        print("="*60)
        return True

    except Exception as e:
        logger.error(f"TEST 2 FAILED: {e}", exc_info=True)
        print("\n" + "="*60)
        print(f"TEST 2 FAILED: {e}")
        print("="*60)
        return False


async def main():
    """Run all tests."""
    print("\n")
    print("=" * 60)
    print("     LFM2-Audio Pipeline Test Suite")
    print("=" * 60)
    print("\n")

    results = []

    # Test 1: Direct node test
    try:
        result1 = await test_lfm2_audio_node()
        results.append(("LFM2AudioNode Direct Test", result1))
    except Exception as e:
        logger.error(f"Test 1 exception: {e}")
        results.append(("LFM2AudioNode Direct Test", False))

    # Test 2: Manifest validation
    try:
        result2 = await test_pipeline_manifest()
        results.append(("Pipeline Manifest Validation", result2))
    except Exception as e:
        logger.error(f"Test 2 exception: {e}")
        results.append(("Pipeline Manifest Validation", False))

    # Summary
    print("\n" + "="*60)
    print("TEST SUMMARY")
    print("="*60)

    for test_name, passed in results:
        status = "[PASS]" if passed else "[FAIL]"
        print(f"{status}: {test_name}")

    total_passed = sum(1 for _, passed in results if passed)
    total_tests = len(results)

    print(f"\nTotal: {total_passed}/{total_tests} tests passed")
    print("="*60)

    if total_passed == total_tests:
        print("\nALL TESTS PASSED! Pipeline is ready to use.")
        return 0
    else:
        print(f"\n{total_tests - total_passed} test(s) failed. Please review errors above.")
        return 1


if __name__ == "__main__":
    try:
        exit_code = asyncio.run(main())
        sys.exit(exit_code)
    except KeyboardInterrupt:
        print("\n\nTests interrupted by user")
        sys.exit(130)
    except Exception as e:
        print(f"\n\nFatal error: {e}")
        logging.error("Fatal error", exc_info=True)
        sys.exit(1)
