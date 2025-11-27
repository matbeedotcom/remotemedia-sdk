"""
Standalone test for SimplePyTorchNode - no gRPC, no Rust.
This tests if the node works in pure Python.
"""
import asyncio
import sys
import logging

logging.basicConfig(level=logging.INFO, format='%(levelname)s:%(name)s:%(message)s')
logger = logging.getLogger(__name__)

# Add python-client to path
sys.path.insert(0, 'python-client')

from remotemedia.nodes.simple_pytorch_test import SimplePyTorchNode


# Mock RuntimeData for testing
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


async def test_simple_pytorch():
    """Test SimplePyTorchNode in standalone Python"""
    logger.info("=" * 60)
    logger.info("Test: SimplePyTorchNode in Pure Python (no Rust)")
    logger.info("=" * 60)

    # Create node
    node = SimplePyTorchNode(node_id="test")

    # Initialize
    logger.info("Initializing node...")
    await node.initialize()
    logger.info("Node initialized")

    # Create mock input
    input_data = MockRuntimeData.text("test input")

    # Process
    logger.info("Calling process()...")
    chunk_count = 0

    try:
        async for result in node.process(input_data):
            chunk_count += 1
            logger.info(f"Received chunk {chunk_count}: {list(result.keys())}")
            if '_message' in result:
                logger.info(f"  Message: {result['_message']}")
            if '_test_result' in result:
                logger.info(f"  Result shape: {result['_test_result'].shape}")

        logger.info(f"Processing complete - received {chunk_count} chunks")

    except Exception as e:
        logger.error(f"Error during processing: {e}")
        import traceback
        traceback.print_exc()
        return False

    # Cleanup
    await node.cleanup()

    logger.info("=" * 60)
    if chunk_count > 0:
        logger.info("SUCCESS - PyTorch works in pure Python async context!")
    else:
        logger.info("FAILURE - No chunks received")
    logger.info("=" * 60)

    return chunk_count > 0


if __name__ == "__main__":
    success = asyncio.run(test_simple_pytorch())
    sys.exit(0 if success else 1)
