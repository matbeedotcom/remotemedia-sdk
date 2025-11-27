"""
Standalone test for Kokoro TTS to debug the async generator behavior.
This tests the TTS node without any Rust runtime interaction.
"""
import asyncio
import sys
import logging

# Configure logging
logging.basicConfig(level=logging.INFO, format='%(levelname)s:%(name)s:%(message)s')
logger = logging.getLogger(__name__)


# Mock RuntimeData for testing without Rust bindings
class MockRuntimeData:
    def __init__(self, data_type, content):
        self._type = data_type
        self._content = content

    def is_text(self):
        return self._type == "text"

    def as_text(self):
        return self._content

    @staticmethod
    def text(content):
        return MockRuntimeData("text", content)


def mock_numpy_to_audio(audio_array, sample_rate, channels):
    """Mock converter that wraps numpy array as audio RuntimeData."""
    return MockRuntimeData("audio", {
        "array": audio_array,
        "sample_rate": sample_rate,
        "channels": channels
    })


def mock_audio_to_numpy(audio_data):
    """Mock converter that extracts numpy array from audio RuntimeData."""
    return audio_data._content["array"]


# Patch the imports in the TTS module
sys.path.insert(0, 'python-client')
import remotemedia.nodes.tts as tts_module

# Monkey-patch the RuntimeData functions
tts_module.RuntimeData = MockRuntimeData
tts_module.numpy_to_audio = mock_numpy_to_audio
tts_module.audio_to_numpy = mock_audio_to_numpy
tts_module.RUNTIME_DATA_AVAILABLE = True

from remotemedia.nodes.tts import KokoroTTSNode


async def test_basic_synthesis():
    """Test basic text-to-speech synthesis."""
    logger.info("=" * 60)
    logger.info("Test 1: Basic Synthesis")
    logger.info("=" * 60)

    # Create TTS node
    tts_node = KokoroTTSNode(
        node_id="kokoro_tts_test",
        lang_code="a",
        voice="af_heart",
        speed=1.0,
        sample_rate=24000,
        stream_chunks=True
    )

    # Initialize
    logger.info("Initializing TTS node...")
    await tts_node.initialize()
    logger.info("TTS node initialized successfully")

    # Create input text
    input_text = MockRuntimeData.text("test")

    # Process and collect audio chunks
    logger.info("Processing input text...")
    chunk_count = 0

    try:
        async for audio_chunk in tts_node.process(input_text):
            chunk_count += 1
            audio_array = mock_audio_to_numpy(audio_chunk)
            logger.info(f"Received chunk {chunk_count}: {len(audio_array)} samples")

        logger.info(f"Successfully received {chunk_count} chunks")

    except Exception as e:
        logger.error(f"Error during processing: {type(e).__name__}: {e}")
        import traceback
        traceback.print_exc()
        raise

    # Cleanup
    await tts_node.cleanup()
    logger.info("Test completed successfully")


async def test_event_loop_behavior():
    """Test how the async generator behaves with event loops."""
    logger.info("=" * 60)
    logger.info("Test 2: Event Loop Behavior")
    logger.info("=" * 60)

    tts_node = KokoroTTSNode(
        node_id="kokoro_tts_test2",
        lang_code="a",
        voice="af_heart",
        speed=1.0,
        sample_rate=24000,
        stream_chunks=True
    )

    await tts_node.initialize()

    input_text = MockRuntimeData.text("Hello world")

    # Get the async generator
    logger.info("Getting async generator from process()...")
    async_gen = tts_node.process(input_text)

    logger.info(f"Generator type: {type(async_gen).__name__}")
    logger.info("Attempting to get first item using anext()...")

    try:
        first_item = await anext(async_gen)
        audio_array = mock_audio_to_numpy(first_item)
        logger.info(f"Got first item: {len(audio_array)} samples")

        # Try to get remaining items
        remaining = 0
        async for item in async_gen:
            remaining += 1
            audio_array = mock_audio_to_numpy(item)
            logger.info(f"Got additional item {remaining}: {len(audio_array)} samples")

        logger.info(f"Total items: 1 + {remaining} = {remaining + 1}")

    except StopAsyncIteration:
        logger.info("Generator was empty")
    except Exception as e:
        logger.error(f"Error: {type(e).__name__}: {e}")
        import traceback
        traceback.print_exc()
        raise

    await tts_node.cleanup()
    logger.info("Test completed")


async def main():
    """Run all tests."""
    try:
        await test_basic_synthesis()
        print()
        await test_event_loop_behavior()

    except Exception as e:
        logger.error(f"Test failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    logger.info("Starting Kokoro TTS standalone tests...")
    asyncio.run(main())
    logger.info("All tests completed successfully!")
