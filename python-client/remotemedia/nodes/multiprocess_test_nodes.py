"""
Test nodes for multiprocess integration testing.

This module provides simple node implementations for testing the multiprocess
infrastructure without depending on heavy ML models.
"""

import logging
from typing import Any
from remotemedia.core.multiprocessing import MultiprocessNode, register_node

logger = logging.getLogger(__name__)


@register_node("test_processor")
class TestProcessorNode(MultiprocessNode):
    """
    Simple test processor node for integration testing.

    This node receives data and echoes it back with a prefix,
    useful for testing IPC communication.
    """

    def __init__(self, node_id=None, config=None, **kwargs):
        # Handle the specific pattern from runner.py
        if node_id is not None and config is not None:
            super().__init__(config=config, **kwargs)
        elif config is not None:
            super().__init__(config=config, **kwargs)
        elif node_id is not None:
            config = {
                'node_id': node_id,
                'node_type': 'test_processor',
                'params': {},
                'session_id': kwargs.get('session_id')
            }
            super().__init__(config=config, **kwargs)
        else:
            raise ValueError("Either node_id or config must be provided")

        self.process_count = 0
        logger.info(f"TestProcessorNode {self.node_id} initialized")

    async def initialize(self) -> None:
        """Initialize the test processor node."""
        await super().initialize()
        self.process_count = 0
        logger.info(f"TestProcessorNode {self.node_id} ready")

    async def process(self, data: Any) -> Any:
        """
        Process the received data.

        Args:
            data: Input RuntimeData

        Returns:
            Processed RuntimeData
        """
        self.process_count += 1
        logger.info(f"TestProcessorNode received data (#{self.process_count}): {data}")

        # Echo back the data
        if data.is_text():
            text = data.as_text()
            response_text = f"[test_processor #{self.process_count}] {text}"
            logger.info(f"Responding with: {response_text}")

            try:
                from remotemedia.core.multiprocessing.data import RuntimeData
                return RuntimeData.text(response_text)
            except ImportError:
                logger.error("RuntimeData not available")
                return None

        elif data.is_audio():
            samples_bytes, sample_rate, channels, format_str, num_samples = data.as_audio()
            logger.info(f"Processing audio: {num_samples} samples @ {sample_rate}Hz, {channels}ch")
            # Echo back the audio unchanged
            return data

        else:
            logger.warning(f"Unsupported data type: {data.data_type()}")
            return data

    async def cleanup(self) -> None:
        """Cleanup resources."""
        logger.info(f"TestProcessorNode {self.node_id} cleanup (processed {self.process_count} messages)")
        await super().cleanup()


@register_node("passthrough")
class PassthroughNode(MultiprocessNode):
    """
    Ultra-simple passthrough node that just returns the input unchanged.
    
    Useful for testing node chaining and IPC overhead.
    """

    def __init__(self, node_id=None, config=None, **kwargs):
        if node_id is not None and config is not None:
            super().__init__(config=config, **kwargs)
        elif config is not None:
            super().__init__(config=config, **kwargs)
        elif node_id is not None:
            config = {
                'node_id': node_id,
                'node_type': 'passthrough',
                'params': {},
                'session_id': kwargs.get('session_id')
            }
            super().__init__(config=config, **kwargs)
        else:
            raise ValueError("Either node_id or config must be provided")

        logger.info(f"PassthroughNode {self.node_id} initialized")

    async def initialize(self) -> None:
        """Initialize the passthrough node."""
        await super().initialize()
        logger.info(f"PassthroughNode {self.node_id} ready")

    async def process(self, data: Any) -> Any:
        """Pass through data unchanged."""
        return data

    async def cleanup(self) -> None:
        """Cleanup resources."""
        await super().cleanup()

